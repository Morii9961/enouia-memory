//! Host pipe v1 framing; limits apply before allocating/serializing a body.

pub const DESCRIPTOR: &str = "host-pipe-v1;u8-tag/u32le-size;hello=1;call=2;lock=3;unlock=4;welcome=129;reply=130;ack=131;hello4096;control=ClientHello;request262144;reply1048576";
#[cfg(windows)]
pub(crate) use codec::*;

#[cfg(any(windows, test))]
mod codec {
    use serde_json::Value;
    use std::io::{self, Write};
    pub const REQUEST_LIMIT: usize = 256 * 1024;
    pub const REPLY_LIMIT: usize = 1024 * 1024;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(u8)]
    pub enum Tag {
        Hello = 1,
        Call = 2,
        Lock = 3,
        Unlock = 4,
        Welcome = 129,
        Reply = 130,
        Ack = 131,
    }
    impl Tag {
        pub fn decode(byte: u8, client: bool) -> io::Result<Self> {
            match (byte, client) {
                (1, true) => Ok(Self::Hello),
                (2, true) => Ok(Self::Call),
                (3, true) => Ok(Self::Lock),
                (4, true) => Ok(Self::Unlock),
                (129, false) => Ok(Self::Welcome),
                (130, false) => Ok(Self::Reply),
                (131, false) => Ok(Self::Ack),
                _ => Err(io::ErrorKind::InvalidData.into()),
            }
        }
        pub fn limit(self) -> usize {
            match self {
                Self::Hello | Self::Welcome | Self::Lock | Self::Unlock => {
                    enouia_memory_contract::host::session::MAX_HANDSHAKE_BYTES
                }
                Self::Call => REQUEST_LIMIT,
                Self::Reply => REPLY_LIMIT,
                Self::Ack => 64,
            }
        }
    }
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if b.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(io::ErrorKind::InvalidData.into());
            }
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    pub fn encode(value: &Value, limit: usize) -> io::Result<Vec<u8>> {
        let mut output = Bounded {
            bytes: Vec::new(),
            limit,
        };
        serde_json::to_writer(&mut output, value)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        Ok(output.bytes)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;
        #[test]
        fn budgets_count_encoded_bytes_and_tag_roles_do_not_overlap() {
            assert!(encode(&json!("\u{0}".repeat(1024)), 4096).is_err());
            let exact = encode(&json!("abc"), 5).unwrap();
            assert_eq!(exact, b"\"abc\"");
            assert!(encode(&json!("abc"), 4).is_err());
            for byte in [1, 2, 3, 4] {
                assert!(Tag::decode(byte, true).unwrap().limit() <= REQUEST_LIMIT);
                assert!(Tag::decode(byte, false).is_err());
            }
            for byte in [129, 130, 131] {
                assert!(Tag::decode(byte, false).unwrap().limit() <= REPLY_LIMIT);
                assert!(Tag::decode(byte, true).is_err());
            }
            for byte in [0, 128, 255] {
                assert!(Tag::decode(byte, false).is_err());
                assert!(Tag::decode(byte, true).is_err());
            }
        }
    }
}
