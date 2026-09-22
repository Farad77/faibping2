//! GameTunnel UDP Protocol Wire Format
//!
//! Header layout (12 bytes fixed):
//! 0x00-0x01: MagicNumber = 0x4754 ("GT")
//! 0x02     : Flags (Bit 0: IS_DUP, Bit 1: IS_PROBE, Bit 2: IS_ACK)
//! 0x03     : Reserved (0x00)
//! 0x04-0x07: SequenceNumber (uint32 big-endian)
//! 0x08-0x0B: Timestamp (uint32 ms big-endian)
//! 0x0C+    : Payload (raw IP packet)

pub const MAGIC_NUMBER: u16 = 0x4754; // 'G', 'T'
pub const HEADER_LEN: usize = 12;

pub const FLAG_IS_DUP: u8   = 1 << 0; // 0x01
pub const FLAG_IS_PROBE: u8 = 1 << 1; // 0x02
pub const FLAG_IS_ACK: u8   = 1 << 2; // 0x04

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameTunnelHeader {
    pub flags: u8,
    pub reserved: u8,
    pub sequence: u32,
    pub timestamp: u32,
}

impl GameTunnelHeader {
    #[inline]
    pub fn new(sequence: u32, timestamp: u32, is_dup: bool, is_probe: bool) -> Self {
        let mut flags = 0u8;
        if is_dup {
            flags |= FLAG_IS_DUP;
        }
        if is_probe {
            flags |= FLAG_IS_PROBE;
        }
        Self {
            flags,
            reserved: 0,
            sequence,
            timestamp,
        }
    }

    #[inline]
    pub fn is_dup(&self) -> bool {
        (self.flags & FLAG_IS_DUP) != 0
    }

    #[inline]
    pub fn is_probe(&self) -> bool {
        (self.flags & FLAG_IS_PROBE) != 0
    }

    #[inline]
    pub fn is_ack(&self) -> bool {
        (self.flags & FLAG_IS_ACK) != 0
    }

    #[inline]
    pub fn set_ack(&mut self) {
        self.flags |= FLAG_IS_ACK;
    }

    #[inline]
    pub fn encode(&self, buf: &mut [u8]) -> Result<(), ProtocolError> {
        if buf.len() < HEADER_LEN {
            return Err(ProtocolError::BufferTooShort);
        }
        buf[0..2].copy_from_slice(&MAGIC_NUMBER.to_be_bytes());
        buf[2] = self.flags;
        buf[3] = self.reserved;
        buf[4..8].copy_from_slice(&self.sequence.to_be_bytes());
        buf[8..12].copy_from_slice(&self.timestamp.to_be_bytes());
        Ok(())
    }

    #[inline]
    pub fn decode(buf: &[u8]) -> Result<Self, ProtocolError> {
        if buf.len() < HEADER_LEN {
            return Err(ProtocolError::BufferTooShort);
        }
        let magic = u16::from_be_bytes([buf[0], buf[1]]);
        if magic != MAGIC_NUMBER {
            return Err(ProtocolError::InvalidMagic(magic));
        }
        let flags = buf[2];
        let reserved = buf[3];
        let sequence = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let timestamp = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);

        Ok(Self {
            flags,
            reserved,
            sequence,
            timestamp,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProtocolError {
    BufferTooShort,
    InvalidMagic(u16),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort => write!(f, "buffer is shorter than GameTunnel header (12 bytes)"),
            Self::InvalidMagic(m) => write!(f, "invalid GameTunnel magic number: 0x{:04X}", m),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let header = GameTunnelHeader::new(133742, 987654, true, false);
        let mut buf = [0u8; HEADER_LEN + 10];
        header.encode(&mut buf).unwrap();

        let decoded = GameTunnelHeader::decode(&buf).unwrap();
        assert_eq!(decoded.sequence, 133742);
        assert_eq!(decoded.timestamp, 987654);
        assert!(decoded.is_dup());
        assert!(!decoded.is_probe());
        assert!(!decoded.is_ack());
    }

    #[test]
    fn test_invalid_magic() {
        let mut buf = [0u8; HEADER_LEN];
        buf[0] = 0x12;
        buf[1] = 0x34;
        assert!(matches!(
            GameTunnelHeader::decode(&buf),
            Err(ProtocolError::InvalidMagic(0x1234))
        ));
    }
}
