use super::{FieldValue, ParsedReading};
use super::config;
use serde_json::Value;

/// Parse a Smart Stuff / BEPR `/data` JSON payload.
/// Field mappings driven by configs/smartstuff.json.
pub fn parse_json(text: &str, ow_id: &str, timestamp_ns: i64) -> Result<ParsedReading, String> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| format!("JSON parse error: {e}"))?;

    let cfg = &config::get_config().smartstuff;
    let data_cfg = &cfg.topics["data"].fields;
    let mut fields = Vec::new();

    if let Some(obj) = v.as_object() {
        for (key, value) in obj {
            if let Some(mapping) = data_cfg.get(key) {
                if mapping.ignored {
                    continue;
                }
                let fv = match mapping.field_type.as_str() {
                    "float" => value.as_f64().map(FieldValue::Float),
                    "integer" => value.as_i64().map(FieldValue::Integer),
                    "string" => value.as_str().map(|s| FieldValue::String(s.to_string())),
                    "boolean" => value.as_bool().map(FieldValue::Boolean),
                    _ => value.as_f64().map(FieldValue::Float),
                };
                if let Some(val) = fv {
                    let scaled = p1m5_apply_scale(val, mapping.scale);
                    fields.push((mapping.name.clone(), scaled));
                }
            }
        }
    }

    if fields.is_empty() {
        return Err("no valid Smart Stuff fields found".to_string());
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: String::new(),
        sn_or_mac: String::new(),
        fields,
        timestamp_ns,
    })
}

/// Parse a single-variable topic (plain-text payload, e.g. BEPR/{mac}/firmware).
pub fn parse_variable(
    variable: &str,
    payload: &str,
    ow_id: &str,
    timestamp_ns: i64,
) -> Result<ParsedReading, String> {
    let cfg = &config::get_config().smartstuff;

    // Check ignored first
    if cfg.ignored_variables.iter().any(|v| v == variable) {
        return Err("ignored variable".to_string());
    }

    let mapping = cfg
        .variables
        .get(variable)
        .ok_or_else(|| format!("unknown variable: {variable}"))?;

    let payload = payload.trim();
    let fv = match mapping.field_type.as_str() {
        "float" => payload.parse::<f64>().ok().map(FieldValue::Float),
        "integer" => payload.parse::<i64>().ok().map(FieldValue::Integer),
        _ => Some(FieldValue::String(payload.to_string())),
    };

    match fv {
        Some(val) => Ok(ParsedReading {
            ow_id: ow_id.to_string(),
            device_type: String::new(),
            sn_or_mac: String::new(),
            fields: vec![(mapping.name.clone(), val)],
            timestamp_ns,
        }),
        None => Err(format!("invalid value for {variable}: {payload}")),
    }
}

/// Reuse p1m5's scale helper (avoids code duplication).
use crate::parser::p1m5::apply_scale as p1m5_apply_scale;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_smart_stuff_data() {
        let json = r#"{
            "power_delivered": 0.500,
            "power_returned": 0.100,
            "voltage_l1": 230.0,
            "voltage_l2": 229.5,
            "current_l1": 2.0,
            "energy_delivered_tariff1": 1234.567,
            "energy_delivered_tariff2": 123.456,
            "temperature": 22.5,
            "gas": 7424.809
        }"#;

        let result = parse_json(json, "ow_AABBCCDDEEFF", 0).unwrap();
        assert_eq!(result.ow_id, "ow_AABBCCDDEEFF");
        assert_eq!(result.device_type, "");

        let fields: std::collections::HashMap<&str, &FieldValue> = result
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        // power_delivered 0.500 kW → 500 W via scale
        assert_eq!(*fields.get("w_delivered").unwrap(), FieldValue::Float(500.0));
        assert_eq!(*fields.get("w_returned").unwrap(), FieldValue::Float(100.0));
        assert_eq!(*fields.get("v_l1").unwrap(), FieldValue::Float(230.0));
        assert_eq!(
            *fields.get("kwh_delivered_t1").unwrap(),
            FieldValue::Float(1234.567)
        );
        assert_eq!(*fields.get("temp").unwrap(), FieldValue::Float(22.5));
        assert_eq!(*fields.get("gas_m3").unwrap(), FieldValue::Float(7424.809));
    }

    #[test]
    fn test_parse_variable() {
        let result = parse_variable("firmware", "4.8.17", "ow_MAC", 0).unwrap();
        assert_eq!(result.fields[0].0, "fw");
        assert_eq!(result.fields[0].1, FieldValue::String("4.8.17".into()));

        let result = parse_variable("wifi_rssi", "  -57  ", "ow_MAC", 0).unwrap();
        assert_eq!(result.fields[0].0, "wifi_rssi");
        assert_eq!(result.fields[0].1, FieldValue::String("-57".into()));
    }

    #[test]
    fn test_ignored_variable_is_error() {
        assert!(parse_variable("LWT", "Online", "ow_MAC", 0).is_err());
    }
}