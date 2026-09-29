use bytes::Bytes;
use serde::Serialize;

pub mod p1m5;
pub mod smart_stuff;

#[derive(Debug, Clone)]
pub struct TelemetryPacket {
    pub ow_id: String,
    pub topic: String,
    pub payload: Bytes,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedReading {
    pub ow_id: String,
    pub device_type: String,
    pub sn_or_mac: String,
    pub fields: Vec<(String, FieldValue)>,
    pub timestamp_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FieldValue {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
}

impl ParsedReading {
    pub fn to_line_protocol(&self, measurement: &str) -> String {
        let mut s = String::new();

        s.push_str(measurement);
        s.push_str(",ow_id=");
        s.push_str(&self.ow_id);
        if !self.device_type.is_empty() {
            s.push_str(",device_type=");
            s.push_str(&self.device_type);
        }
        s.push_str(",sn=");
        s.push_str(&self.sn_or_mac);

        s.push(' ');
        for (i, (name, value)) in self.fields.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(name);
            s.push('=');
            match value {
                FieldValue::Integer(v) => {
                    s.push_str(&format!("{v}i"));
                }
                FieldValue::Float(v) => {
                    s.push_str(&format!("{v}"));
                }
                FieldValue::String(v) => {
                    s.push('"');
                    s.push_str(&v.replace('"', "\\\""));
                    s.push('"');
                }
                FieldValue::Boolean(v) => {
                    s.push_str(if *v { "true" } else { "false" });
                }
            }
        }

        s.push(' ');
        s.push_str(&self.timestamp_ns.to_string());

        s
    }
}

pub fn derive_ow_id(client_id: &str, topic: &str) -> String {
    if client_id.starts_with("P1M5_") {
        client_id.to_string()
    } else if client_id.len() == 8 && client_id.starts_with("P1") {
        // v1.0.47+ format: P1855EA8
        format!("p1m5_{}", client_id)
    } else if client_id.starts_with("ow_") {
        client_id.to_string()
    } else if topic.starts_with("P1M5/") {
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() >= 2 {
            format!("p1m5_{}", parts[1])
        } else {
            client_id.to_string()
        }
    } else if topic.starts_with("BEPR/") {
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() >= 2 {
            format!("ow_{}", parts[1])
        } else {
            client_id.to_string()
        }
    } else {
        client_id.to_string()
    }
}

pub fn parse_telemetry(packet: &TelemetryPacket) -> Result<ParsedReading, String> {
    let topic = &packet.topic;
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);

    if topic.contains("/data") {
        let text = String::from_utf8_lossy(&packet.payload).to_string();

        if topic.starts_with("P1M5/") {
            // v1.0.47+ firmware sends JSON, older firmware sends raw DSMR.
            if p1m5::is_json(&packet.payload) {
                let json: serde_json::Value =
                    serde_json::from_slice(&packet.payload).map_err(|e| format!("JSON: {e}"))?;
                if topic.contains("/data/") {
                    p1m5::parse_json_data(&json, &packet.ow_id, now)
                } else if topic.contains("/sys/") {
                    p1m5::parse_json_config(&json, &packet.ow_id, now)
                } else {
                    p1m5::parse_dsmr(&text, &packet.ow_id, now)
                }
            } else {
                p1m5::parse_dsmr(&text, &packet.ow_id, now)
            }
        } else if topic.starts_with("BEPR/") {
            smart_stuff::parse_json(&text, &packet.ow_id, now)
        } else {
            Err(format!("unknown data topic: {topic}"))
        }
    } else {
        Err(format!("not a data topic: {topic}"))
    }
}