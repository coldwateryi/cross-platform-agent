use std::future::Future;
use axum::extract::Path;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::actions;
use crate::error::{AppError, AppResult};
use crate::models::ExecuteTaskRequest;
use crate::runtime::Runtime;

#[derive(Debug, Default, Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WsClientMessage {
    #[serde(rename = "execute")]
    Execute {
        #[serde(default, alias = "requestId")]
        request_id: Option<String>,
        action: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "get_task", alias = "getTask")]
    GetTask {
        #[serde(default, alias = "requestId")]
        request_id: Option<String>,
        #[serde(alias = "taskId")]
        task_id: String,
    },
    #[serde(rename = "ping")]
    Ping {
        #[serde(default, alias = "requestId")]
        request_id: Option<String>,
    },
}

pub fn build_router(runtime: Runtime) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        .route("/actions", get(list_actions))
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{task_id}", get(get_task))
        .route("/ws", get(websocket_handler))
        .layer(cors)
        .with_state(runtime)
}

pub async fn serve(
    listener: TcpListener,
    runtime: Runtime,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let app = build_router(runtime);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn list_actions(
    State(runtime): State<Runtime>,
    headers: HeaderMap,
    query: Query<AuthQuery>,
) -> AppResult<Json<Value>> {
    authorize(&runtime, &headers, query.0.token.as_deref())?;
    Ok(Json(json!({ "actions": actions::catalog() })))
}

async fn list_tasks(
    State(runtime): State<Runtime>,
    headers: HeaderMap,
    query: Query<AuthQuery>,
) -> AppResult<Json<Value>> {
    authorize(&runtime, &headers, query.0.token.as_deref())?;
    Ok(Json(json!({ "tasks": runtime.list_tasks().await })))
}

async fn get_task(
    State(runtime): State<Runtime>,
    headers: HeaderMap,
    query: Query<AuthQuery>,
    Path(task_id): Path<String>,
) -> AppResult<Json<Value>> {
    authorize(&runtime, &headers, query.0.token.as_deref())?;
    let task_id = parse_uuid(&task_id)?;
    let task = runtime
        .get_task(task_id)
        .await
        .ok_or_else(|| AppError::not_found("Task not found."))?;
    Ok(Json(json!({ "task": task })))
}

async fn create_task(
    State(runtime): State<Runtime>,
    headers: HeaderMap,
    query: Query<AuthQuery>,
    Json(request): Json<ExecuteTaskRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    authorize(&runtime, &headers, query.0.token.as_deref())?;
    let task = runtime.submit_task(request).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "task": task }))))
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(runtime): State<Runtime>,
    headers: HeaderMap,
    query: Query<AuthQuery>,
) -> AppResult<impl IntoResponse> {
    authorize(&runtime, &headers, query.0.token.as_deref())?;
    Ok(ws.on_upgrade(move |socket| websocket_loop(socket, runtime)))
}

async fn websocket_loop(mut socket: WebSocket, runtime: Runtime) {
    let mut updates = runtime.subscribe();
    if send_json(
        &mut socket,
        json!({
            "type": "welcome",
            "actions": actions::catalog(),
        }),
    )
    .await
    .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if handle_ws_message(&mut socket, &runtime, text.to_string()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            update = updates.recv() => {
                match update {
                    Ok(task) => {
                        if send_json(&mut socket, json!({ "type": "task.updated", "task": task })).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn handle_ws_message(
    socket: &mut WebSocket,
    runtime: &Runtime,
    raw: String,
) -> Result<(), ()> {
    match serde_json::from_str::<WsClientMessage>(&raw) {
        Ok(WsClientMessage::Ping { request_id }) => {
            send_json(
                socket,
                json!({
                    "type": "pong",
                    "request_id": request_id,
                }),
            )
            .await
        }
        Ok(WsClientMessage::GetTask { request_id, task_id }) => {
            let response = match parse_uuid(&task_id) {
                Ok(task_id) => {
                    let task = runtime.get_task(task_id).await;
                    json!({
                        "type": "task.snapshot",
                        "request_id": request_id,
                        "task": task,
                    })
                }
                Err(error) => {
                    json!({
                        "type": "error",
                        "request_id": request_id,
                        "error": error.payload(),
                    })
                }
            };
            send_json(socket, response).await
        }
        Ok(WsClientMessage::Execute {
            request_id,
            action,
            params,
        }) => {
            let response = match runtime
                .submit_task(ExecuteTaskRequest { action, params })
                .await
            {
                Ok(task) => json!({
                    "type": "task.accepted",
                    "request_id": request_id,
                    "task": task,
                }),
                Err(error) => json!({
                    "type": "error",
                    "request_id": request_id,
                    "error": error.payload(),
                }),
            };
            send_json(socket, response).await
        }
        Err(error) => {
            send_json(
                socket,
                json!({
                    "type": "error",
                    "error": {
                        "code": "invalid_json",
                        "message": format!("Invalid WebSocket payload: {error}"),
                    },
                }),
            )
            .await
        }
    }
}

async fn send_json(socket: &mut WebSocket, payload: Value) -> Result<(), ()> {
    socket
        .send(Message::Text(payload.to_string().into()))
        .await
        .map_err(|_| ())
}

fn authorize(runtime: &Runtime, headers: &HeaderMap, query_token: Option<&str>) -> AppResult<()> {
    let Some(expected) = runtime.config().api_token.as_deref() else {
        return Ok(());
    };

    if extract_token(headers).as_deref() == Some(expected) || query_token == Some(expected) {
        return Ok(());
    }

    Err(AppError::unauthorized())
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(raw) = value.to_str() {
            if let Some(token) = raw.strip_prefix("Bearer ") {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    headers
        .get("x-api-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_uuid(raw: &str) -> AppResult<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| AppError::validation(format!("Invalid task id: {raw}")))
}
