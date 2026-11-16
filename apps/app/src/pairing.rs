//! The QR payload that pairs the phone with this PC.
//!
//! The payload is deliberately small and human readable:
//!
//! ```text
//! recast://pair?hosts=192.168.1.82,10.0.0.5&port=47777&pin=123456
//! ```
//!
//! The phone tries every address in `hosts` before it gives up, so a PC with
//! Wi-Fi, Ethernet and a virtual adapter still pairs on the first scan.

/// Builds the text that is encoded in the pairing QR code.
pub fn payload(hosts: &[String], port: u16, pin: &str) -> String {
    let hosts = if hosts.is_empty() {
        "127.0.0.1".to_string()
    } else {
        hosts.join(",")
    };
    format!("recast://pair?hosts={hosts}&port={port}&pin={pin}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_contains_every_address() {
        let text = payload(
            &["192.168.1.82".to_string(), "10.0.0.5".to_string()],
            47777,
            "123456",
        );
        assert_eq!(
            text,
            "recast://pair?hosts=192.168.1.82,10.0.0.5&port=47777&pin=123456"
        );
    }

    #[test]
    fn payload_falls_back_to_loopback() {
        assert_eq!(
            payload(&[], 47777, "000000"),
            "recast://pair?hosts=127.0.0.1&port=47777&pin=000000"
        );
    }
}
