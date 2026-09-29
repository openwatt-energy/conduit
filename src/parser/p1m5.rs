use super::{FieldValue, ParsedReading};
use super::config;

/// Parse v1.0.47+ JSON telemetry payload (data/readings topic).
/// Field mappings are driven by configs/p1m5.json.
pub fn parse_json_data(
    json: &serde_json::Value,
    ow_id: &str,
    timestamp_ns: i64,
) -> Result<ParsedReading, String> {
    let cfg = &config::get_config().p1m5.topics["data/readings"].fields;
    let mut fields = Vec::new();

    for (obis, mapping) in cfg {
        if mapping.ignored {
            continue;
        }
        if let Some(obj) = json.get(obis) {
            if let Some(val) = extract_json_value(obj) {
                let scaled = apply_scale(val, mapping.scale);
                fields.push((mapping.name.clone(), scaled));
            }
        }
    }

    if fields.is_empty() {
        return Err("no valid fields in v1.0.47 JSON".to_string());
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: String::new(),
        sn_or_mac: String::new(),
        fields,
        timestamp_ns,
    })
}

/// Parse v1.0.47+ sys/config telemetry — field mappings driven by configs/p1m5.json.
pub fn parse_json_config(
    json: &serde_json::Value,
    ow_id: &str,
    timestamp_ns: i64,
) -> Result<ParsedReading, String> {
    let cfg = &config::get_config().p1m5.topics["sys/config"].fields;
    let mut fields = Vec::new();

    for (key, mapping) in cfg {
        if mapping.ignored {
            continue;
        }
        if let Some(val) = json.get(key) {
            if let Some(fv) = json_value_to_field(val, &mapping.field_type) {
                fields.push((mapping.name.clone(), fv));
            }
        }
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: String::new(),
        sn_or_mac: String::new(),
        fields,
        timestamp_ns,
    })
}

/// Parse a DSMR-format telegram (raw OBIS lines) — backward compat.
pub fn parse_dsmr(text: &str, ow_id: &str, timestamp_ns: i64) -> Result<ParsedReading, String> {
    let mut fields = Vec::new();
    let mut sn = String::new();

    for line in text.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('!') || line.starts_with('/') {
            continue;
        }

        if let Some((key, value)) = parse_obis_line(line) {
            if key == "0-0:96.1.255" || key == "0-0:96.1.0" {
                sn = value.trim_matches('"').to_string();
            }
            if let Some((field_name, field_value)) = map_obis_to_field(&key, &value) {
                fields.push((field_name, field_value));
            }
        }
    }

    if fields.is_empty() {
        return Err("no valid OBIS fields found".to_string());
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: String::new(),
        sn_or_mac: String::new(),
        fields,
        timestamp_ns,
    })
}

// ---------------------------------------------------------------------------
// JSON helper functions
// ---------------------------------------------------------------------------

fn extract_json_value(obj: &serde_json::Value) -> Option<FieldValue> {
    if let Some(v) = obj.get("value") {
        if let Some(n) = v.as_f64() {
            return Some(FieldValue::Float(n));
        }
        if let Some(n) = v.as_i64() {
            return Some(FieldValue::Float(n as f64));
        }
        if let Some(s) = v.as_str() {
            if let Ok(n) = s.parse::<f64>() {
                return Some(FieldValue::Float(n));
            }
            return Some(FieldValue::String(s.to_string()));
        }
    }
    if let Some(n) = obj.as_f64() {
        return Some(FieldValue::Float(n));
    }
    if let Some(n) = obj.as_i64() {
        return Some(FieldValue::Float(n as f64));
    }
    None
}

fn json_value_to_field(val: &serde_json::Value, field_type: &str) -> Option<FieldValue> {
    match field_type {
        "integer" => val.as_i64().map(FieldValue::Integer),
        "float" => val.as_f64().map(FieldValue::Float).or_else(|| {
            val.as_i64().map(|i| FieldValue::Float(i as f64))
        }),
        "string" => val.as_str().map(|s| FieldValue::String(s.to_string())),
        "boolean" => val.as_bool().map(FieldValue::Boolean),
        _ => val.as_f64().map(FieldValue::Float),
    }
}

