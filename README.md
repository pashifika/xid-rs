# xid

[![CI](https://github.com/pashifika/xid-rs/workflows/CI/badge.svg)](https://github.com/pashifika/xid-rs/actions?query=workflow%3ACI)

Globally unique sortable id generator. A Rust port of https://github.com/rs/xid.

This fork maintains [kazk/xid-rs](https://github.com/kazk/xid-rs). Its checked
generation API is not part of the upstream crates.io `xid` 1.1.1 release.

The binary representation is compatible with the Mongo DB 12-byte [ObjectId][object-id].
The value consists of:

- a 4-byte timestamp value in seconds since the Unix epoch
- a 3-byte value based on the machine identifier
- a 2-byte value based on the process id
- a 3-byte incrementing counter, initialized to a random value

The string representation is 20 bytes, using a base32 hex variant with characters `[0-9a-v]`
to retain the sortable property of the id.

See the original [`xid`] project for more details.

## Usage

```rust
fn main() -> Result<(), xid::GenerationError> {
    let id = xid::try_new()?;
    let text = id.to_string();
    assert_eq!(text.parse::<xid::Id>().unwrap(), id);
    println!("{text}");
    Ok(())
}
```

## Examples

- [`cargo run --example gen`](./examples/gen.rs): Generate xid

## Checked generation

`try_new() -> Result<Id, GenerationError>` preserves the standard format and
returns typed errors for a clock before the Unix epoch, whole seconds above
`u32::MAX`, invalid `XID_MACHINE_ID`, unavailable machine/fallback entropy, or
unavailable counter entropy. Fractional seconds are discarded only after the
whole-second range check. `new() -> Id` remains available for compatibility and
panics on these errors; use `try_new()` in applications that must report failure.

Initialization is lazy, process-wide and thread-safe. Only successful
initialization is cached; a later call can retry a failed initialization.
Machine discovery falls back to the hostname and then operating-system entropy.
Machine and hostname strings are hashed with SHA-256, taking the first three
bytes. This changes newly generated machine components from the former MD5
derivation; already stored IDs and their parsing are unchanged.

Set `XID_MACHINE_ID` before the first allocation. An unset or empty value uses
machine discovery. Otherwise it must contain one to eight ASCII decimal digits,
optionally preceded by `+` or `-`, representing `0..=16777215`; `-0` is valid.
Whitespace, other text, non-Unicode values, longer digit strings and out-of-range
values return `InvalidMachineIdOverride` rather than silently using a fallback.
The value is encoded directly as three big-endian bytes, not hashed.

XIDs expose time, machine/process contributions and a counter. They are not
secrets, authentication tokens or collision-proof identifiers. The 24-bit
counter wraps; IDs from a fixed generator are ordered only while time does not
regress and the counter does not wrap within a second. There is no monotonic
clock correction or guarantee beyond the counter space. Do not inherit an
initialized generator across a process fork without an exec.

## Semantic maintenance record

Starting Rust revision:
[`9d1fd22d281c379362888bf729a927509f2d8ffc`](https://github.com/pashifika/xid-rs/tree/9d1fd22d281c379362888bf729a927509f2d8ffc).
Reviewed Go revision:
[`34d39051ca0d70f40a8cd8c5491ef835e624b71d`](https://github.com/rs/xid/tree/34d39051ca0d70f40a8cd8c5491ef835e624b71d).
These are review baselines, not a qualified consumer dependency pin.
The original MIT license and Olivier Poitrey/kazk attribution remain in
[`LICENSE.md`](LICENSE.md).

| Go concern | Rust location | Classification and evidence |
| --- | --- | --- |
| `id.go`: 4/3/2/3-byte layout and atomic counter | `src/generator.rs` | Already equivalent. Fixed generation vector, ordered fixed-time samples, 24-bit/32-bit wrap and concurrent allocation regressions. Rust retains fetch-before-increment; Go increments first. Both start from a uniformly random 24-bit counter. |
| `id.go`: unchecked timestamp and entropy panics | `src/generator.rs`, `src/lib.rs` | Fork-specific checked boundary rather than copying Go casts/panics. Epoch/max-second tests prove rejected times consume no counter; deterministic entropy errors retain their source. |
| `id.go`: SHA-256 machine derivation and override | `src/machine_id.rs` | Ported with bounded decimal validation and typed errors. SHA-256 known vector, source precedence, empty/missing sources, override limits and failed entropy regressions. |
| `id.go`: lazy machine resolution | `src/generator.rs`, `src/lib.rs` | Already equivalent through `OnceCell`; retain whole-generator laziness and publish only successful initialization. Isolated-process tests exercise invalid overrides and concurrent first use. |
| `id.go`: encoding, decoding, components, zero value, comparison | `src/id.rs` | Already equivalent through `Display`, `FromStr`, accessors, `Default` and `Ord`. Fixed vectors, every bit boundary, all final characters, malformed text and maximum components are covered. |
| `id.go`: process/container contribution | `src/pid.rs` | Already equivalent: low 16 PID bits XOR raw cpuset IEEE CRC-32 when its length exceeds one. Fixed CRC vector and root/empty input regressions preserve newline behavior. |
| `hostid_windows.go`: registry view | `src/machine_id.rs` | Ported explicit `KEY_WOW64_64KEY` with existing safe `winreg` access. Requires Windows runtime verification, including a 32-bit process on 64-bit Windows. |
| Other `hostid_*.go`: discovery sources | `src/machine_id.rs` | Equivalent fallback role, not byte-identical platform input. Keep Rust's macOS `kern.uuid` rather than spawning `ioreg`, and Linux dbus-then-etc machine ID with trailing whitespace removed. Other platforms retain hostname/entropy fallback; Go's BSD-specific sysctls are not added. |
| Go JSON, SQL and `b/` adapters | Existing Rust API | Not applicable: no matching Rust adapter surface. No adapter or Go-only convenience API is added solely for parity. |

## Verification

Run from this checkout:

```sh
cargo test --lib
cargo test --test checked_generation
cargo test --doc
cargo run --example gen
```

The integration tests launch isolated subprocesses, avoiding shared environment
mutation and nondeterministic global-generator resets. Run the same commands on
macOS, Linux and Windows to exercise host discovery and OS entropy; deterministic
unit tests alone do not qualify platform behavior. Cross-target `cargo check`
establishes compilation only, not registry/sysctl, process/container or runtime
behavior. No new browser/Wasm or BSD-specific qualification is claimed.

Consumers should pin an immutable public fork revision only after their own
allocation, parse, persistence and reopen smoke succeeds.

[`xid`]:  https://github.com/rs/xid
[object-id]: https://docs.mongodb.org/manual/reference/object-id/
