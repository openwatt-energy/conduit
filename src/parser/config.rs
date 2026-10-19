use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Inline-embedded at compile time — zero runtime IO, same perf as hardcoded.
static P1M5_CONFIG_JSON: &str = include_str!("../../configs/p1m5.json");
static SMARTSTUFF_CONFIG_JSON: &str = include_str!("../../configs/smartstuff.json");

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
pub struct VariableMapping {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrandConfig {
    pub topics: HashMap<String, TopicConfig>,
    #[serde(default)]
    pub variables: HashMap<String, VariableMapping>,
    #[serde(default)]
    pub ignored_variables: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParserConfig {
    pub p1m5: BrandConfig,
    #[serde(default)]
    pub smartstuff: BrandConfig,
}

impl Default for BrandConfig {
    fn default() -> Self {
        BrandConfig {
            topics: HashMap::new(),
            variables: HashMap::new(),
            ignored_variables: Vec::new(),
        }
    }
}

/// Global config cache — deserialized once on first access.
static CONFIG: OnceLock<ParserConfig> = OnceLock::new();

pub fn get_config() -> &'static ParserConfig {
    CONFIG.get_or_init(|| {
        let mut cfg: ParserConfig =
            serde_json::from_str(P1M5_CONFIG_JSON).expect("failed to parse configs/p1m5.json");
        let ss: serde_json::Value = serde_json::from_str(SMARTSTUFF_CONFIG_JSON)
            .expect("failed to parse configs/smartstuff.json");
        cfg.smartstuff = serde_json::from_value(ss["smartstuff"].clone())
            .unwrap_or_default();
        cfg
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
        assert_eq!(config["fw_ver"].name, "fw");
        assert!(config["email"].ignored);

        // Smart Stuff
        let data = &cfg.smartstuff.topics["data"].fields;
        assert_eq!(data["power_delivered"].name, "w_delivered");
        assert_eq!(data["power_delivered"].scale, Some(1000.0));
        assert_eq!(data["temperature"].name, "temp");
        assert_eq!(cfg.smartstuff.variables["firmware"].name, "fw");
    }
}