fn apply_scale(val: FieldValue, scale: Option<f64>) -> FieldValue {
    match (val, scale) {
        (FieldValue::Float(v), Some(s)) => FieldValue::Float(v * s),
        (FieldValue::Integer(v), Some(s)) => FieldValue::Float(v as f64 * s),
        (v, _) => v,
    }
}

pub fn is_json(payload: &[u8]) -> bool {
    payload.first() == Some(&b'{')
}

// ---------------------------------------------------------------------------
// DSMR helper functions
// ---------------------------------------------------------------------------

fn parse_obis_line(line: &str) -> Option<(String, String)> {
    let open_paren = line.find('(')?;
    let close_paren = line.rfind(')')?;

    let key = line[..open_paren].trim().to_string();
    let value = line[open_paren + 1..close_paren].trim().to_string();

    Some((key, value))
}

fn map_obis_to_field(key: &str, value: &str) -> Option<(String, FieldValue)> {
    let value = value.trim_start_matches('(').trim_end_matches(')');

    match key {
        "1-0:32.7.0" => parse_voltage(value).map(|v| ("v_l1".into(), FieldValue::Float(v))),
        "1-0:52.7.0" => parse_voltage(value).map(|v| ("v_l2".into(), FieldValue::Float(v))),
        "1-0:72.7.0" => parse_voltage(value).map(|v| ("v_l3".into(), FieldValue::Float(v))),
        "1-0:1.7.0" => parse_kw_to_w(value).map(|v| ("w_delivered".into(), FieldValue::Integer(v))),
        "1-0:2.7.0" => parse_kw_to_w(value).map(|v| ("w_received".into(), FieldValue::Integer(v))),
        "1-0:31.7.0" => parse_ampere(value).map(|v| ("a_l1".into(), FieldValue::Integer(v))),
        "1-0:51.7.0" => parse_ampere(value).map(|v| ("a_l2".into(), FieldValue::Integer(v))),
        "1-0:71.7.0" => parse_ampere(value).map(|v| ("a_l3".into(), FieldValue::Integer(v))),
        "1-0:1.8.1" => parse_kwh(value).map(|v| ("e_delivered_t1".into(), FieldValue::Float(v))),
        "1-0:1.8.2" => parse_kwh(value).map(|v| ("e_delivered_t2".into(), FieldValue::Float(v))),
        "1-0:2.8.1" => parse_kwh(value).map(|v| ("e_received_t1".into(), FieldValue::Float(v))),
        "1-0:2.8.2" => parse_kwh(value).map(|v| ("e_received_t2".into(), FieldValue::Float(v))),
        "0-1:24.2.1" => parse_gas(value).map(|v| ("gas_m3".into(), FieldValue::Float(v))),
        "0-0:96.14.0" => {
            parse_tariff(value).map(|v| ("tariff".into(), FieldValue::Float(v)))
        }
        _ => None,
    }
}

fn parse_voltage(value: &str) -> Option<f64> {
    let v: f64 = value.split('*').next()?.parse().ok()?;
    Some(v)
}

fn parse_kw_to_w(value: &str) -> Option<i64> {
    let kw: f64 = value.split('*').next()?.parse().ok()?;
    Some((kw * 1000.0).round() as i64)
}

fn parse_ampere(value: &str) -> Option<i64> {
    let a: f64 = value.split('*').next()?.parse().ok()?;
    Some(a.round() as i64)
}

fn parse_kwh(value: &str) -> Option<f64> {
    let v: f64 = value.split('*').next()?.parse().ok()?;
    Some(v)
}

fn parse_gas(value: &str) -> Option<f64> {
    let v: f64 = value.split('*').next()?.parse().ok()?;
    Some(v)
}

