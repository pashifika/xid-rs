use std::env;

use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use sysctl::{Sysctl, SysctlError};

use crate::GenerationError;

// https://github.com/rs/xid/blob/34d39051ca0d70f40a8cd8c5491ef835e624b71d/id.go
pub fn get() -> Result<[u8; 3], GenerationError> {
    match env::var("XID_MACHINE_ID") {
        Ok(value) => {
            if let Some(id) = parse_override(&value)? {
                return Ok(id);
            }
        }
        Err(env::VarError::NotPresent) => {}
        Err(env::VarError::NotUnicode(_)) => {
            return Err(GenerationError::InvalidMachineIdOverride);
        }
    }
    derive(machine_id().ok(), hostname_fallback, getrandom::getrandom)
}

fn parse_override(value: &str) -> Result<Option<[u8; 3]>, GenerationError> {
    if value.is_empty() {
        return Ok(None);
    }
    // Bound work even for a large environment value. Go accepts a decimal sign;
    // this port additionally limits the magnitude text to eight ASCII digits.
    let digits = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(GenerationError::InvalidMachineIdOverride);
    }
    let number: i32 = value
        .parse()
        .map_err(|_| GenerationError::InvalidMachineIdOverride)?;
    if !(0..=0x00FF_FFFF).contains(&number) {
        return Err(GenerationError::InvalidMachineIdOverride);
    }
    let bytes = number.to_be_bytes();
    Ok(Some([bytes[1], bytes[2], bytes[3]]))
}

fn derive(
    platform_id: Option<String>,
    hostname: impl FnOnce() -> Option<String>,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<[u8; 3], GenerationError> {
    let id = platform_id
        .filter(|id| !id.is_empty())
        .or_else(|| hostname().filter(|name| !name.is_empty()));
    let mut bytes = [0_u8; 3];
    if let Some(id) = id {
        bytes.copy_from_slice(&Sha256::digest(id.as_bytes())[..3]);
    } else {
        fill(&mut bytes).map_err(GenerationError::MachineIdUnavailable)?;
    }
    Ok(bytes)
}

#[cfg(not(target_arch = "wasm32"))]
fn hostname_fallback() -> Option<String> {
    hostname::get().ok()?.into_string().ok()
}

// `hostname` does not support wasm; use the checked entropy fallback.
#[cfg(target_arch = "wasm32")]
fn hostname_fallback() -> Option<String> {
    None
}

// https://github.com/rs/xid/blob/efa678f304ab65d6d57eedcb086798381ae22206/hostid_linux.go
// Not checking "/sys/class/dmi/id/product_uuid" because normal users can't read it.
#[cfg(target_os = "linux")]
fn machine_id() -> std::io::Result<String> {
    use std::fs;
    // Get machine-id and remove the trailing new line.
    fs::read_to_string("/var/lib/dbus/machine-id")
        .or_else(|_| fs::read_to_string("/etc/machine-id"))
        .map(|s| s.trim_end().to_string())
}

// https://github.com/rs/xid/blob/efa678f304ab65d6d57eedcb086798381ae22206/hostid_darwin.go
#[cfg(target_os = "macos")]
fn machine_id() -> Result<String, SysctlError> {
    sysctl::Ctl::new("kern.uuid")?
        .value()
        .map(|v| v.to_string())
}

// https://github.com/rs/xid/blob/efa678f304ab65d6d57eedcb086798381ae22206/hostid_windows.go
#[cfg(target_os = "windows")]
fn machine_id() -> std::io::Result<String> {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let guid: String = hklm
        .open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Cryptography",
            winreg::enums::KEY_READ | winreg::enums::KEY_WOW64_64KEY,
        )?
        .get_value("MachineGuid")?;
    Ok(guid)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn machine_id() -> std::io::Result<String> {
    // Fallback to hostname or a random value
    Ok("".to_string())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn decimal_override_bounds_and_encoding() {
        for (value, expected) in &[
            ("", None),
            ("0", Some([0, 0, 0])),
            ("-0", Some([0, 0, 0])),
            ("+00000001", Some([0, 0, 1])),
            ("66051", Some([1, 2, 3])),
            ("16777215", Some([255, 255, 255])),
        ] {
            assert_eq!(parse_override(value).unwrap(), *expected);
        }
        for value in &[
            "-1",
            "16777216",
            "99999999",
            "000000000",
            " 1",
            "1 ",
            "+",
            "--0",
            "+-0",
            "0xff",
            "1.0",
            "１",
            "\0",
        ] {
            assert!(matches!(
                parse_override(value),
                Err(GenerationError::InvalidMachineIdOverride)
            ));
        }
        assert!(matches!(
            parse_override(&"0".repeat(1_000_000)),
            Err(GenerationError::InvalidMachineIdOverride)
        ));
    }

    #[test]
    fn platform_id_uses_sha256_without_fallbacks() {
        // SHA-256("abc") = ba7816bf..., not the former MD5 prefix 900150.
        assert_eq!(
            derive(
                Some("abc".to_owned()),
                || panic!("hostname must not replace a platform ID"),
                |_| panic!("entropy must not replace a platform ID"),
            )
            .unwrap(),
            [0xba, 0x78, 0x16]
        );
    }

    #[test]
    fn missing_or_empty_platform_uses_hostname_sha256() {
        for platform in &[None, Some(String::new())] {
            assert_eq!(
                derive(
                    platform.clone(),
                    || Some("abc".to_owned()),
                    |_| panic!("entropy must not replace a hostname"),
                )
                .unwrap(),
                [0xba, 0x78, 0x16]
            );
        }
    }

    #[test]
    fn missing_machine_and_hostname_require_entropy() {
        for hostname in &[None, Some(String::new())] {
            assert_eq!(
                derive(
                    None,
                    || hostname.clone(),
                    |bytes| {
                        bytes.copy_from_slice(&[1, 2, 3]);
                        Ok(())
                    },
                )
                .unwrap(),
                [1, 2, 3]
            );
            let failure = getrandom::Error::from(NonZeroU32::new(7).unwrap());
            assert!(matches!(
                derive(None, || hostname.clone(), |bytes| {
                    bytes[0] = 255;
                    Err(failure)
                }),
                Err(GenerationError::MachineIdUnavailable(source)) if source == failure
            ));
        }
    }
}
