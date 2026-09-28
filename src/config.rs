use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub tls_listener: ListenerConfig,
    pub plain_listener: Option<ListenerConfig>,
    pub auth: AuthConfig,
    pub influx: InfluxConfig,
    pub api: ApiConfig,
    pub registry: RegistryConfig,
    pub limits: LimitsConfig,
}

#[derive(Clone, Debug)]
pub struct ListenerConfig {
    pub addr: SocketAddr,
    pub cert_chain: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
    pub ca_bundle: Option<PathBuf>,
    pub require_client_cert: bool,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub salt: String,
}

#[derive(Clone, Debug)]
pub struct InfluxConfig {
    pub url: String,
    pub token: String,
    pub org: String,
    pub bucket: String,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
    pub max_queue_points: usize,
}

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub addr: SocketAddr,
    pub admin_token: String,
}

#[derive(Clone, Debug)]
pub struct RegistryConfig {
    pub url: String,
    pub api_key: String,
    pub reload_interval_secs: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct LimitsConfig {
    pub max_connections: usize,
    pub max_payload_bytes: usize,
    pub max_topic_depth: usize,
    pub keep_alive_grace_secs: u64,
    pub connection_timeout_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let tls_addr: SocketAddr = env_str("CONDUIT_TLS_ADDR", "0.0.0.0:8883")
            .parse()
            .map_err(|e| format!("CONDUIT_TLS_ADDR: {e}"))?;

        let tls_listener = ListenerConfig {
            addr: tls_addr,
            cert_chain: env_opt("CONDUIT_TLS_CERT_CHAIN").map(PathBuf::from),
            key_file: env_opt("CONDUIT_TLS_KEY_FILE").map(PathBuf::from),
            ca_bundle: env_opt("CONDUIT_TLS_CA_BUNDLE").map(PathBuf::from),
            require_client_cert: env_bool("CONDUIT_TLS_REQUIRE_CLIENT_CERT", false),
        };

        let plain_listener = if env_bool("CONDUIT_PLAIN_ENABLED", false) {
            let addr: SocketAddr = env_str("CONDUIT_PLAIN_ADDR", "0.0.0.0:1883")
                .parse()
                .map_err(|e| format!("CONDUIT_PLAIN_ADDR: {e}"))?;
            Some(ListenerConfig {
                addr,
                cert_chain: None,
                key_file: None,
                ca_bundle: None,
                require_client_cert: false,
            })
        } else {
            None
        };

        let api_addr: SocketAddr = env_str("CONDUIT_API_ADDR", "0.0.0.0:9090")
            .parse()
            .map_err(|e| format!("CONDUIT_API_ADDR: {e}"))?;

        Ok(Self {
            tls_listener,
            plain_listener,
            auth: AuthConfig {
                salt: env_str("CONDUIT_AUTH_SALT", ""),
            },
            influx: InfluxConfig {
                url: env_str("CONDUIT_INFLUX_URL", ""),
                token: env_str("CONDUIT_INFLUX_TOKEN", ""),
                org: env_str("CONDUIT_INFLUX_ORG", ""),
                bucket: env_str("CONDUIT_INFLUX_BUCKET", "openwatt_test"),
                batch_size: env_usize("CONDUIT_INFLUX_BATCH_SIZE", 5000),
                flush_interval_ms: env_u64("CONDUIT_INFLUX_FLUSH_MS", 500),
                max_queue_points: env_usize("CONDUIT_INFLUX_MAX_QUEUE", 500_000),
            },
            api: ApiConfig {
                addr: api_addr,
                admin_token: env_str("CONDUIT_API_ADMIN_TOKEN", ""),
            },
            registry: RegistryConfig {
                url: env_str("CONDUIT_REGISTRY_URL", ""),
                api_key: env_str("CONDUIT_REGISTRY_API_KEY", ""),
                reload_interval_secs: env_opt("CONDUIT_REGISTRY_RELOAD_SECS")
                    .map(|s| s.parse().unwrap_or(300)),
            },
            limits: LimitsConfig {
                max_connections: env_usize("CONDUIT_MAX_CONNECTIONS", 50_000),
                max_payload_bytes: env_usize("CONDUIT_MAX_PAYLOAD_BYTES", 65536),
                max_topic_depth: env_usize("CONDUIT_MAX_TOPIC_DEPTH", 8),
                keep_alive_grace_secs: env_u64("CONDUIT_KEEPALIVE_GRACE_SECS", 30),
                connection_timeout_secs: env_u64("CONDUIT_CONN_TIMEOUT_SECS", 10),
            },
        })
    }
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}