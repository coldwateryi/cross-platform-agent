use tokio::net::TcpListener;
use tracing::info;

use cross_platform_agent::{Config, Runtime, serve};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let config = Config::from_env()?;
    let runtime = Runtime::new(config.clone());
    let bind_address = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&bind_address).await?;
    let local_addr = listener.local_addr()?;

    info!("agent listening on http://{}", local_addr);
    info!("websocket endpoint ws://{}{}", local_addr, config.ws_path);
    info!("allowed roots: {:?}", config.allowed_roots);

    serve(listener, runtime, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;

    Ok(())
}
