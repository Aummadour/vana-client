//! The voice chat's UDP packets: "VVC1", a kind byte, the sender's session, then the kind's fields.

use std::net::Ipv4Addr;

const MAGIC: &[u8; 4] = b"VVC1";
const HELLO: u8 = 1;
const AUDIO: u8 = 2;
const BYE: u8 = 3;
/// Names and channels on the wire, in bytes.
pub const NAME_MAX: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    /// A beacon: who this is and the channel it talks on.
    Hello { session: u64, name: String, channel: String },
    /// One frame (super::FRAME samples, 48 kHz mono 16-bit).
    Audio { session: u64, seq: u32, pcm: Vec<i16> },
    Bye { session: u64 },
}

/// Private, loopback and link-local IPv4: the only addresses voice goes to or is taken from.
pub fn lan(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_link_local()
}

/// `s` cut to `max` bytes on a char boundary, without control characters.
pub fn clip(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}

fn head(kind: u8, session: u64) -> Vec<u8> {
    let mut b = Vec::with_capacity(1024);
    b.extend_from_slice(MAGIC);
    b.push(kind);
    b.extend_from_slice(&session.to_le_bytes());
    b
}

fn text(b: &mut Vec<u8>, s: &str) {
    let s = clip(s, NAME_MAX);
    b.push(s.len() as u8);
    b.extend_from_slice(s.as_bytes());
}

pub fn encode(p: &Packet) -> Vec<u8> {
    match p {
        Packet::Hello { session, name, channel } => {
            let mut b = head(HELLO, *session);
            text(&mut b, name);
            text(&mut b, channel);
            b
        }
        Packet::Audio { session, seq, pcm } => {
            let mut b = head(AUDIO, *session);
            b.extend_from_slice(&seq.to_le_bytes());
            for s in pcm {
                b.extend_from_slice(&s.to_le_bytes());
            }
            b
        }
        Packet::Bye { session } => head(BYE, *session),
    }
}

pub fn decode(b: &[u8]) -> Option<Packet> {
    if b.len() < 13 || &b[..4] != MAGIC {
        return None;
    }
    let session = u64::from_le_bytes(b[5..13].try_into().ok()?);
    let rest = &b[13..];
    match b[4] {
        HELLO => {
            let (name, rest) = read_text(rest)?;
            let (channel, _) = read_text(rest)?;
            Some(Packet::Hello { session, name, channel })
        }
        AUDIO if rest.len() == 4 + super::FRAME * 2 => {
            let seq = u32::from_le_bytes(rest[..4].try_into().ok()?);
            let pcm = rest[4..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            Some(Packet::Audio { session, seq, pcm })
        }
        BYE => Some(Packet::Bye { session }),
        _ => None,
    }
}

fn read_text(b: &[u8]) -> Option<(String, &[u8])> {
    let n = *b.first()? as usize;
    let s = b.get(1..1 + n)?;
    Some((clip(&String::from_utf8_lossy(s), NAME_MAX), &b[1 + n..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let pcm: Vec<i16> = (0..super::super::FRAME as i32).map(|i| (i * 37 - 9000) as i16).collect();
        for p in [
            Packet::Hello { session: 7, name: "Nine".into(), channel: "".into() },
            Packet::Hello { session: u64::MAX, name: "Ünïcode".into(), channel: "party".into() },
            Packet::Audio { session: 3, seq: 99, pcm },
            Packet::Bye { session: 1 },
        ] {
            assert_eq!(decode(&encode(&p)), Some(p));
        }
        // Short audio, other magic and truncated names are refused.
        let mut a = encode(&Packet::Audio { session: 3, seq: 1, pcm: vec![0; super::super::FRAME] });
        a.pop();
        assert_eq!(decode(&a), None);
        assert_eq!(decode(b"XXXX\x01\0\0\0\0\0\0\0\0\x05ab"), None);
        assert_eq!(decode(b"VVC1\x01\0\0\0\0\0\0\0\0\x05ab"), None);
    }

    #[test]
    fn lan_only() {
        for ip in ["192.168.1.196", "10.0.0.2", "172.16.4.1", "127.0.0.1", "169.254.3.3"] {
            assert!(lan(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "100.64.0.1", "172.32.0.1", "239.255.70.88", "255.255.255.255"] {
            assert!(!lan(ip.parse().unwrap()), "{ip}");
        }
        assert_eq!(clip("ab\u{7}cdé", 5), "abcd");
    }
}
