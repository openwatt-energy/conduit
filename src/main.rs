use conduitd::api;
use conduitd::auth;
use conduitd::config;
use conduitd::influx;
use conduitd::mqtt;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "conduitd=info".into()),
        )
        .init();

    let config = config::Config::from_env()?;
    info!("ow-mqtt v{} starting", env!("CARGO_PKG_VERSION"));

    let auth = Arc::new(auth::CompositeAuth::new(config.auth.salt.clone()));

    // Load initial registry from openwatt.eu if URL is configured
    if !config.registry.url.is_empty() {
        load_registry(&config.registry, &auth).await;
        if let Some(interval) = config.registry.reload_interval_secs {
            let auth = auth.clone();
            let reg_config = config.registry.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(tokio::time::Duration::from_secs(interval));
                loop {
                    tick.tick().await;
                    load_registry(&reg_config, &auth).await;
                }
            });
        }
    }

    let influx = influx::writer::InfluxWriter::new(config.influx.clone());

    let command_subscribers: Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<
                String,
                tokio::sync::mpsc::UnboundedSender<mqtt::protocol::Packet>,
            >,
        >,
    > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

    let stats = mqtt::server::ServerStats::new();

    // Spawn MQTT TLS listener
    {
        let auth = auth.clone();
        let influx_tx = influx.sender();
        let cmd_subs = command_subscribers.clone();
        let stats = stats.clone();
        let limits = config.limits.clone();
        let addr = config.tls_listener.addr;

        tokio::spawn(async move {
            if let Err(e) = mqtt::server::run_tcp_listener(
                addr, auth, influx_tx, cmd_subs, stats, limits,
            )
            .await
            {
                tracing::error!("MQTT listener error: {e}");
            }
        });
    }

    // Spawn plain MQTT listener if enabled
    if let Some(ref plain) = config.plain_listener {
        let auth = auth.clone();
        let influx_tx = influx.sender();
        let cmd_subs = command_subscribers.clone();
        let stats = stats.clone();
        let limits = config.limits.clone();
        let addr = plain.addr;

        tokio::spawn(async move {
            if let Err(e) = mqtt::server::run_tcp_listener(
                addr, auth, influx_tx, cmd_subs, stats, limits,
            )
            .await
            {
                tracing::error!("plain MQTT listener error: {e}");
            }
        });
    }

    // HTTP API
    let api_state = api::AppState {
        stats: stats.clone(),
        influx_flushed: influx.flushed_counter(),
        influx_failed: influx.failed_counter(),
        auth,
        command_subs: command_subscribers,
        registry_config: config.registry,
        admin_token: config.api.admin_token,
        uptime: std::time::Instant::now(),
        tls_enabled: true,
        plain_enabled: config.plain_listener.is_some(),
    };

    let app = api::router(api_state);
    let listener = tokio::net::TcpListener::bind(config.api.addr).await?;
    info!("API listening on {}", config.api.addr);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn load_registry(
    config: &config::RegistryConfig,
    auth: &auth::CompositeAuth,
) {
    let client = reqwest::Client::new();
    match client
        .get(&config.url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                let entries = parse_registry_json(&json);
                info!("loaded {} devices from registry", entries.len());
                auth.update_registry(entries);
            }
        }
        Ok(resp) => {
            tracing::warn!("registry fetch HTTP {}", resp.status());
        }
        Err(e) => {
            tracing::warn!("registry fetch failed: {e}");
        }
    }
}

fn parse_registry_json(json: &serde_json::Value) -> Vec<auth::registry::DeviceEntry> {
    let arr = match json {
        serde_json::Value::Array(a) => a,
        _ => {
            if let Some(a) = json.get("devices").and_then(|v| v.as_array()) {
                a
            } else {
                return Vec::new();
            }
        }
    };

    arr.iter()
        .filter_map(|v| {
            let dongle = v.get("dongle")?.as_str()?.to_string();
            let auth_val = v.get("auth")?.as_str()?.to_string();
            Some(auth::registry::DeviceEntry {
                dongle,
                auth: auth_val,
            })
        })
        .collect()
}