use conduitd::mqtt::protocol::*;
use bytes::BytesMut;
use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = env::args().nth(1).unwrap_or_else(|| "help".into());
    match cmd.as_str() {
        "p1m5" => run_p1m5().await,
        "smart-stuff" => run_smart_stuff().await,
        "load" => run_load().await,
        "auth-negative" => run_auth_negative().await,
        _ => {
            eprintln!("usage: conduit-emulators <p1m5|smart-stuff|load|auth-negative>");
            Ok(())
        }
    }
}

async fn mqtt_connect(
    addr: &str,
    client_id: &str,
    username: &str,
    password: &str,
) -> Result<TcpStream, String> {
    let mut stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("connect: {e}"))?;

    let connect = Packet::Connect(Connect {
        client_id: client_id.to_string(),
        clean_session: true,
        keep_alive: 30,
        username: Some(username.to_string()),
        password: Some(password.as_bytes().to_vec()),
    });

    let mut buf = BytesMut::new();
    connect.encode(&mut buf);
    stream.write_all(&buf).await.map_err(|e| format!("write: {e}"))?;

    let mut read_buf = BytesMut::with_capacity(256);
loop {
        let n = stream
            .read_buf(&mut read_buf)
            .await
            .map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("connection closed".into());
        }
        if let Some(Packet::ConnAck(ack)) =
            Packet::decode(&mut read_buf).map_err(|e| e.to_string())?
        {
            match ack.return_code {
                ConnAckCode::Accepted => return Ok(stream),
                code => return Err(format!("CONNACK rejected: {code:?}")),
            }
        }
    }
}

async fn send_publish(
    stream: &mut TcpStream,
    topic: &str,
    payload: &[u8],
    qos: u8,
) -> Result<(), String> {
    let publish = Packet::Publish(Publish {
        dup: false,
        qos,
        retain: false,
        topic: topic.to_string(),
        packet_id: if qos > 0 { Some(1) } else { None },
        payload: bytes::Bytes::from(payload.to_vec()),
    });
    let mut buf = BytesMut::new();
    publish.encode(&mut buf);
    stream
        .write_all(&buf)
        .await
        .map_err(|e| format!("write: {e}"))?;
    Ok(())
}

fn env_or(key: &str, default: impl Into<String>) -> String {
    env::var(key).unwrap_or_else(|_| default.into())
}

// ---- P1M5 emulator ----

async fn run_p1m5() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env_or("CONDUIT_BROKER", "127.0.0.1:1883");
    let sn = env_or("CONDUIT_DEVICE_SN", "EMU0001");
    let username = env_or("CONDUIT_USERNAME", format!("P1M5_{sn}"));
    let salt = env_or("CONDUIT_SALT", "test_salt");
    let password = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{salt}{username}"),
    );
    let interval_ms: u64 = env_or("CONDUIT_INTERVAL_MS", "10000")
        .parse()
        .unwrap_or(10000);

    let client_id = format!("P1M5_{sn}");
    println!("connecting as {client_id} to {addr}...");
    let mut stream = mqtt_connect(&addr, &client_id, &username, &password).await?;
    println!("connected");

    let mut tick = interval(Duration::from_millis(interval_ms));
    loop {
        tick.tick().await;
        let telegram = build_dsmr_telegram(&sn);
        let topic = format!("P1M5/{sn}/data/readings");
        send_publish(&mut stream, &topic, telegram.as_bytes(), 0).await?;
        println!("PUBLISH {topic} ({telegram_len} bytes)", telegram_len = telegram.len());
    }
}

fn build_dsmr_telegram(sn: &str) -> String {
    format!(
        "/KMP5 ZIV2 E0002\n\n0-0:96.1.255({sn})\n1-0:1.8.1(001234.567*kWh)\n1-0:1.8.2(000123.456*kWh)\n1-0:2.8.1(000012.345*kWh)\n1-0:2.8.2(000001.234*kWh)\n1-0:1.7.0(00.500*kW)\n1-0:2.7.0(00.000*kW)\n1-0:32.7.0(230.0*V)\n1-0:52.7.0(229.5*V)\n1-0:72.7.0(230.8*V)\n1-0:31.7.0(002*A)\n0-1:24.2.1(12345.678*m3)\n!ABCD\n"
    )
}

// ---- Smart Stuff emulator ----

