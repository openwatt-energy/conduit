use crate::config::LimitsConfig;
use crate::mqtt::protocol::*;
use crate::parser::{ParsedReading, TelemetryPacket};
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthFailure {
    pub timestamp: String,
    pub username: String,
    pub client_id: String,
    pub ip: String,
}

#[derive(Clone)]
pub struct ServerStats {
    pub connections: Arc<std::sync::atomic::AtomicUsize>,
    pub auth_failures: Arc<std::sync::atomic::AtomicUsize>,
    pub messages_processed: Arc<std::sync::atomic::AtomicUsize>,
    pub parse_errors: Arc<std::sync::atomic::AtomicUsize>,
    pub recent_auth_failures: Arc<Mutex<Vec<AuthFailure>>>,
}

const MAX_RECENT_FAILURES: usize = 50;

impl ServerStats {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            auth_failures: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            messages_processed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            parse_errors: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            recent_auth_failures: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record_auth_failure(&self, username: &str, client_id: &str, ip: &str) {
        let mut failures = self.recent_auth_failures.lock().unwrap();
        failures.insert(0, AuthFailure {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            username: username.to_string(),
            client_id: client_id.to_string(),
            ip: ip.to_string(),
        });
        failures.truncate(MAX_RECENT_FAILURES);
    }
}

