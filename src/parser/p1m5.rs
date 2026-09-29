use super::{FieldValue, ParsedReading};

/// Parse a DSMR-format telegram (raw OBIS lines).
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
        sn_or_mac: sn,
        fields,
        timestamp_ns,
    })
}

/// Parse v1.0.47+ JSON telemetry payload (data/readings topic).
pub fn parse_json_data(
    json: &serde_json::Value,
    ow_id: &str,
    timestamp_ns: i64,
) -> Result<ParsedReading, String> {
    let mut fields = Vec::new();
    let mut sn = String::new();

    // Try to extract serial/equipment ID
    if let Some(eq) = json.get("0-0:96.1.1") {
        sn = eq.as_str().unwrap_or("").to_string();
    }

    // Map OBIS-like keys to field names
    let mappings: &[(&str, &str)] = &[
        ("1-0:32.7.0", "v_l1"),
        ("1-0:52.7.0", "v_l2"),
        ("1-0:72.7.0", "v_l3"),
        ("1-0:1.7.0", "w_delivered"),
        ("1-0:2.7.0", "w_received"),
        ("1-0:31.7.0", "a_l1"),
        ("1-0:51.7.0", "a_l2"),
        ("1-0:71.7.0", "a_l3"),
        ("1-0:1.8.1", "e_delivered_t1"),
        ("1-0:1.8.2", "e_delivered_t2"),
        ("1-0:2.8.1", "e_received_t1"),
        ("1-0:2.8.2", "e_received_t2"),
        ("1-0:21.7.0", "w_import_l1"),
        ("1-0:41.7.0", "w_import_l2"),
        ("1-0:61.7.0", "w_import_l3"),
        ("1-0:22.7.0", "w_export_l1"),
        ("1-0:42.7.0", "w_export_l2"),
        ("1-0:62.7.0", "w_export_l3"),
    ];

    for (obis, field_name) in mappings {
        if let Some(obj) = json.get(*obis) {
            let value = extract_value(obj);
            if let Some(v) = value {
                fields.push((field_name.to_string(), v));
            }
        }
    }

    // Also extract gas (sent under "0-1:24.2.1" in JSON)
    if let Some(obj) = json.get("0-1:24.2.1") {
        if let Some(v) = extract_value(obj) {
            fields.push(("gas_m3".to_string(), v));
        }
    }

    // Tariff
    if let Some(t) = json.get("0-0:96.14.0") {
        let val = t.as_str().unwrap_or("").parse::<f64>().unwrap_or(0.0);
        fields.push(("tariff".to_string(), FieldValue::Float(val)));
    }

    if fields.is_empty() {
        return Err("no valid fields in v1.0.47 JSON".to_string());
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: String::new(),
        sn_or_mac: sn,
        fields,
        timestamp_ns,
    })
}

/// Parse v1.0.47+ sys/config telemetry.
pub fn parse_json_config(
    json: &serde_json::Value,
    ow_id: &str,
    timestamp_ns: i64,
) -> Result<ParsedReading, String> {
    let mut fields = Vec::new();

    if let Some(fw) = json.get("fw_ver").and_then(|v| v.as_str()) {
        fields.push(("fw_version".to_string(), FieldValue::String(fw.to_string())));
    }
    if let Some(uptime) = json.get("uptime").and_then(|v| v.as_i64()) {
        fields.push(("uptime".to_string(), FieldValue::Integer(uptime)));
    }
    if let Some(reboots) = json.get("reboots").and_then(|v| v.as_i64()) {
        fields.push(("reboots".to_string(), FieldValue::Integer(reboots)));
    }
    if let Some(rssi) = json.get("wifi_rssi").and_then(|v| v.as_i64()) {
        fields.push(("wifi_rssi".to_string(), FieldValue::Integer(rssi)));
    }
    if let Some(ssid) = json.get("wifi_ssid").and_then(|v| v.as_str()) {
        fields.push(("wifi_ssid".to_string(), FieldValue::String(ssid.to_string())));
    }
    if let Some(ip) = json.get("dongle_ip").and_then(|v| v.as_str()) {
        fields.push(("dongle_ip".to_string(), FieldValue::String(ip.to_string())));
    }

    Ok(ParsedReading {
        ow_id: ow_id.to_string(),
        device_type: String::new(),
        sn_or_mac: String::new(),
        fields,
        timestamp_ns,
    })
}

/// Extract a numeric value from a v1.0.47 JSON object like {"value": 230.0, "unit": "V"}.
fn extract_value(obj: &serde_json::Value) -> Option<FieldValue> {
    if let Some(v) = obj.get("value") {
        if let Some(n) = v.as_f64() {
            // Convert kW to W for power fields
            return Some(FieldValue::Float(n));
        }
        if let Some(n) = v.as_i64() {
            return Some(FieldValue::Integer(n));
        }
        if let Some(s) = v.as_str() {
            if let Ok(n) = s.parse::<f64>() {
                return Some(FieldValue::Float(n));
            }
            return Some(FieldValue::String(s.to_string()));
        }
    }
    // Flattened scalar values
    if let Some(n) = obj.as_f64() {
        return Some(FieldValue::Float(n));
    }
    if let Some(n) = obj.as_i64() {
        return Some(FieldValue::Integer(n));
    }
    None
}

pub fn is_json(payload: &[u8]) -> bool {
    payload.first() == Some(&b'{')
}

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
        assert!(lp.starts_with("readings,ow_id=p1m5_TEST,sn=12345 "));
        assert!(lp.contains("v_l1=230"));
        assert!(lp.contains("w_delivered=500i"));
        assert!(lp.contains("1234567890000000000"));
    }
}