async fn run_smart_stuff() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env_or("CONDUIT_BROKER", "127.0.0.1:1883");
    let mac = env_or("CONDUIT_MAC", "AABBCCDDEE01");
    let username = env_or("CONDUIT_USERNAME", format!("ow_{mac}"));
    let salt = env_or("CONDUIT_SALT", "test_salt");
    let password = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{salt}{username}"),
    );
    let interval_ms: u64 = env_or("CONDUIT_INTERVAL_MS", "10000")
        .parse()
        .unwrap_or(10000);

    let client_id = format!("ow_{mac}");
    println!("connecting as {client_id} to {addr}...");
    let mut stream = mqtt_connect(&addr, &client_id, &username, &password).await?;
    println!("connected");

    let mut tick = interval(Duration::from_millis(interval_ms));
    loop {
        tick.tick().await;
        let payload = serde_json::json!({
            "mac": mac,
            "power_delivered": 0.500,
            "power_received": 0.100,
            "voltage_l1": 230.0,
            "voltage_l2": 229.5,
            "current_l1": 2.0,
            "energy_delivered_t1": 1234.567,
            "energy_delivered_t2": 123.456,
            "temperature": 22.5,
        });
        let topic = format!("BEPR/{mac}/data");
        let body = serde_json::to_vec(&payload).unwrap();
        send_publish(&mut stream, &topic, &body, 0).await?;
        println!("PUBLISH {topic}");
    }
}

// ---- Load emulator ----

async fn run_load() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env_or("CONDUIT_BROKER", "127.0.0.1:1883");
    let num_clients: usize = env_or("CONDUIT_NUM_CLIENTS", "100")
        .parse()
        .unwrap_or(100);
    let rate_hz: f64 = env_or("CONDUIT_RATE_HZ", "1.0")
        .parse()
        .unwrap_or(1.0);
    let salt = env_or("CONDUIT_SALT", "test_salt");

    println!("starting {num_clients} clients at {rate_hz} Hz to {addr}");

    let interval_us = (1_000_000.0 / rate_hz) as u64;
    let mut handles = Vec::new();

    for i in 0..num_clients {
        let addr = addr.clone();
        let salt = salt.clone();
        let handle = tokio::spawn(async move {
            let sn = format!("EMU{i:04X}");
            let client_id = format!("P1M5_{sn}");
            let password = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                format!("{}{}", salt, client_id),
            );

            let mut stream = match mqtt_connect(&addr, &client_id, &client_id, &password).await
            {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[{client_id}] connect failed: {e}");
                    return;
                }
            };

            let telegram = build_dsmr_telegram(&sn);
            let topic = format!("P1M5/{sn}/data/readings");
            let mut tick = interval(Duration::from_micros(interval_us));

            loop {
                tick.tick().await;
                if send_publish(&mut stream, &topic, telegram.as_bytes(), 0)
                    .await
                    .is_err()
                {
                    eprintln!("[{client_id}] disconnected");
                    break;
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

// ---- Auth-negative emulator ----

async fn run_auth_negative() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env_or("CONDUIT_BROKER", "127.0.0.1:1883");

    println!("=== auth-negative tests ===");

    // Test 1: wrong password
    println!("1. wrong password...");
    match mqtt_connect(&addr, "P1M5_WRONG", "P1M5_WRONG", "bad_password").await {
        Ok(_) => println!("   FAIL: expected auth failure"),
        Err(e) => println!("   OK: {e}"),
    }

    // Test 2: missing username
    println!("2. missing username...");
    let mut stream = TcpStream::connect(&addr).await?;
    let connect = Packet::Connect(Connect {
        client_id: "no_user".into(),
        clean_session: true,
        keep_alive: 30,
        username: None,
        password: None,
    });
    let mut buf = BytesMut::new();
    connect.encode(&mut buf);
    stream.write_all(&buf).await?;
    let mut read_buf = BytesMut::with_capacity(256);
    loop {
        stream.read_buf(&mut read_buf).await?;
        if let Some(Packet::ConnAck(ack)) = Packet::decode(&mut read_buf).map_err(|e| e.to_string())? {
            if ack.return_code != ConnAckCode::Accepted {
                println!("   OK: rejected ({:?})", ack.return_code);
            } else {
                println!("   FAIL: accepted with no auth");
            }
            break;
        }
    }

    // Test 3: unauthorized topic
    println!("3. unauthorized topic...");
    let salt = env_or("CONDUIT_SALT", "test_salt");
    let password = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{salt}P1M5_AUTHNEG"),
    );
    let mut stream = match mqtt_connect(&addr, "P1M5_AUTHNEG", "P1M5_AUTHNEG", &password).await
    {
        Ok(s) => s,
        Err(e) => {
            println!("   SKIP: connect failed: {e}");
            return Ok(());
        }
    };
    send_publish(
        &mut stream,
        "P1M5/OTHER_DEVICE/data/readings",
        b"bad topic",
        0,
    )
    .await?;
    println!("   sent (expect broker to drop)");

    println!("=== done ===");
    Ok(())
}