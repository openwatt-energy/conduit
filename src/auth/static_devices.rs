use crate::auth::{AuthError, AuthProvider, AclResult};
use crate::mqtt::protocol::Connect;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::OnceLock;

static BE_DEVICES_JSON: &str = include_str!("../../configs/be-devices.json");

#[derive(Debug, Clone)]
pub struct StaticDeviceAuth {
    pub passwords: HashMap<String, String>,
}

impl StaticDeviceAuth {
    pub fn from_embedded() -> Self {
        let raw: serde_json::Value =
            serde_json::from_str(BE_DEVICES_JSON).expect("failed to parse be-devices.json");
        let mut passwords = HashMap::new();

        if let Some(default_pw) = raw["password"].as_str() {
            if let Some(devices) = raw["devices"].as_array() {
                for d in devices {
                    if let Some(mac) = d.as_str() {
                        passwords.insert(mac.to_string(), default_pw.to_string());
                    }
                }
            }
        }

        if let Some(extra) = raw["extra_users"].as_object() {
            for (user, pw) in extra {
                if let Some(p) = pw.as_str() {
                    passwords.insert(user.clone(), p.to_string());
                }
            }
        }

        StaticDeviceAuth { passwords }
    }
}

#[async_trait]
impl AuthProvider for StaticDeviceAuth {
    async fn authenticate(&self, connect: &Connect) -> Result<String, AuthError> {
        let username = connect.username.clone().unwrap_or_default();

        if let Some(expected_pw) = self.passwords.get(&username) {
            let provided_pw = String::from_utf8_lossy(
                connect.password.as_deref().unwrap_or(b"")
            );
            if provided_pw == *expected_pw {
                return Ok(username);
            }
        }

        Err(AuthError::UnknownDevice)
    }

    fn check_topic_access(&self, client_id: &str, topic: &str, _is_subscribe: bool) -> AclResult {
        // BEPR/ prefix + MAC-based client_id — same logic as smart stuff
        let allowed = if client_id.len() >= 12 {
            let mac_lower = client_id.to_lowercase();
            topic.starts_with(&format!("BEPR/{mac_lower}"))
                || topic.starts_with(&format!("BEPR/{}/", mac_lower))
        } else {
            false
        };
        AclResult { allowed }
    }

    fn resolve_ow_id(&self, client_id: &str) -> String {
        // Use mac without colons, lowercased, with ow_ prefix
        format!("ow_{}", client_id.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loads_devices() {
        let auth = StaticDeviceAuth::from_embedded();
        assert!(auth.passwords.contains_key("34B7DA9882EF"));
        assert!(auth.passwords.contains_key("3030F9FD9234"));
        assert_eq!(auth.passwords.get("34B7DA9882EF").unwrap(), "0p3nW404tt");
        assert_eq!(auth.passwords.get("BEPR_SD001").unwrap(), "jabra");
    }

    #[tokio::test]
    async fn test_auth_success() {
        let auth = StaticDeviceAuth::from_embedded();
        let connect = Connect {
            protocol_name: Some("MQTT".into()),
            protocol_level: 4,
            clean_session: true,
            keep_alive: 60,
            client_id: "3030F9FD9234".into(),
            username: Some("3030F9FD9234".into()),
            password: Some(b"0p3nW404tt".to_vec()),
            will: None,
        };
        let result = auth.authenticate(&connect).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "3030F9FD9234");
    }

    #[tokio::test]
    async fn test_auth_bad_password() {
        let auth = StaticDeviceAuth::from_embedded();
        let connect = Connect {
            protocol_name: Some("MQTT".into()),
            protocol_level: 4,
            clean_session: true,
            keep_alive: 60,
            client_id: "34B7DA9882EF".into(),
            username: Some("34B7DA9882EF".into()),
            password: Some(b"wrong".to_vec()),
            will: None,
        };
        assert!(auth.authenticate(&connect).await.is_err());
    }

    #[tokio::test]
    async fn test_auth_unknown() {
        let auth = StaticDeviceAuth::from_embedded();
        let connect = Connect {
            protocol_name: Some("MQTT".into()),
            protocol_level: 4,
            clean_session: true,
            keep_alive: 60,
            client_id: "UNKNOWN".into(),
            username: Some("UNKNOWN".into()),
            password: Some(b"x".to_vec()),
            will: None,
        };
        assert!(auth.authenticate(&connect).await.is_err());
    }

    #[test]
    fn test_acl_bepr_topic() {
        let auth = StaticDeviceAuth::from_embedded();
        assert!(auth.check_topic_access("3030F9FD9234", "BEPR/3030f9fd9234/data", false).allowed);
        assert!(auth.check_topic_access("3030F9FD9234", "BEPR/other/data", false).allowed == false);
    }

    #[test]
    fn test_ow_id() {
        let auth = StaticDeviceAuth::from_embedded();
        assert_eq!(auth.resolve_ow_id("3030F9FD9234"), "ow_3030f9fd9234");
        assert_eq!(auth.resolve_ow_id("34B7DA9882EF"), "ow_34b7da9882ef");
    }
}