use ai_ide_server::serve;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;

const DEFAULT_PORT: u16 = 3001;
const DEFAULT_HOST: &str = "127.0.0.1";

/// Reads `AI_IDE_HOST` and `AI_IDE_PORT`, falling back to 127.0.0.1:3001.
fn bind_address() -> Result<(IpAddr, u16), String> {
    let host = match std::env::var("AI_IDE_HOST") {
        Ok(h) => h.parse::<IpAddr>().map_err(|e| format!("AI_IDE_HOST '{h}' is not an IP address: {e}"))?,
        Err(_) => DEFAULT_HOST.parse().expect("DEFAULT_HOST is a valid IP address"),
    };
    let port = match std::env::var("AI_IDE_PORT") {
        Ok(p) => p.parse::<u16>().map_err(|e| format!("AI_IDE_PORT '{p}' is not a valid port: {e}"))?,
        Err(_) => DEFAULT_PORT,
    };
    Ok((host, port))
}

/// Resolves when Ctrl+C is pressed, so the port is released cleanly on exit.
async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!("Failed to listen for Ctrl+C: {}", e);
    }
    info!("Shutting down AI-IDE Server");
}

fn fail(message: &str) -> ! {
    tracing::error!("{}", message);
    eprintln!("{}", message);
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let root_path = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => fail(&format!("Cannot read the current directory: {}", e)),
    };

    let (host, port) = match bind_address() {
        Ok(addr) => addr,
        Err(message) => fail(&message),
    };
    let addr = std::net::SocketAddr::new(host, port);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => fail(&format!(
            "Port {port} is already in use. Another AI-IDE server is probably running. \
             Stop it (Get-Process ai-ide-server | Stop-Process) or set AI_IDE_PORT."
        )),
        Err(e) => fail(&format!("Failed to bind {}: {}", addr, e)),
    };

    if let Err(e) = serve(PathBuf::from(&root_path), listener, shutdown_signal()).await {
        fail(&format!("Server stopped with an error: {}", e));
    }
}
