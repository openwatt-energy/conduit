use bytes::{Buf, BufMut, Bytes, BytesMut};

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Connect(Connect),
    ConnAck(ConnAck),
    Publish(Publish),
    PubAck(u16),
    Subscribe(Subscribe),
    SubAck(SubAck),
    Unsubscribe(Unsubscribe),
    UnsubAck(u16),
    PingReq,
    PingResp,
    Disconnect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Connect {
    pub protocol_name: Option<String>,
    pub protocol_level: u8,
    pub client_id: String,
    pub clean_session: bool,
    pub keep_alive: u16,
    pub username: Option<String>,
    pub password: Option<Vec<u8>>,
    pub will: Option<LastWill>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LastWill {
    pub topic: String,
    pub message: String,
    pub qos: u8,
    pub retain: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnAck {
    pub session_present: bool,
    pub return_code: ConnAckCode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnAckCode {
    Accepted = 0,
    UnacceptableProtocolVersion = 1,
    IdentifierRejected = 2,
    ServerUnavailable = 3,
    BadUsernameOrPassword = 4,
    NotAuthorized = 5,
}

impl ConnAckCode {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Accepted,
            1 => Self::UnacceptableProtocolVersion,
            2 => Self::IdentifierRejected,
            3 => Self::ServerUnavailable,
            4 => Self::BadUsernameOrPassword,
            5 => Self::NotAuthorized,
            _ => Self::ServerUnavailable,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Publish {
    pub dup: bool,
    pub qos: u8,
    pub retain: bool,
    pub topic: String,
    pub packet_id: Option<u16>,
    pub payload: Bytes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subscribe {
    pub packet_id: u16,
    pub topics: Vec<(String, u8)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubAck {
    pub packet_id: u16,
    pub return_codes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unsubscribe {
    pub packet_id: u16,
    pub topics: Vec<String>,
}

const CONNECT: u8 = 1;
const CONNACK: u8 = 2;
const PUBLISH: u8 = 3;
const PUBACK: u8 = 4;
const SUBSCRIBE: u8 = 8;
const SUBACK: u8 = 9;
const UNSUBSCRIBE: u8 = 10;
const UNSUBACK: u8 = 11;
const PINGREQ: u8 = 12;
const PINGRESP: u8 = 13;
const DISCONNECT: u8 = 14;

#[derive(Debug)]
pub enum DecodeError {
    Incomplete,
    InvalidFixedHeader,
    InvalidRemainingLength,
    InvalidProtocolName,
    InvalidProtocolLevel,
    InvalidUtf8,
    InvalidPacketType(u8),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Incomplete => write!(f, "incomplete packet"),
            DecodeError::InvalidFixedHeader => write!(f, "invalid fixed header"),
            DecodeError::InvalidRemainingLength => write!(f, "invalid remaining length"),
            DecodeError::InvalidProtocolName => write!(f, "invalid protocol name"),
            DecodeError::InvalidProtocolLevel => write!(f, "invalid protocol level"),
            DecodeError::InvalidUtf8 => write!(f, "invalid UTF-8"),
            DecodeError::InvalidPacketType(t) => write!(f, "invalid packet type {t}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl Packet {
    pub fn decode(src: &mut BytesMut) -> Result<Option<Self>, DecodeError> {
        if src.len() < 2 {
            return Ok(None);
        }

        let byte0 = src[0];
        let packet_type = byte0 >> 4;
        let flags = byte0 & 0x0F;

        let remaining_len = match read_remaining_length(src) {
            Ok((len, consumed)) => {
                let total = 1 + consumed + len;
                if src.len() < total {
                    return Ok(None);
                }
                src.advance(1 + consumed);
                len
            }
            Err(e) => return Err(e),
        };

        if src.len() < remaining_len {
            return Ok(None);
        }

        let payload = src.split_to(remaining_len);
        let mut payload = payload.freeze();

        match packet_type {
            CONNECT => Ok(Some(Packet::Connect(decode_connect(
                &mut payload, flags,
            )?))),
            CONNACK => Ok(Some(Packet::ConnAck(decode_connack(&mut payload)?))),
            PUBLISH => Ok(Some(Packet::Publish(decode_publish(
                &mut payload, flags,
            )?))),
            PUBACK => {
                Ok(Some(Packet::PubAck(read_u16(&mut payload)?)))
            }
            SUBSCRIBE => Ok(Some(Packet::Subscribe(decode_subscribe(
                &mut payload, flags,
            )?))),
            SUBACK => Ok(Some(Packet::SubAck(decode_suback(&mut payload)?))),
            UNSUBSCRIBE => Ok(Some(Packet::Unsubscribe(decode_unsubscribe(
                &mut payload, flags,
            )?))),
            UNSUBACK => {
                Ok(Some(Packet::UnsubAck(read_u16(&mut payload)?)))
            }
            PINGREQ => Ok(Some(Packet::PingReq)),
            PINGRESP => Ok(Some(Packet::PingResp)),
            DISCONNECT => Ok(Some(Packet::Disconnect)),
            t => Err(DecodeError::InvalidPacketType(t)),
        }
    }

    pub fn encode(&self, dst: &mut BytesMut) {
        match self {
            Packet::Connect(p) => encode_connect(p, dst),
            Packet::ConnAck(p) => encode_connack(p, dst),
            Packet::Publish(p) => encode_publish(p, dst),
            Packet::PubAck(id) => encode_puback(*id, dst),
            Packet::Subscribe(p) => encode_subscribe(p, dst),
            Packet::SubAck(p) => encode_suback(p, dst),
            Packet::Unsubscribe(p) => encode_unsubscribe(p, dst),
            Packet::UnsubAck(id) => encode_unsuback(*id, dst),
            Packet::PingReq => {
                dst.reserve(2);
                dst.put_u8(PINGREQ << 4);
                dst.put_u8(0);
            }
            Packet::PingResp => {
                dst.reserve(2);
                dst.put_u8(PINGRESP << 4);
                dst.put_u8(0);
            }
            Packet::Disconnect => {
                dst.reserve(2);
                dst.put_u8(DISCONNECT << 4);
                dst.put_u8(0);
            }
        }
    }

    pub fn packet_type_name(&self) -> &'static str {
        match self {
            Packet::Connect(_) => "CONNECT",
            Packet::ConnAck(_) => "CONNACK",
            Packet::Publish(_) => "PUBLISH",
            Packet::PubAck(_) => "PUBACK",
            Packet::Subscribe(_) => "SUBSCRIBE",
            Packet::SubAck(_) => "SUBACK",
            Packet::Unsubscribe(_) => "UNSUBSCRIBE",
            Packet::UnsubAck(_) => "UNSUBACK",
            Packet::PingReq => "PINGREQ",
            Packet::PingResp => "PINGRESP",
            Packet::Disconnect => "DISCONNECT",
        }
    }
}

fn read_remaining_length(src: &[u8]) -> Result<(usize, usize), DecodeError> {
    let mut multiplier: usize = 1;
    let mut value: usize = 0;
    let mut bytes = 0;

    loop {
        if bytes >= 4 {
            return Err(DecodeError::InvalidRemainingLength);
        }
        if bytes + 1 >= src.len() {
            return Err(DecodeError::Incomplete);
        }
        let byte = src[1 + bytes] as usize;
        value += (byte & 0x7F) * multiplier;
        bytes += 1;
        if (byte & 0x80) == 0 {
            break;
        }
        multiplier *= 128;
    }

    Ok((value, bytes))
}

fn write_remaining_length(len: usize, dst: &mut BytesMut) {
    if len >= 268_435_456 {
        panic!("remaining length too large");
    }
    let mut x = len;
    loop {
        let mut byte = (x % 128) as u8;
        x /= 128;
        if x > 0 {
            byte |= 0x80;
        }
        dst.put_u8(byte);
        if x == 0 {
            break;
        }
    }
}

fn read_u16(buf: &mut Bytes) -> Result<u16, DecodeError> {
    if buf.len() < 2 {
        return Err(DecodeError::Incomplete);
    }
    Ok(buf.get_u16())
}

fn read_utf8(buf: &mut Bytes) -> Result<String, DecodeError> {
    let len = read_u16(buf)? as usize;
    if buf.len() < len {
        return Err(DecodeError::Incomplete);
    }
    let data = buf.split_to(len);
    String::from_utf8(data.to_vec()).map_err(|_| DecodeError::InvalidUtf8)
}

fn write_u16(v: u16, dst: &mut BytesMut) {
    dst.put_u16(v);
}

fn write_utf8(s: &str, dst: &mut BytesMut) {
    write_u16(s.len() as u16, dst);
    dst.put_slice(s.as_bytes());
}

fn decode_connect(src: &mut Bytes, _flags: u8) -> Result<Connect, DecodeError> {
    let proto_name = read_utf8(src)?;
    if proto_name != "MQTT" {
        return Err(DecodeError::InvalidProtocolName);
    }

    if src.remaining() < 1 {
        return Err(DecodeError::Incomplete);
    }
    let proto_level = src.get_u8();
    if proto_level != 4 {
        return Err(DecodeError::InvalidProtocolLevel);
    }

    if src.remaining() < 1 {
        return Err(DecodeError::Incomplete);
    }
    let flags = src.get_u8();

    let clean_session = (flags & 0x02) != 0;
    let has_will = (flags & 0x04) != 0;
    let will_qos = (flags & 0x18) >> 3;
    let will_retain = (flags & 0x20) != 0;
    let has_username = (flags & 0x80) != 0;
    let has_password = (flags & 0x40) != 0;

    if src.remaining() < 2 {
        return Err(DecodeError::Incomplete);
    }
    let keep_alive = src.get_u16();
    let client_id = read_utf8(src)?;

    // Will — parsed before username/password per MQTT 3.1.1 §3.1
    let will = if has_will {
        let will_topic = read_utf8(src)?;
        let will_message = read_utf8(src)?;
        Some(LastWill {
            topic: will_topic,
            message: will_message,
            qos: will_qos,
            retain: will_retain,
        })
    } else {
        None
    };

    let username = if has_username { Some(read_utf8(src)?) } else { None };
    let password = if has_password {
        if src.remaining() < 2 {
            return Err(DecodeError::Incomplete);
        }
        let pw_len = src.get_u16() as usize;
        if src.remaining() < pw_len {
            return Err(DecodeError::Incomplete);
        }
        Some(src.split_to(pw_len).to_vec())
    } else {
        None
    };

    Ok(Connect {
        protocol_name: Some(proto_name),
        protocol_level: proto_level,
        client_id,
        clean_session,
        keep_alive,
        username,
        password,
        will,
    })
}

fn encode_connect(p: &Connect, dst: &mut BytesMut) {
    let mut payload = BytesMut::new();

    write_utf8("MQTT", &mut payload);
    payload.put_u8(4);

    let mut flags: u8 = 0;
    if let Some(ref w) = p.will {
        flags |= 0x04;
        flags |= (w.qos & 0x03) << 3;
        if w.retain { flags |= 0x20; }
    }
    if p.username.is_some() {
        flags |= 0x80;
    }
    if p.password.is_some() {
        flags |= 0x40;
    }
    if p.clean_session {
        flags |= 0x02;
    }
    payload.put_u8(flags);
    payload.put_u16(p.keep_alive);
    write_utf8(&p.client_id, &mut payload);
    if let Some(ref w) = p.will {
        write_utf8(&w.topic, &mut payload);
        write_utf8(&w.message, &mut payload);
    }
    if let Some(ref u) = p.username {
        write_utf8(u, &mut payload);
    }
    if let Some(ref pw) = p.password {
        payload.put_u16(pw.len() as u16);
        payload.put_slice(pw);
    }

    dst.put_u8(CONNECT << 4);
    write_remaining_length(payload.len(), dst);
    dst.unsplit(payload);
}

fn decode_connack(src: &mut Bytes) -> Result<ConnAck, DecodeError> {
    if src.remaining() < 2 {
        return Err(DecodeError::Incomplete);
    }
    let flags = src.get_u8();
    let code = src.get_u8();
    Ok(ConnAck {
        session_present: (flags & 0x01) != 0,
        return_code: ConnAckCode::from_u8(code),
    })
}

fn encode_connack(p: &ConnAck, dst: &mut BytesMut) {
    dst.put_u8(CONNACK << 4);
    dst.put_u8(2);
    dst.put_u8(if p.session_present { 1 } else { 0 });
    dst.put_u8(p.return_code as u8);
}

fn decode_publish(src: &mut Bytes, flags: u8) -> Result<Publish, DecodeError> {
    let dup = (flags & 0x08) != 0;
    let qos = (flags >> 1) & 0x03;
    let retain = (flags & 0x01) != 0;

    let topic = read_utf8(src)?;
    let packet_id = if qos > 0 {
        Some(read_u16(src)?)
    } else {
        None
    };

    let payload = src.split_to(src.remaining());

    Ok(Publish {
        dup,
        qos,
        retain,
        topic,
        packet_id,
        payload,
    })
}

fn encode_publish(p: &Publish, dst: &mut BytesMut) {
    let mut flags: u8 = 0;
    if p.dup {
        flags |= 0x08;
    }
    flags |= (p.qos & 0x03) << 1;
    if p.retain {
        flags |= 0x01;
    }

    let mut payload = BytesMut::new();
    write_utf8(&p.topic, &mut payload);
    if let Some(id) = p.packet_id {
        payload.put_u16(id);
    }
    payload.extend_from_slice(&p.payload);

    dst.put_u8(PUBLISH << 4 | flags);
    write_remaining_length(payload.len(), dst);
    dst.unsplit(payload);
}

fn encode_puback(packet_id: u16, dst: &mut BytesMut) {
    dst.put_u8(PUBACK << 4);
    dst.put_u8(2);
    dst.put_u16(packet_id);
}

fn decode_subscribe(src: &mut Bytes, _flags: u8) -> Result<Subscribe, DecodeError> {
    let packet_id = read_u16(src)?;
    let mut topics = Vec::new();
    while src.has_remaining() {
        let topic = read_utf8(src)?;
        if src.remaining() < 1 {
            return Err(DecodeError::Incomplete);
        }
        let qos = src.get_u8() & 0x03;
        topics.push((topic, qos));
    }
    Ok(Subscribe { packet_id, topics })
}

fn encode_subscribe(p: &Subscribe, dst: &mut BytesMut) {
    let mut payload = BytesMut::new();
    payload.put_u16(p.packet_id);
    for (topic, qos) in &p.topics {
        write_utf8(topic, &mut payload);
        payload.put_u8(*qos & 0x03);
    }

    dst.put_u8(SUBSCRIBE << 4 | 0x02);
    write_remaining_length(payload.len(), dst);
    dst.unsplit(payload);
}

fn decode_suback(src: &mut Bytes) -> Result<SubAck, DecodeError> {
    let packet_id = read_u16(src)?;
    let mut return_codes = Vec::with_capacity(src.remaining());
    while src.has_remaining() {
        return_codes.push(src.get_u8());
    }
    Ok(SubAck { packet_id, return_codes })
}

fn encode_suback(p: &SubAck, dst: &mut BytesMut) {
    let mut payload = BytesMut::new();
    payload.put_u16(p.packet_id);
    for rc in &p.return_codes {
        payload.put_u8(*rc);
    }

    dst.put_u8(SUBACK << 4);
    write_remaining_length(payload.len(), dst);
    dst.unsplit(payload);
}

fn decode_unsubscribe(src: &mut Bytes, _flags: u8) -> Result<Unsubscribe, DecodeError> {
    let packet_id = read_u16(src)?;
    let mut topics = Vec::new();
    while src.has_remaining() {
        topics.push(read_utf8(src)?);
    }
    Ok(Unsubscribe { packet_id, topics })
}

fn encode_unsubscribe(p: &Unsubscribe, dst: &mut BytesMut) {
    let mut payload = BytesMut::new();
    payload.put_u16(p.packet_id);
    for topic in &p.topics {
        write_utf8(topic, &mut payload);
    }

    dst.put_u8(UNSUBSCRIBE << 4 | 0x02);
    write_remaining_length(payload.len(), dst);
    dst.unsplit(payload);
}

fn encode_unsuback(packet_id: u16, dst: &mut BytesMut) {
    dst.put_u8(UNSUBACK << 4);
    dst.put_u8(2);
    dst.put_u16(packet_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(packet: Packet) {
        let mut buf = BytesMut::new();
        packet.encode(&mut buf);
        let decoded = Packet::decode(&mut buf).unwrap().unwrap();
        assert_eq!(packet, decoded);
    }

    #[test]
    fn test_connect_roundtrip() {
        roundtrip(Packet::Connect(Connect {
            protocol_name: Some("MQTT".into()),
            protocol_level: 4,
            client_id: "P1ABC123".into(),
            clean_session: true,
            keep_alive: 60,
            username: Some("device".into()),
            password: Some(b"secret".to_vec()),
            will: None,
        }));
    }

    #[test]
    fn test_connect_minimal() {
        roundtrip(Packet::Connect(Connect {
            protocol_name: Some("MQTT".into()),
            protocol_level: 4,
            client_id: "test".into(),
            clean_session: false,
            keep_alive: 30,
            username: None,
            password: None,
            will: None,
        }));
    }

    #[test]
    fn test_publish_qos0() {
        roundtrip(Packet::Publish(Publish {
            dup: false,
            qos: 0,
            retain: false,
            topic: "P1M5/ABC/data/readings".into(),
            packet_id: None,
            payload: Bytes::from("hello"),
        }));
    }

    #[test]
    fn test_publish_qos1() {
        roundtrip(Packet::Publish(Publish {
            dup: false,
            qos: 1,
            retain: false,
            topic: "test/topic".into(),
            packet_id: Some(42),
            payload: Bytes::from("data"),
        }));
    }

    #[test]
    fn test_simple_packets() {
        for pkt in [
            Packet::PingReq,
            Packet::PingResp,
            Packet::Disconnect,
        ] {
            let mut buf = BytesMut::new();
            pkt.encode(&mut buf);
            let decoded = Packet::decode(&mut buf).unwrap().unwrap();
            assert_eq!(pkt, decoded);
        }
    }

    #[test]
    fn test_subscribe_roundtrip() {
        roundtrip(Packet::Subscribe(Subscribe {
            packet_id: 1,
            topics: vec![
                ("P1M5/ABC/command".into(), 1),
                ("test/#".into(), 0),
            ],
        }));
    }

    #[test]
    fn test_incomplete_input() {
        let mut buf = BytesMut::from(&b"\x10"[..]);
        assert!(matches!(Packet::decode(&mut buf), Ok(None)));

        let mut buf = BytesMut::from(&b"\x10\x05"[..]);
        assert!(matches!(Packet::decode(&mut buf), Ok(None)));
    }
}