#![deny(unsafe_code)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
//! Globally unique sortable id generator. A Rust port of <https://github.com/rs/xid>.
//!
//! The binary representation is compatible with the Mongo DB 12-byte
//! [`ObjectId`][object-id]. The value consists of:
//!
//! - a 4-byte timestamp value in seconds since the Unix epoch
//! - a 3-byte value based on the machine identifier
//! - a 2-byte value based on the process id
//! - a 3-byte incrementing counter, initialized to a random value
//!
//! The string representation is 20 bytes, using a base32 hex variant with
//! characters `[0-9a-v]` to retain the sortable property of the id.
//!
//! See the original [`xid`] project for more details.
//!
//! ## Usage
//!
//! ```
//! # fn main() -> Result<(), xid::GenerationError> {
//! println!("{}", xid::try_new()?);
//! # Ok(())
//! # }
//! ```
//!
//! [`xid`]:  https://github.com/rs/xid
//! [object-id]: https://docs.mongodb.org/manual/reference/object-id/
mod generator;
mod id;
mod machine_id;
mod pid;

pub use generator::GenerationError;
pub use id::{Id, ParseIdError};

/// Generate a new globally unique id.
///
/// Prefer [`try_new`] when generation failures must be handled.
///
/// # Panics
///
/// Panics if generator initialization fails or the clock is outside the
/// unsigned 32-bit XID timestamp range.
#[must_use]
pub fn new() -> Id {
    try_new().expect("XID generation failed")
}

/// Generate a new globally unique id without panicking on generation failures.
///
/// Initialization is lazy and shared by all threads. A failed initialization
/// publishes no generator; a later call may attempt initialization again.
///
/// # Errors
///
/// Returns [`GenerationError`] for an invalid `XID_MACHINE_ID`, unavailable
/// machine/entropy input, or a clock outside `0..=u32::MAX` whole seconds since
/// the Unix epoch. Successful allocation does not guarantee collision freedom
/// or chronological ordering across clock regressions or counter wrap.
pub fn try_new() -> Result<Id, GenerationError> {
    generator::get()?.new_id()
}
