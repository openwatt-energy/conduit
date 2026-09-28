use super::{FieldValue, ParsedReading};

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
        device_type: "p1m5".to_string(),
        sn_or_mac: sn,
        fields,
        timestamp_ns,
    })
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
            parse_tariff(value).map(|v| ("tariff".into(), FieldValue::Integer(v)))
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

fn parse_tariff(value: &str) -> Option<i64> {
    let v: i64 = value.split('*').next()?.parse().ok()?;
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
        assert_eq!(result.device_type, "p1m5");

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
            device_type: "p1m5".into(),
            sn_or_mac: "12345".into(),
            fields: vec![
                ("v_l1".into(), FieldValue::Float(230.0)),
                ("w_delivered".into(), FieldValue::Integer(500)),
            ],
            timestamp_ns: 1234567890000000000,
        };
        let lp = reading.to_line_protocol("readings");
        assert!(lp.starts_with("readings,ow_id=p1m5_TEST,device_type=p1m5,sn=12345 "));
        assert!(lp.contains("v_l1=230"));
        assert!(lp.contains("w_delivered=500i"));
        assert!(lp.contains("1234567890000000000"));
    }
}