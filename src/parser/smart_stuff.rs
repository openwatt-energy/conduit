use super::{FieldValue, ParsedReading};
use serde_json::Value;

pub fn parse_json(text: &str, ow_id: &str, timestamp_ns: i64) -> Result<ParsedReading, String> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| format!("JSON parse error: {e}"))?;

    let mut fields = Vec::new();
    let mut mac = String::new();

    if let Some(obj) = v.as_object() {
        for (key, value) in obj {
            match key.as_str() {
                "mac" | "device_mac" => {
                    if let Some(s) = value.as_str() {
                        mac = s.to_string();
                    }
                }
                "power_delivered" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("w_delivered".into(), FieldValue::Integer((n * 1000.0).round() as i64)));
                    }
                }
                "power_received" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("w_received".into(), FieldValue::Integer((n * 1000.0).round() as i64)));
                    }
                }
                "voltage_l1" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("v_l1".into(), FieldValue::Float(n)));
                    }
                }
                "voltage_l2" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("v_l2".into(), FieldValue::Float(n)));
                    }
                }
                "voltage_l3" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("v_l3".into(), FieldValue::Float(n)));
                    }
                }
                "current_l1" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("a_l1".into(), FieldValue::Integer(n.round() as i64)));
                    }
                }
                "current_l2" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("a_l2".into(), FieldValue::Integer(n.round() as i64)));
                    }
                }
                "current_l3" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("a_l3".into(), FieldValue::Integer(n.round() as i64)));
                    }
                }
                "energy_delivered_t1" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("e_delivered_t1".into(), FieldValue::Float(n)));
                    }
                }
                "energy_delivered_t2" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("e_delivered_t2".into(), FieldValue::Float(n)));
                    }
                }
                "energy_received_t1" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("e_received_t1".into(), FieldValue::Float(n)));
                    }
                }
                "energy_received_t2" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("e_received_t2".into(), FieldValue::Float(n)));
                    }
                }
                "temperature" => {
                    if let Some(n) = value.as_f64() {
                        fields.push(("temp".into(), FieldValue::Float(n)));
                    }
                }
                _ => {}
            }
        }
    }

    if fields.is_empty() {
        return Err("no valid Smart Stuff fields found".to_string());
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: "smart_stuff".to_string(),
        sn_or_mac: mac,
        fields,
        timestamp_ns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_smart_stuff() {
        let json = r#"{
            "mac": "AA:BB:CC:DD:EE:FF",
            "power_delivered": 0.500,
            "power_received": 0.100,
            "voltage_l1": 230.0,
            "voltage_l2": 229.5,
            "current_l1": 2.0,
            "energy_delivered_t1": 1234.567,
            "energy_delivered_t2": 123.456,
            "temperature": 22.5
        }"#;

        let result = parse_json(json, "ow_AABBCCDDEEFF", 0).unwrap();
        assert_eq!(result.ow_id, "ow_AABBCCDDEEFF");
        assert_eq!(result.device_type, "smart_stuff");
        assert_eq!(result.sn_or_mac, "AA:BB:CC:DD:EE:FF");

        let fields: Vec<(String, FieldValue)> = result.fields;
        let w_delivered = fields.iter().find(|(k, _)| k == "w_delivered").unwrap();
        assert_eq!(w_delivered.1, FieldValue::Integer(500));

        let v_l1 = fields.iter().find(|(k, _)| k == "v_l1").unwrap();
        assert_eq!(v_l1.1.clone(), FieldValue::Float(230.0));
    }

    #[test]
    fn test_smart_stuff_power_conversion() {
        let json = r#"{"power_delivered": 1.234}"#;
        let result = parse_json(json, "ow_test", 0).unwrap();
        let w = result.fields.iter().find(|(k, _)| k == "w_delivered").unwrap();
        assert_eq!(w.1.clone(), FieldValue::Integer(1234));
    }
}