use std::{
    str::{self, FromStr},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

pub(crate) const RAW_LEN: usize = 12;
const ENCODED_LEN: usize = 20;
const ENC: &[u8] = "0123456789abcdefghijklmnopqrstuv".as_bytes();
const DEC: [u8; 256] = gen_dec();
pub(crate) const ZERO: Id = Id([0u8; RAW_LEN]);

/// An ID.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Default)]
pub struct Id(pub [u8; RAW_LEN]);

impl Id {
    /// Create an Id from a bytes slice.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the length of `bytes` is invalid.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseIdError> {
        if bytes.len() != RAW_LEN {
            return Err(ParseIdError::InvalidLength(bytes.len()));
        }

        let mut id = [0u8; RAW_LEN];
        id.copy_from_slice(bytes);
        Ok(Id(id))
    }

    /// The binary representation of the id.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; RAW_LEN] {
        let Self(raw) = self;
        raw
    }

    /// Extract the 3-byte machine id.
    #[must_use]
    pub fn machine(&self) -> [u8; 3] {
        let raw = self.as_bytes();
        [raw[4], raw[5], raw[6]]
    }

    /// Extract the process id.
    #[must_use]
    pub fn pid(&self) -> u16 {
        let raw = self.as_bytes();
        u16::from_be_bytes([raw[7], raw[8]])
    }

    /// Extract the timestamp.
    #[must_use]
    pub fn time(&self) -> SystemTime {
        let raw = self.as_bytes();
        let unix_ts = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
        UNIX_EPOCH + Duration::from_secs(u64::from(unix_ts))
    }

    /// Extract the incrementing counter.
    #[must_use]
    pub fn counter(&self) -> u32 {
        // Counter is stored as big-endian 3-byte value
        let raw = self.as_bytes();
        u32::from_be_bytes([0, raw[9], raw[10], raw[11]])
    }

    /// Returns true if this is a "zero" ID
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == ZERO.0
    }
}

impl std::fmt::Display for Id {
    // https://github.com/rs/xid/blob/efa678f304ab65d6d57eedcb086798381ae22206/id.go#L208
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self(raw) = self;
        let mut bs = [0_u8; ENCODED_LEN];
        bs[19] = ENC[((raw[11] << 4) & 31) as usize];
        bs[18] = ENC[((raw[11] >> 1) & 31) as usize];
        bs[17] = ENC[(((raw[11] >> 6) | (raw[10] << 2)) & 31) as usize];
        bs[16] = ENC[(raw[10] >> 3) as usize];
        bs[15] = ENC[(raw[9] & 31) as usize];
        bs[14] = ENC[(((raw[9] >> 5) | (raw[8] << 3)) & 31) as usize];
        bs[13] = ENC[((raw[8] >> 2) & 31) as usize];
        bs[12] = ENC[(((raw[8] >> 7) | (raw[7] << 1)) & 31) as usize];
        bs[11] = ENC[(((raw[7] >> 4) | (raw[6] << 4)) & 31) as usize];
        bs[10] = ENC[((raw[6] >> 1) & 31) as usize];
        bs[9] = ENC[(((raw[6] >> 6) | (raw[5] << 2)) & 31) as usize];
        bs[8] = ENC[(raw[5] >> 3) as usize];
        bs[7] = ENC[(raw[4] & 31) as usize];
        bs[6] = ENC[(((raw[4] >> 5) | (raw[3] << 3)) & 31) as usize];
        bs[5] = ENC[((raw[3] >> 2) & 31) as usize];
        bs[4] = ENC[(((raw[3] >> 7) | (raw[2] << 1)) & 31) as usize];
        bs[3] = ENC[(((raw[2] >> 4) | (raw[1] << 4)) & 31) as usize];
        bs[2] = ENC[((raw[1] >> 1) & 31) as usize];
        bs[1] = ENC[(((raw[1] >> 6) | (raw[0] << 2)) & 31) as usize];
        bs[0] = ENC[(raw[0] >> 3) as usize];
        write!(f, "{}", str::from_utf8(&bs).expect("valid utf8"))
    }
}

impl std::fmt::Debug for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Id").field(&self.to_string()).finish()
    }
}

/// An error which can be returned when parsing an id.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum ParseIdError {
    /// Returned when the input is not 12 raw bytes or 20 encoded bytes.
    #[error("invalid length {0}")]
    InvalidLength(usize),
    /// Returned when the id had character not in `[0-9a-v]`.
    #[error("invalid character '{0}'")]
    InvalidCharacter(char),
}

impl FromStr for Id {
    type Err = ParseIdError;

