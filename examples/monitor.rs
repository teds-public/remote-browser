use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    let interval = env_or("MONITOR_INTERVAL_MS", 60_000u64);
    let port = env_or("PORT", 5001u16);

    loop {
        ping(port).await;
        tokio::time::sleep(Duration::from_millis(interval)).await;
    }
}

async fn ping(port: u16) {
    let start = Instant::now();
    let result = request_health(port).await;

    match result {
        Ok(body) => println!(
            "ok latency={}ms body={}",
            start.elapsed().as_millis(),
            body.trim()
        ),
        Err(e) => println!("FAIL {}", e),
    }
}

async fn request_health(port: u16) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;

    let request = format!(
        "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        port
    );
    stream.write_all(request.as_bytes()).await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();

    Ok(body)
}