fn parse_tariff(value: &str) -> Option<f64> {
    let v: f64 = value.split('*').next()?.parse().ok()?;
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_dsmr_telegram() {
        let telegram = r"/KMP5 ZIV2 E0002

0-0:96.1.255(12345678)
1-0:1.8.1(001234.567*kWh)
1-0:1.8.2(000123.456*kWh)
1-0:2.8.1(000012.345*kWh)
1-0:2.8.2(000001.234*kWh)
1-0:1.7.0(00.500*kW)
1-0:2.7.0(00.000*kW)
1-0:32.7.0(230.0*V)
1-0:52.7.0(229.5*V)
1-0:72.7.0(230.8*V)
1-0:31.7.0(002*A)
0-1:24.2.1(12345.678*m3)
!ABCD";

        let result = parse_dsmr(telegram, "p1m5_TEST", 0).unwrap();
        assert_eq!(result.ow_id, "p1m5_TEST");
        assert_eq!(result.device_type, "");

        let fields: HashMap<&str, &FieldValue> = result
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        assert_eq!(
            fields.get("v_l1").copied(),
            Some(&FieldValue::Float(230.0))
        );
        assert_eq!(
            fields.get("w_delivered").copied(),
            Some(&FieldValue::Integer(500))
        );
        assert_eq!(
            fields.get("w_received").copied(),
            Some(&FieldValue::Integer(0))
        );
        assert_eq!(
            fields.get("e_delivered_t1").copied(),
            Some(&FieldValue::Float(1234.567))
        );
        assert_eq!(
            fields.get("gas_m3").copied(),
            Some(&FieldValue::Float(12345.678))
        );
    }

    #[test]
    fn test_kw_to_w_conversion() {
        assert_eq!(parse_kw_to_w("00.500*kW"), Some(500));
        assert_eq!(parse_kw_to_w("01.234*kW"), Some(1234));
        assert_eq!(parse_kw_to_w("00.000*kW"), Some(0));
    }

    #[test]
    fn test_json_data_parsing() {
        let json: serde_json::Value = serde_json::json!({
            "0-0:96.1.1": "3153414733313030323131333538",
            "0-0:96.14.0": "1",
            "1-0:1.8.1": {"value": 29865.06, "unit": "kWh"},
            "1-0:1.8.2": {"value": 58161.78, "unit": "kWh"},
            "1-0:2.8.1": {"value": 18273.0, "unit": "kWh"},
            "1-0:2.8.2": {"value": 11677.9, "unit": "kWh"},
            "1-0:1.7.0": {"value": 0.333, "unit": "kW"},
            "1-0:2.7.0": {"value": 6.008, "unit": "kW"},
            "1-0:32.7.0": {"value": 247.1, "unit": "V"},
            "1-0:52.7.0": {"value": 0.0, "unit": "V"},
            "1-0:72.7.0": {"value": 247.9, "unit": "V"},
            "1-0:31.7.0": {"value": 14.89, "unit": "A"},
            "1-0:51.7.0": {"value": 9.66, "unit": "A"},
            "1-0:71.7.0": {"value": 21.23, "unit": "A"}
        });

        let result = parse_json_data(&json, "ow_TEST", 0).unwrap();

        let fields: HashMap<&str, &FieldValue> = result
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        assert_eq!(*fields.get("kwh_delivered_t1").unwrap(), FieldValue::Float(29865.06));
        assert_eq!(*fields.get("kwh_delivered_t2").unwrap(), FieldValue::Float(58161.78));
        // kW → W via scale: 0.333 * 1000 = 333.0
        assert_eq!(*fields.get("w_delivered").unwrap(), FieldValue::Float(333.0));
        assert_eq!(*fields.get("w_returned").unwrap(), FieldValue::Float(6008.0));
        assert_eq!(*fields.get("v_l1").unwrap(), FieldValue::Float(247.1));
        assert_eq!(*fields.get("a_l1").unwrap(), FieldValue::Float(14.89));
        // tariff comes from string "1" → float
        assert_eq!(*fields.get("tariff").unwrap(), FieldValue::Float(1.0));
    }

    #[test]
    fn test_json_config_parsing() {
        let json: serde_json::Value = serde_json::json!({
            "fw_ver": "v1.0.47-ow",
            "uptime": 3600,
            "boot_count": 12,
            "wifi_rssi": -45,
            "wifi_ssid": "Clovis Home",
            "dongle_ip": "10.13.20.146",
            "mqtt_connected": true,
            "p1_connected": true,
            "email": "test@example.com"
        });

        let result = parse_json_config(&json, "ow_TEST", 0).unwrap();

        let fields: HashMap<&str, &FieldValue> = result
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        assert_eq!(*fields.get("fw_version").unwrap(), FieldValue::String("v1.0.47-ow".into()));
        assert_eq!(*fields.get("uptime").unwrap(), FieldValue::Integer(3600));
        assert_eq!(*fields.get("reboots").unwrap(), FieldValue::Integer(12));
        assert_eq!(*fields.get("wifi_rssi").unwrap(), FieldValue::Integer(-45));
        assert_eq!(*fields.get("wifi_ssid").unwrap(), FieldValue::String("Clovis Home".into()));
        assert_eq!(*fields.get("mqtt_connected").unwrap(), FieldValue::Boolean(true));
        // 'email' is ignored in config
        assert!(fields.get("email").is_none());
    }

    #[test]
    fn test_ignored_fields() {
        let json: serde_json::Value = serde_json::json!({
            "email": "should@be.ignored",
            "ts": "2024-01-01",
            "fw_ver": "v1.0",
            "uptime": 1
        });

        let result = parse_json_config(&json, "ow_TEST", 0).unwrap();

        let fields: HashMap<&str, &FieldValue> = result
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();

        assert!(fields.get("email").is_none());
        assert!(fields.get("ts").is_none());
        assert!(fields.get("fw_version").is_some());
        assert!(fields.get("uptime").is_some());
    }

    #[test]
    fn test_empty_data_is_error() {
        let json: serde_json::Value = serde_json::json!({});
        assert!(parse_json_data(&json, "ow_TEST", 0).is_err());
    }

    #[test]
    fn test_apply_scale() {
        assert_eq!(apply_scale(FieldValue::Float(0.5), Some(1000.0)), FieldValue::Float(500.0));
        assert_eq!(apply_scale(FieldValue::Integer(2), Some(1000.0)), FieldValue::Float(2000.0));
        assert_eq!(apply_scale(FieldValue::Float(230.0), None), FieldValue::Float(230.0));
        assert_eq!(apply_scale(FieldValue::String("x".into()), Some(10.0)), FieldValue::String("x".into()));
    }

    #[test]
    fn test_json_value_conversion() {
        assert_eq!(
            json_value_to_field(&serde_json::json!(42), "integer"),
            Some(FieldValue::Integer(42))
        );
        assert_eq!(
            json_value_to_field(&serde_json::json!(3.14), "float"),
            Some(FieldValue::Float(3.14))
        );
        assert_eq!(
            json_value_to_field(&serde_json::json!("hello"), "string"),
            Some(FieldValue::String("hello".into()))
        );
        assert_eq!(
            json_value_to_field(&serde_json::json!(true), "boolean"),
            Some(FieldValue::Boolean(true))
        );
    }

    #[test]
    fn test_line_protocol() {
        let reading = ParsedReading {
            ow_id: "p1m5_TEST".into(),
            device_type: String::new(),
            sn_or_mac: "12345".into(),
            fields: vec![
                ("v_l1".into(), FieldValue::Float(230.0)),
                ("w_delivered".into(), FieldValue::Integer(500)),
            ],
            timestamp_ns: 1234567890000000000,
        };
        let lp = reading.to_line_protocol("readings");
        assert!(lp.starts_with("readings,ow_id=p1m5_TEST "));
        assert!(lp.contains("v_l1=230"));
        assert!(lp.contains("w_delivered=500i"));
        assert!(lp.contains("1234567890000000000"));
    }
}