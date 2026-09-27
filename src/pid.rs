use std::{fs, process};

use crc32fast::Hasher;

// 2 bytes of PID
// https://github.com/rs/xid/blob/34d39051ca0d70f40a8cd8c5491ef835e624b71d/id.go
pub fn get() -> u16 {
    contribution(process::id(), fs::read("/proc/self/cpuset").ok().as_deref())
}

fn contribution(pid: u32, cpuset: Option<&[u8]>) -> u16 {
    // Preserve Go's raw cpuset bytes and len > 1 rule, including its newline.
    let pid = match cpuset {
        Some(bytes) if bytes.len() > 1 => pid ^ crc32(bytes),
        _ => pid,
    };
    let bytes = pid.to_be_bytes();
    u16::from_be_bytes([bytes[2], bytes[3]])
}

fn crc32(buff: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(buff);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_id_uses_low_sixteen_bits() {
        for cpuset in &[None, Some(&b""[..]), Some(&b"/"[..])] {
            assert_eq!(contribution(0x1234_5678, *cpuset), 0x5678);
        }
    }

    #[test]
    fn container_bytes_contribute_ieee_crc32() {
        // Standard IEEE CRC-32 check vector: CRC32("123456789") = 0xcbf43926.
        assert_eq!(contribution(0x1234_5678, Some(b"123456789")), 0x6f5e);
        assert_ne!(
            contribution(0x1234_5678, Some(b"/\n")),
            contribution(0x1234_5678, Some(b"/"))
        );
    }
}