    // https://github.com/rs/xid/blob/efa678f304ab65d6d57eedcb086798381ae22206/id.go#L259
    /// Create an Id from its string representation.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 20 {
            return Err(ParseIdError::InvalidLength(s.len()));
        }
        if let Some(c) = s.chars().find(|&c| !matches!(c, '0'..='9' | 'a'..='v')) {
            return Err(ParseIdError::InvalidCharacter(c));
        }

        let bs = s.as_bytes();
        let mut raw = [0_u8; RAW_LEN];
        raw[11] = DEC[bs[17] as usize] << 6 | DEC[bs[18] as usize] << 1 | DEC[bs[19] as usize] >> 4;
        // check the last byte
        if ENC[((raw[11] << 4) & 31) as usize] != bs[19] {
            return Err(ParseIdError::InvalidCharacter(bs[19] as char));
        }

        raw[10] = DEC[bs[16] as usize] << 3 | DEC[bs[17] as usize] >> 2;
        raw[9] = DEC[bs[14] as usize] << 5 | DEC[bs[15] as usize];
        raw[8] = DEC[bs[12] as usize] << 7 | DEC[bs[13] as usize] << 2 | DEC[bs[14] as usize] >> 3;
        raw[7] = DEC[bs[11] as usize] << 4 | DEC[bs[12] as usize] >> 1;
        raw[6] = DEC[bs[9] as usize] << 6 | DEC[bs[10] as usize] << 1 | DEC[bs[11] as usize] >> 4;
        raw[5] = DEC[bs[8] as usize] << 3 | DEC[bs[9] as usize] >> 2;
        raw[4] = DEC[bs[6] as usize] << 5 | DEC[bs[7] as usize];
        raw[3] = DEC[bs[4] as usize] << 7 | DEC[bs[5] as usize] << 2 | DEC[bs[6] as usize] >> 3;
        raw[2] = DEC[bs[3] as usize] << 4 | DEC[bs[4] as usize] >> 1;
        raw[1] = DEC[bs[1] as usize] << 6 | DEC[bs[2] as usize] << 1 | DEC[bs[3] as usize] >> 4;
        raw[0] = DEC[bs[0] as usize] << 3 | DEC[bs[1] as usize] >> 2;
        Ok(Self(raw))
    }
}

#[rustfmt::skip]
const fn gen_dec() -> [u8; 256] {
    let mut dec = [0_u8; 256];
    // Fill in ranges b'0'..=b'9' and b'a'..=b'v'.
    // dec[48..=57].copy_from_slice(&(0..=9).collect::<Vec<u8>>());
    dec[48] = 0; dec[49] = 1; dec[50] = 2; dec[51] = 3; dec[52] = 4;
    dec[53] = 5; dec[54] = 6; dec[55] = 7; dec[56] = 8; dec[57] = 9;
    // dec[97..=118].copy_from_slice(&(10..=31).collect::<Vec<u8>>());
    dec[ 97] = 10; dec[ 98] = 11; dec[ 99] = 12; dec[100] = 13;
    dec[101] = 14; dec[102] = 15; dec[103] = 16; dec[104] = 17;
    dec[105] = 18; dec[106] = 19; dec[107] = 20; dec[108] = 21;
    dec[109] = 22; dec[110] = 23; dec[111] = 24; dec[112] = 25;
    dec[113] = 26; dec[114] = 27; dec[115] = 28; dec[116] = 29;
    dec[117] = 30; dec[118] = 31;
    dec
}

#[cfg(test)]
mod tests {
    use super::*;

    // https://github.com/rs/xid/blob/34d39051ca0d70f40a8cd8c5491ef835e624b71d/id_test.go
    #[test]
    fn fixed_binary_text_vectors() {
        for (raw, text) in &[
            (
                [
                    0x4d, 0x88, 0xe1, 0x5b, 0x60, 0xf4, 0x86, 0xe4, 0x28, 0x41, 0x2d, 0xc9,
                ],
                "9m4e2mr0ui3e8a215n4g",
            ),
            ([0; RAW_LEN], "00000000000000000000"),
            ([255; RAW_LEN], "vvvvvvvvvvvvvvvvvvvg"),
        ] {
            let id = Id::from_bytes(raw).unwrap();
            assert_eq!(id.to_string(), *text);
            assert_eq!(Id::from_str(text).unwrap(), id);
            assert_eq!(id.as_bytes(), raw);
        }
    }

    #[test]
    fn malformed_lengths_and_alphabet_are_rejected() {
        for text in &["", "9m4e2mr0ui3e8a215n4", "9m4e2mr0ui3e8a215n4g0"] {
            assert_eq!(
                Id::from_str(text),
                Err(ParseIdError::InvalidLength(text.len()))
            );
        }
        for length in &[0, 11, 13, 20] {
            assert_eq!(
                Id::from_bytes(&vec![0; *length]),
                Err(ParseIdError::InvalidLength(*length))
            );
        }
        for (text, character) in &[
            ("9M4e2mr0ui3e8a215n4g", 'M'),
            ("9w4e2mr0ui3e8a215n4g", 'w'),
            ("9z4e2mr0ui3e8a215n4g", 'z'),
            ("9-4e2mr0ui3e8a215n4g", '-'),
            ("9 4e2mr0ui3e8a215n4g", ' '),
            ("9\u{0}4e2mr0ui3e8a215n4g", '\0'),
            ("000000000000000000é", 'é'),
        ] {
            assert_eq!(
                Id::from_str(text),
                Err(ParseIdError::InvalidCharacter(*character))
            );
        }
    }

