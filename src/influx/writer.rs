use crate::config::InfluxConfig;
use crate::parser::ParsedReading;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

pub struct InfluxWriter {
    tx: mpsc::Sender<ParsedReading>,
    flushed: Arc<std::sync::atomic::AtomicUsize>,
    failed: Arc<std::sync::atomic::AtomicUsize>,
}

impl InfluxWriter {
    pub fn new(config: InfluxConfig) -> Self {
        let (tx, rx) = mpsc::channel(100_000);
        let flushed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let failed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let f1 = flushed.clone();
        let f2 = failed.clone();

        tokio::spawn(async move {
            let mut buffer: Vec<ParsedReading> = Vec::with_capacity(config.batch_size);
            let client = reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .expect("failed to build InfluxDB client");
            let write_url = format!(
                "{}/api/v2/write?org={}&bucket={}&precision=ns",
                config.url.trim_end_matches('/'),
                config.org,
                config.bucket,
            );
            let mut ticker = tokio::time::interval(Duration::from_millis(config.flush_interval_ms));
            let mut rx = rx;

            loop {
                tokio::select! {
                    biased;
                    reading = rx.recv() => {
                        match reading {
                            Some(r) => {
                                buffer.push(r);
                                if buffer.len() >= config.batch_size {
                                    flush(&mut buffer, &client, &write_url, &config.token, &f1, &f2).await;
                                }
                            }
                            None => {
                                if !buffer.is_empty() {
                                    flush(&mut buffer, &client, &write_url, &config.token, &f1, &f2).await;
                                }
                                break;
                            }
                        }
                    }
                    _ = ticker.tick() => {
                        if !buffer.is_empty() {
                            if buffer.len() > config.max_queue_points {
                                let excess = buffer.len() - config.max_queue_points;
                                buffer.drain(0..excess);
                                warn!("dropped {excess} oldest points (queue overflow)");
                                f2.fetch_add(excess, std::sync::atomic::Ordering::Relaxed);
                            }
                            flush(&mut buffer, &client, &write_url, &config.token, &f1, &f2).await;
                        }
                    }
                }
            }

            info!("InfluxDB writer stopped");
        });

        Self { tx, flushed, failed }
    }

    pub fn sender(&self) -> mpsc::Sender<ParsedReading> {
        self.tx.clone()
    }

    pub fn flushed_counter(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        self.flushed.clone()
    }

    pub fn failed_counter(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        self.failed.clone()
    }
}

async fn flush(
    buffer: &mut Vec<ParsedReading>,
    client: &reqwest::Client,
    url: &str,
    token: &str,
    flushed: &Arc<std::sync::atomic::AtomicUsize>,
    failed: &Arc<std::sync::atomic::AtomicUsize>,
) {
    if buffer.is_empty() {
        return;
    }

    let mut body = String::with_capacity(buffer.len() * 128);
    for r in buffer.iter() {
        body.push_str(&r.to_line_protocol("p1_dongles"));
        body.push('\n');
    }

    let count = buffer.len();
    buffer.clear();

    let mut backoff = 100u64;
    let max_retries = 5;

    for retry in 0..max_retries {
        match client
            .post(url)
            .header("Authorization", format!("Token {token}"))
            .body(body.clone())
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    flushed.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
                    debug!("flushed {count} points");
                    return;
                }
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                error!("InfluxDB HTTP {status}: {text}");
            }
            Err(e) => {
                error!("InfluxDB request error: {e}");
            }
        }

        if retry + 1 < max_retries {
            sleep(Duration::from_millis(backoff)).await;
            backoff = (backoff * 2).min(30_000);
        }
    }

    failed.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
    error!("dropped {count} points after {max_retries} retries");
}