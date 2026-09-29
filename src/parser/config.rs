use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Inline-embedded at compile time — zero runtime IO, same perf as hardcoded.
static P1M5_CONFIG_JSON: &str = include_str!("../../configs/p1m5.json");

#[derive(Debug, Clone, Deserialize)]
pub struct FieldMapping {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub ignored: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TopicConfig {
    pub fields: HashMap<String, FieldMapping>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrandTopics {
    pub topics: HashMap<String, TopicConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParserConfig {
    pub p1m5: BrandTopics,
}

/// Global config cache — deserialized once on first access.
static CONFIG: OnceLock<ParserConfig> = OnceLock::new();

pub fn get_config() -> &'static ParserConfig {
    CONFIG.get_or_init(|| {
        serde_json::from_str(P1M5_CONFIG_JSON)
            .expect("failed to parse configs/p1m5.json")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loads() {
        let cfg = get_config();
        let readings = &cfg.p1m5.topics["data/readings"].fields;
        assert_eq!(readings["1-0:32.7.0"].name, "v_l1");
        assert_eq!(readings["1-0:32.7.0"].field_type, "float");
        assert_eq!(readings["1-0:1.7.0"].scale, Some(1000.0));

        let config = &cfg.p1m5.topics["sys/config"].fields;
        assert_eq!(config["fw_ver"].name, "fw_version");
        assert!(config["email"].ignored);
    }
}