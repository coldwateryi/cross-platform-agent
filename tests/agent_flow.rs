use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use cross_platform_agent::{Config, Runtime, serve};
use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn run_git(args: &[&str], cwd: Option<&Path>) {
    let mut command = Command::new("git");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(directory) = cwd {
        command.current_dir(directory);
    }

    let status = command.status().await.expect("git should start");
    assert!(status.success(), "git command failed: git {}", args.join(" "));
}

async fn run_git_capture(args: &[&str], cwd: Option<&Path>) -> String {
    let mut command = Command::new("git");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = cwd {
        command.current_dir(directory);
    }

    let output = command.output().await.expect("git should start");
    assert!(
        output.status.success(),
        "git command failed: git {}",
        args.join(" ")
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

async fn create_remote_repo(root: &Path) -> PathBuf {
    let bare_repo = root.join("remote.git");
    let seed_repo = root.join("seed");

    run_git(&["init", "--bare", bare_repo.to_str().unwrap()], None).await;
    tokio::fs::create_dir_all(&seed_repo).await.unwrap();
    run_git(&["init", "-b", "main"], Some(&seed_repo)).await;
    run_git(&["config", "user.name", "Agent Test"], Some(&seed_repo)).await;
    run_git(&["config", "user.email", "agent@example.com"], Some(&seed_repo)).await;
    tokio::fs::write(seed_repo.join("README.md"), "# Seed Repo\n")
        .await
        .unwrap();
    run_git(&["add", "README.md"], Some(&seed_repo)).await;
    run_git(&["commit", "-m", "Initial commit"], Some(&seed_repo)).await;
    run_git(
        &["remote", "add", "origin", bare_repo.to_str().unwrap()],
        Some(&seed_repo),
    )
    .await;
    run_git(&["push", "-u", "origin", "main"], Some(&seed_repo)).await;
    run_git(
        &["symbolic-ref", "HEAD", "refs/heads/main"],
        Some(&bare_repo),
    )
    .await;

    bare_repo
}

async fn wait_for_task(client: &reqwest::Client, base_url: &str, token: &str, task_id: &str) -> Value {
    for _ in 0..120 {
        let response = client
            .get(format!("{base_url}/tasks/{task_id}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response.json::<Value>().await.unwrap();
        let status = payload["task"]["status"].as_str().unwrap();
        if status == "succeeded" || status == "failed" {
            return payload["task"].clone();
        }
        sleep(Duration::from_millis(100)).await;
    }

    panic!("timed out waiting for task {task_id}");
}

async fn next_message(
    stream: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    kind: &str,
    request_id: Option<&str>,
) -> Value {
    loop {
        let message = stream.next().await.expect("message expected").expect("valid ws frame");
        if let Message::Text(text) = message {
            let payload: Value = serde_json::from_str(&text).unwrap();
            if payload["type"].as_str() == Some(kind)
                && request_id
                    .map(|value| payload["request_id"].as_str() == Some(value))
                    .unwrap_or(true)
            {
                return payload;
            }
        }
    }
}

async fn wait_for_ws_task(
    stream: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    request_id: &str,
) -> Value {
    let accepted = next_message(stream, "task.accepted", Some(request_id)).await;
    let task_id = accepted["task"]["id"].as_str().unwrap().to_string();

    loop {
        let payload = next_message(stream, "task.updated", None).await;
        if payload["task"]["id"].as_str() == Some(&task_id) {
            let status = payload["task"]["status"].as_str().unwrap_or_default();
            if status == "succeeded" || status == "failed" {
                return payload["task"].clone();
            }
        }
    }
}

#[tokio::test]
async fn rest_and_websocket_execute_git_actions_and_command_run() {
    let temp_dir = TempDir::new().unwrap();
    let allowed_root = temp_dir.path().to_path_buf();
    let remote_repo = create_remote_repo(temp_dir.path()).await;
    let clone_path = temp_dir.path().join("workspace").join("clone");
    let collaborator_path = temp_dir.path().join("workspace").join("collaborator");
    let token = "test-token";

    let config = Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        ws_path: "/ws".to_string(),
        api_token: Some(token.to_string()),
        allowed_roots: vec![allowed_root.clone()],
        command_timeout: Duration::from_secs(30),
        retained_task_limit: 50,
        git_binary: "git".to_string(),
        command_allowed_programs: ["git".to_string()].into_iter().collect(),
    };

    let runtime = Runtime::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base_url = format!("http://{}", address);
    let ws_url = format!("ws://{}/ws?token={token}", address);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server_task = tokio::spawn(serve(listener, runtime, async move {
        let _ = shutdown_rx.await;
    }));

    let client = reqwest::Client::new();

    let create_task = client
        .post(format!("{base_url}/tasks"))
        .bearer_auth(token)
        .json(&json!({
            "action": "git.clone",
            "params": {
                "remote_url": remote_repo,
                "destination_path": clone_path,
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(create_task.status(), StatusCode::ACCEPTED);
    let create_payload = create_task.json::<Value>().await.unwrap();
    let clone_task_id = create_payload["task"]["id"].as_str().unwrap().to_string();
    let clone_task = wait_for_task(&client, &base_url, token, &clone_task_id).await;
    assert_eq!(clone_task["status"].as_str(), Some("succeeded"));
    assert_eq!(
        tokio::fs::read_to_string(clone_path.join("README.md")).await.unwrap(),
        "# Seed Repo\n"
    );

    let (mut ws_stream, _) = connect_async(&ws_url).await.unwrap();
    let welcome = next_message(&mut ws_stream, "welcome", None).await;
    assert!(welcome["actions"].as_array().unwrap().len() >= 10);

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "checkout-1",
                "action": "git.checkout_branch",
                "params": {
                    "repo_path": clone_path,
                    "branch_name": "feature/demo",
                    "create": true,
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let checkout_task = wait_for_ws_task(&mut ws_stream, "checkout-1").await;
    assert_eq!(checkout_task["status"].as_str(), Some("succeeded"));

    run_git(&["config", "user.name", "Agent Test"], Some(&clone_path)).await;
    run_git(&["config", "user.email", "agent@example.com"], Some(&clone_path)).await;
    tokio::fs::write(clone_path.join("feature.txt"), "hello from branch\n")
        .await
        .unwrap();

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "add-1",
                "action": "git.add",
                "params": {
                    "repo_path": clone_path,
                    "paths": ["feature.txt"]
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let add_task = wait_for_ws_task(&mut ws_stream, "add-1").await;
    assert_eq!(add_task["status"].as_str(), Some("succeeded"));

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "commit-1",
                "action": "git.commit",
                "params": {
                    "repo_path": clone_path,
                    "message": "Add feature file"
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let commit_task = wait_for_ws_task(&mut ws_stream, "commit-1").await;
    assert_eq!(commit_task["status"].as_str(), Some("succeeded"));

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "merge-1",
                "action": "git.merge_branch",
                "params": {
                    "repo_path": clone_path,
                    "source_branch": "feature/demo",
                    "target_branch": "main",
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let merge_task = wait_for_ws_task(&mut ws_stream, "merge-1").await;
    assert_eq!(merge_task["status"].as_str(), Some("succeeded"));

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "push-1",
                "action": "git.push",
                "params": {
                    "repo_path": clone_path,
                    "remote": "origin",
                    "branch": "main"
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let push_task = wait_for_ws_task(&mut ws_stream, "push-1").await;
    assert_eq!(push_task["status"].as_str(), Some("succeeded"));
    let local_head = run_git_capture(&["rev-parse", "HEAD"], Some(&clone_path)).await;
    let remote_head = run_git_capture(&["rev-parse", "refs/heads/main"], Some(&remote_repo)).await;
    assert_eq!(local_head, remote_head);

    run_git(&["clone", remote_repo.to_str().unwrap(), collaborator_path.to_str().unwrap()], None).await;
    run_git(&["config", "user.name", "Agent Test"], Some(&collaborator_path)).await;
    run_git(&["config", "user.email", "agent@example.com"], Some(&collaborator_path)).await;
    tokio::fs::write(collaborator_path.join("remote.txt"), "hello from remote\n")
        .await
        .unwrap();
    run_git(&["add", "remote.txt"], Some(&collaborator_path)).await;
    run_git(&["commit", "-m", "Remote update"], Some(&collaborator_path)).await;
    run_git(&["push", "origin", "main"], Some(&collaborator_path)).await;

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "pull-1",
                "action": "git.pull",
                "params": {
                    "repo_path": clone_path,
                    "remote": "origin",
                    "branch": "main",
                    "rebase": false
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let pull_task = wait_for_ws_task(&mut ws_stream, "pull-1").await;
    assert_eq!(pull_task["status"].as_str(), Some("succeeded"));
    assert_eq!(
        tokio::fs::read_to_string(clone_path.join("remote.txt")).await.unwrap(),
        "hello from remote\n"
    );

    ws_stream
        .send(Message::Text(
            json!({
                "type": "execute",
                "request_id": "command-1",
                "action": "command.run",
                "params": {
                    "program": "git",
                    "args": ["rev-parse", "--abbrev-ref", "HEAD"],
                    "cwd": clone_path,
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let command_task = wait_for_ws_task(&mut ws_stream, "command-1").await;

    assert_eq!(
        command_task["result"]["stdout"].as_str().unwrap().trim(),
        "main"
    );
    assert_eq!(
        tokio::fs::read_to_string(clone_path.join("feature.txt")).await.unwrap(),
        "hello from branch\n"
    );

    ws_stream.close(None).await.unwrap();
    let _ = shutdown_tx.send(());
    server_task.await.unwrap().unwrap();
}