pub async fn run_tcp_listener(
    addr: std::net::SocketAddr,
    auth: Arc<dyn crate::auth::AuthProvider>,
    influx_tx: mpsc::Sender<ParsedReading>,
    command_subscribers: Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<String, mpsc::UnboundedSender<Packet>>,
        >,
    >,
    stats: ServerStats,
    limits: LimitsConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("MQTT TCP listener on {}", addr);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(limits.max_connections));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let permit = match semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                warn!("connection limit reached, rejecting {peer_addr}");
                drop(stream);
                continue;
            }
        };

        let auth = auth.clone();
        let influx_tx = influx_tx.clone();
        let cmd_subs = command_subscribers.clone();
        let stats = stats.clone();
        let limits = limits.clone();

        tokio::spawn(async move {
            let _permit = permit;
            stats
                .connections
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Err(e) = handle_connection(
                stream,
                auth,
                influx_tx,
                cmd_subs,
                stats.clone(),
                limits,
            )
            .await
            {
                debug!("peer {peer_addr} disconnected: {e}");
            }
            stats
                .connections
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    auth: Arc<dyn crate::auth::AuthProvider>,
    influx_tx: mpsc::Sender<ParsedReading>,
    command_subscribers: Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<String, mpsc::UnboundedSender<Packet>>,
        >,
    >,
    stats: ServerStats,
    limits: LimitsConfig,
) -> Result<(), String> {
    let mut read_buf = BytesMut::with_capacity(4096);

    let connect =
        read_connect_packet(&mut stream, &mut read_buf, limits.connection_timeout_secs)
            .await
            .map_err(|e| format!("CONNECT: {e}"))?;

    let device_id = match auth.authenticate(&connect).await {
        Ok(id) => id,
        Err(_) => {
            let username = connect.username.clone().unwrap_or_default();
            let client_id = connect.client_id.clone();
            let ip = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
            warn!("auth failed: user={username:?} client={client_id} ip={ip}");
            stats.record_auth_failure(&username, &client_id, &ip);
            let _ = send_connack(&mut stream, ConnAckCode::BadUsernameOrPassword).await;
            stats
                .auth_failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err("auth failed".into());
        }
    };

    let _ = send_connack(&mut stream, ConnAckCode::Accepted).await;
    info!("device {device_id} connected");

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<Packet>();
    {
        let mut subs = command_subscribers.write().await;
        subs.insert(device_id.clone(), cmd_tx);
    }

    let keep_alive =
        Duration::from_secs(connect.keep_alive as u64 + limits.keep_alive_grace_secs);

    loop {
        let packet = tokio::select! {
            result = read_packet(&mut stream, &mut read_buf, keep_alive) => {
                match result {
                    Ok(Some(p)) => p,
                    Ok(None) => break,
                    Err(e) => {
                        debug!("read error from {device_id}: {e}");
                        break;
                    }
                }
            }
            Some(cmd) = cmd_rx.recv() => {
                let mut out = BytesMut::new();
                cmd.encode(&mut out);
                let _ = stream.write_all(&out).await;
                continue;
            }
        };

        match packet {
            Packet::Publish(publish) => {
                stats
                    .messages_processed
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                if !auth.check_topic_access(&device_id, &publish.topic, false).allowed {
                    warn!(
                        "ACL denied PUBLISH from {device_id} to {}",
                        publish.topic
                    );
                    continue;
                }

                if publish.topic.contains("/data") || publish.topic.contains("/sys") {
                    let ow_id = auth.resolve_ow_id(&device_id);
                    let _text = String::from_utf8_lossy(&publish.payload).to_string();
                    match crate::parser::parse_telemetry(&TelemetryPacket { ow_id, topic: publish.topic, payload: publish.payload }) {
                        Ok(reading) => {
                            if influx_tx.send(reading).await.is_err() {
                                warn!("influx channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            debug!("parse error from {device_id}: {e}");
                            stats.parse_errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }

                if publish.qos > 0 {
                    if let Some(id) = publish.packet_id {
                        let puback = Packet::PubAck(id);
                        let mut out = BytesMut::new();
                        puback.encode(&mut out);
                        let _ = stream.write_all(&out).await;
                    }
                }
            }
            Packet::Subscribe(sub) => {
                let mut codes = Vec::new();
                for (topic, requested_qos) in &sub.topics {
                    let acl = auth.check_topic_access(&device_id, topic, true);
                    codes.push(if acl.allowed { *requested_qos & 0x03 } else { 0x80 });
                }
                let suback = Packet::SubAck(SubAck {
                    packet_id: sub.packet_id,
                    return_codes: codes,
                });
                let mut out = BytesMut::new();
                suback.encode(&mut out);
                let _ = stream.write_all(&out).await;
            }
            Packet::Unsubscribe(unsub) => {
                let unsuback = Packet::UnsubAck(unsub.packet_id);
                let mut out = BytesMut::new();
                unsuback.encode(&mut out);
                let _ = stream.write_all(&out).await;
            }
            Packet::PingReq => {
                let mut out = BytesMut::new();
                Packet::PingResp.encode(&mut out);
                let _ = stream.write_all(&out).await;
            }
            Packet::Disconnect => {
                debug!("{device_id} disconnected");
                break;
            }
            _ => {}
        }
    }

    {
        let mut subs = command_subscribers.write().await;
        subs.remove(&device_id);
    }

    Ok(())
}

async fn read_connect_packet(
    stream: &mut TcpStream,
    buf: &mut BytesMut,
    timeout_secs: u64,
) -> Result<Connect, String> {
    let dur = tokio::time::Duration::from_secs(timeout_secs);
    loop {
        let n = tokio::time::timeout(dur, stream.read_buf(buf))
            .await
            .map_err(|_| "connect timeout".to_string())?
            .map_err(|e| format!("read: {e}"))?;

        if n == 0 {
            return Err("connection closed before CONNECT".into());
        }

        if let Ok(Some(Packet::Connect(c))) = Packet::decode(buf) {
            return Ok(c);
        }

        if buf.len() > 65536 {
            return Err("CONNECT packet too large".into());
        }
    }
}

async fn send_connack(
    stream: &mut TcpStream,
    code: ConnAckCode,
) -> Result<(), String> {
    let ack = Packet::ConnAck(ConnAck {
        session_present: false,
        return_code: code,
    });
    let mut out = BytesMut::new();
    ack.encode(&mut out);
    stream
        .write_all(&out)
        .await
        .map_err(|e| format!("write: {e}"))
}

async fn read_packet(
    stream: &mut TcpStream,
    buf: &mut BytesMut,
    timeout_dur: tokio::time::Duration,
) -> Result<Option<Packet>, String> {
    loop {
        let n = tokio::time::timeout(timeout_dur, stream.read_buf(buf))
            .await
            .map_err(|_| "read timeout".to_string())?
            .map_err(|e| format!("read: {e}"))?;

        if n == 0 {
            return Ok(None);
        }

        match Packet::decode(buf) {
            Ok(Some(p)) => return Ok(Some(p)),
            Ok(None) => {
                if buf.len() > 65536 {
                    return Err("buffer overflow".into());
                }
                continue;
            }
            Err(_) => return Err("decode error".into()),
        }
    }
}