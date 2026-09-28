pub mod registry;

use async_trait::async_trait;
use crate::mqtt::protocol::Connect;
use base64::Engine;

#[derive(Debug, Clone)]
pub struct AclResult {
    pub allowed: bool,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn authenticate(&self, connect: &Connect) -> Result<String, AuthError>;

    fn check_topic_access(&self, client_id: &str, topic: &str, is_subscribe: bool) -> AclResult;
}

#[derive(Debug, Clone)]
pub enum AuthError {
    InvalidCredentials,
    UnknownDevice,
    ServerError(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidCredentials => write!(f, "invalid credentials"),
            AuthError::UnknownDevice => write!(f, "unknown device"),
            AuthError::ServerError(s) => write!(f, "server error: {s}"),
        }
    }
}

pub struct PasswordAuth {
    salt: String,
    registry: std::sync::RwLock<Vec<registry::DeviceEntry>>,
}

impl PasswordAuth {
    pub fn new(salt: String) -> Self {
        Self {
            salt,
            registry: std::sync::RwLock::new(Vec::new()),
        }
    }

    pub fn update_registry(&self, entries: Vec<registry::DeviceEntry>) {
        *self.registry.write().unwrap() = entries;
    }

    pub fn registry(&self) -> Vec<registry::DeviceEntry> {
        self.registry.read().unwrap().clone()
    }

    fn verify_password(&self, device_id: &str, password: &[u8]) -> bool {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(format!("{}{}", self.salt, device_id));
        let hash = hasher.finalize();
        let expected = base64::engine::general_purpose::STANDARD
            .encode(&hash[..10]);
        let provided = String::from_utf8_lossy(password);
        expected == provided
    }
}

#[async_trait]
impl AuthProvider for PasswordAuth {
    async fn authenticate(&self, connect: &Connect) -> Result<String, AuthError> {
        let username = connect
            .username
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;

        let password = connect
            .password
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;

        let registry = self.registry.read().unwrap();

        let entry = registry
            .iter()
            .find(|e| e.dongle == *username)
            .ok_or(AuthError::UnknownDevice)?;

        if entry.auth != "openwatt_salt_v1" {
            return Err(AuthError::InvalidCredentials);
        }

        if self.verify_password(username, password) {
            Ok(username.clone())
        } else {
            Err(AuthError::InvalidCredentials)
        }
    }

    fn check_topic_access(&self, client_id: &str, topic: &str, _is_subscribe: bool) -> AclResult {
        let allowed = topic_owned_by_client(client_id, topic);
        AclResult { allowed }
    }
}

fn topic_owned_by_client(client_id: &str, topic: &str) -> bool {
    if let Some(serial) = client_id.strip_prefix("P1M5_") {
        if topic.starts_with(&format!("P1M5/{serial}"))
            || topic == &format!("P1M5/{serial}")
        {
            return true;
        }
    }

    // v1.0.47+ firmware format: P1<HEX><HEX><HEX> (e.g., P1855EA8)
    // Publishes to P1M5/<device_id>/data/readings etc.
    if client_id.len() == 8 && client_id.starts_with("P1") {
        if topic.starts_with(&format!("P1M5/{client_id}"))
            || topic == &format!("P1M5/{client_id}")
        {
            return true;
        }
    }

    if let Some(mac) = client_id.strip_prefix("ow_") {
        if topic.starts_with(&format!("BEPR/{mac}"))
            || topic == &format!("BEPR/{mac}")
        {
            return true;
        }
    }

    if client_id == "control" {
        return true;
    }

    false
}

pub struct CompositeAuth {
    password: PasswordAuth,
}

impl CompositeAuth {
    pub fn new(salt: String) -> Self {
        Self {
            password: PasswordAuth::new(salt),
        }
    }

    pub fn update_registry(&self, entries: Vec<registry::DeviceEntry>) {
        self.password.update_registry(entries);
    }

    pub fn registry(&self) -> Vec<registry::DeviceEntry> {
        self.password.registry()
    }
}

#[async_trait]
impl AuthProvider for CompositeAuth {
    async fn authenticate(&self, connect: &Connect) -> Result<String, AuthError> {
        self.password.authenticate(connect).await
    }

    fn check_topic_access(&self, client_id: &str, topic: &str, is_subscribe: bool) -> AclResult {
        self.password.check_topic_access(client_id, topic, is_subscribe)
    }
}