    #[test]
    fn only_zero_padding_bits_are_canonical() {
        for &character in ENC {
            let mut text = *b"00000000000000000000";
            text[19] = character;
            let parsed = Id::from_str(str::from_utf8(&text).unwrap());
            if character == b'0' || character == b'g' {
                let mut raw = [0; RAW_LEN];
                raw[11] = u8::from(character == b'g');
                let id = parsed.unwrap();
                assert_eq!(id.as_bytes(), &raw);
                assert_eq!(id.to_string().as_bytes(), &text);
            } else {
                assert_eq!(
                    parsed,
                    Err(ParseIdError::InvalidCharacter(char::from(character)))
                );
            }
        }
    }

    fn bitwise_encoding(raw: &[u8; RAW_LEN]) -> String {
        // Independent, deliberately simple oracle for the unrolled codec.
        (0..ENCODED_LEN)
            .map(|group| {
                let mut value = 0_u8;
                for offset in 0..5 {
                    let bit = group * 5 + offset;
                    value <<= 1;
                    if bit < RAW_LEN * 8 {
                        value |= (raw[bit / 8] >> (7 - bit % 8)) & 1;
                    }
                }
                char::from(ENC[usize::from(value)])
            })
            .collect()
    }

    #[test]
    fn codec_matches_bitwise_encoding_at_every_bit_boundary() {
        let mut previous = Id([0; RAW_LEN]);
        for bit in (0..RAW_LEN * 8).rev() {
            let mut raw = [0; RAW_LEN];
            raw[bit / 8] = 1 << (7 - bit % 8);
            let id = Id(raw);
            let text = bitwise_encoding(&raw);
            assert_eq!(id.to_string(), text);
            assert_eq!(Id::from_str(&text).unwrap(), id);
            assert!(previous < id);
            assert!(previous.to_string() < text);
            previous = id;
        }
        for byte in 0..=u8::MAX {
            let raw = [byte; RAW_LEN];
            let text = bitwise_encoding(&raw);
            assert_eq!(Id(raw).to_string(), text);
            assert_eq!(Id::from_str(&text).unwrap().as_bytes(), &raw);
        }
    }

    // https://github.com/rs/xid/blob/efa678f304ab65d6d57eedcb086798381ae22206/id_test.go#L45
    #[test]
    fn test_extraction() {
        struct IDParts {
            raw: [u8; RAW_LEN],
            timestamp: u64,
            machine_id: [u8; 3],
            pid: u16,
            counter: u32,
        }

        let tests = vec![
            IDParts {
                raw: [
                    0x4d, 0x88, 0xe1, 0x5b, 0x60, 0xf4, 0x86, 0xe4, 0x28, 0x41, 0x2d, 0xc9,
                ],
                timestamp: 1_300_816_219,
                machine_id: [0x60, 0xf4, 0x86],
                pid: 0xe428,
                counter: 4_271_561,
            },
            IDParts {
                raw: [
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ],
                timestamp: 0,
                machine_id: [0x00, 0x00, 0x00],
                pid: 0x0000,
                counter: 0,
            },
            IDParts {
                raw: [
                    0x00, 0x00, 0x00, 0x00, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0x00, 0x00, 0x01,
                ],
                timestamp: 0,
                machine_id: [0xaa, 0xbb, 0xcc],
                pid: 0xddee,
                counter: 1,
            },
            IDParts {
                raw: [255; RAW_LEN],
                timestamp: u64::from(u32::MAX),
                machine_id: [255; 3],
                pid: u16::MAX,
                counter: 0x00FF_FFFF,
            },
        ];

        for t in tests {
            let id = Id(t.raw);
            assert_eq!(
                id.time().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                t.timestamp
            );
            assert_eq!(id.machine(), t.machine_id);
            assert_eq!(id.pid(), t.pid);
            assert_eq!(id.counter(), t.counter);

            let rt = Id::from_bytes(id.as_bytes());
            assert!(rt.is_ok());
            assert_eq!(rt.unwrap(), id);
        }
    }

    #[test]
    fn test_default() {
        let id = Id::default();
        assert!(id.is_zero());
        assert_eq!("00000000000000000000", id.to_string().as_str());
        assert_eq!(Id::from_str("00000000000000000000").unwrap(), id);
        assert_eq!(
            id.time().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            0u64,
        );
        assert_eq!(id.machine(), [0u8; 3]);
        assert_eq!(id.pid(), 0u16);
        assert_eq!(id.counter(), 0u32);
    }
}
