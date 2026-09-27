use std::{
    convert::TryFrom,
    sync::atomic::{AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use once_cell::sync::OnceCell;
use thiserror::Error;

use crate::{
    id::{Id, RAW_LEN},
    machine_id, pid,
};

/// A failure to initialize the generator or represent its timestamp.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GenerationError {
    /// The system clock is before the Unix epoch.
    #[error("XID clock is before the Unix epoch")]
    BeforeUnixEpoch,
    /// Whole seconds since the Unix epoch exceed the 32-bit XID field.
    #[error("XID timestamp {0} exceeds u32::MAX seconds")]
    TimestampOverflow(u64),
    /// The override is not a bounded decimal integer in `0..=0xFF_FFFF`.
    #[error("XID_MACHINE_ID must contain at most eight decimal digits, an optional sign, and a value in 0..=16777215")]
    InvalidMachineIdOverride,
    /// Neither a machine identifier nor fallback random bytes were available.
    #[error("XID machine identifier and fallback entropy are unavailable: {0}")]
    MachineIdUnavailable(#[source] getrandom::Error),
    /// The counter could not be seeded from the operating system.
    #[error("XID counter entropy is unavailable: {0}")]
    EntropyUnavailable(#[source] getrandom::Error),
}

#[derive(Debug)]
pub struct Generator {
    counter: AtomicU32,
    machine_id: [u8; 3],
    pid: [u8; 2],
}

pub fn get() -> Result<&'static Generator, GenerationError> {
    static INSTANCE: OnceCell<Generator> = OnceCell::new();

    INSTANCE.get_or_try_init(|| {
        Ok(Generator {
            machine_id: machine_id::get()?,
            counter: AtomicU32::new(init_random(getrandom::getrandom)?),
            pid: pid::get().to_be_bytes(),
        })
    })
}

impl Generator {
    pub fn new_id(&self) -> Result<Id, GenerationError> {
        self.with_time(&SystemTime::now())
    }

    #[cfg_attr(target_os = "windows", allow(clippy::trivially_copy_pass_by_ref))]
    fn with_time(&self, time: &SystemTime) -> Result<Id, GenerationError> {
        let seconds = time
            .duration_since(UNIX_EPOCH)
            .map_err(|_| GenerationError::BeforeUnixEpoch)?
            .as_secs();
        let timestamp =
            u32::try_from(seconds).map_err(|_| GenerationError::TimestampOverflow(seconds))?;
        Ok(self.generate(timestamp))
    }

    fn generate(&self, unix_ts: u32) -> Id {
        let counter = self.counter.fetch_add(1, Ordering::SeqCst);

        let mut raw = [0_u8; RAW_LEN];
        // 4 bytes of Timestamp (big endian)
        raw[0..=3].copy_from_slice(&unix_ts.to_be_bytes());
        // 3 bytes of Machine ID
        raw[4..=6].copy_from_slice(&self.machine_id);
        // 2 bytes of PID
        raw[7..=8].copy_from_slice(&self.pid);
        // 3 bytes of increment counter (big endian)
        raw[9..].copy_from_slice(&counter.to_be_bytes()[1..]);

        Id(raw)
    }
}

// https://github.com/rs/xid/blob/34d39051ca0d70f40a8cd8c5491ef835e624b71d/id.go
fn init_random(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<u32, GenerationError> {
    let mut bytes = [0_u8; 3];
    fill(&mut bytes).map_err(GenerationError::EntropyUnavailable)?;
    Ok(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, num::NonZeroU32, sync::Arc, thread, time::Duration};

    use super::*;

    fn generator(counter: u32) -> Generator {
        Generator {
            counter: AtomicU32::new(counter),
            machine_id: [0x60, 0xf4, 0x86],
            pid: [0xe4, 0x28],
        }
    }

    #[test]
    fn fixed_generation_matches_go_vector() {
        let generator = generator(4_271_561);
        let time = UNIX_EPOCH + Duration::from_secs(1_300_816_219);
        let id = generator.with_time(&time).unwrap();
        assert_eq!(
            id.as_bytes(),
            &[0x4d, 0x88, 0xe1, 0x5b, 0x60, 0xf4, 0x86, 0xe4, 0x28, 0x41, 0x2d, 0xc9]
        );
        assert_eq!(id.to_string(), "9m4e2mr0ui3e8a215n4g");
    }

    #[test]
    fn invalid_timestamps_do_not_consume_a_counter() {
        let generator = generator(7);
        assert!(matches!(
            generator.with_time(&(UNIX_EPOCH - Duration::from_secs(1))),
            Err(GenerationError::BeforeUnixEpoch)
        ));
        let overflow = u64::from(u32::MAX) + 1;
        assert!(matches!(
            generator.with_time(&(UNIX_EPOCH + Duration::from_secs(overflow))),
            Err(GenerationError::TimestampOverflow(seconds)) if seconds == overflow
        ));
        let first = generator.with_time(&UNIX_EPOCH).unwrap();
        assert_eq!(first.time(), UNIX_EPOCH);
        assert_eq!(first.counter(), 7);
        let last_second = UNIX_EPOCH + Duration::new(u64::from(u32::MAX), 999_999_999);
        let last = generator.with_time(&last_second).unwrap();
        assert_eq!(
            last.time(),
            UNIX_EPOCH + Duration::from_secs(u64::from(u32::MAX))
        );
        assert_eq!(last.counter(), 8);
    }

    #[test]
    fn same_generator_is_ordered_until_counter_wrap() {
        let generator = generator(0);
        let time = UNIX_EPOCH + Duration::from_secs(42);
        let mut previous = generator.with_time(&time).unwrap();
        for counter in 1..1024 {
            let id = generator.with_time(&time).unwrap();
            assert!(previous < id);
            assert!(previous.to_string() < id.to_string());
            assert_eq!(id.counter(), counter);
            previous = id;
        }
    }

    #[test]
    fn counter_wrap_preserves_other_components() {
        for seed in &[0x00FF_FFFE, u32::MAX - 1] {
            let generator = generator(*seed);
            let time = UNIX_EPOCH + Duration::from_secs(42);
            let ids: Vec<_> = (0..4)
                .map(|_| generator.with_time(&time).unwrap())
                .collect();
            let counters: Vec<_> = ids.iter().map(Id::counter).collect();
            assert_eq!(counters, [0x00FF_FFFE, 0x00FF_FFFF, 0, 1]);
            assert!(ids[1] > ids[2]);
            for id in ids {
                assert_eq!(id.time(), time);
                assert_eq!(id.machine(), [0x60, 0xf4, 0x86]);
                assert_eq!(id.pid(), 0xe428);
            }
        }
    }

    #[test]
    fn supported_clock_regression_is_not_clamped() {
        let generator = generator(0);
        let later = generator
            .with_time(&(UNIX_EPOCH + Duration::from_secs(2)))
            .unwrap();
        let earlier = generator
            .with_time(&(UNIX_EPOCH + Duration::from_secs(1)))
            .unwrap();
        assert!(earlier < later);
        assert_eq!(earlier.time(), UNIX_EPOCH + Duration::from_secs(1));
        assert_eq!(earlier.counter(), 1);
    }

    #[test]
    fn counter_seed_is_big_endian_and_entropy_failure_is_returned() {
        let seed = init_random(|bytes| {
            bytes.copy_from_slice(&[0x12, 0x34, 0x56]);
            Ok(())
        })
        .unwrap();
        assert_eq!(seed, 0x0012_3456);
        let failure = getrandom::Error::from(NonZeroU32::new(7).unwrap());
        assert!(matches!(
            init_random(|bytes| {
                bytes[0] = 255;
                Err(failure)
            }),
            Err(GenerationError::EntropyUnavailable(source)) if source == failure
        ));
    }

    #[test]
    fn concurrent_allocation_reserves_each_counter_once_across_wrap() {
        let seed = u32::MAX - 4095;
        let generator = Arc::new(generator(seed));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let generator = Arc::clone(&generator);
                thread::spawn(move || {
                    (0..1024)
                        .map(|_| generator.with_time(&UNIX_EPOCH).unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let ids: Vec<_> = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect();
        let counters: BTreeSet<_> = ids.iter().map(Id::counter).collect();
        let expected: BTreeSet<_> = (0..8192_u32)
            .map(|offset| seed.wrapping_add(offset) & 0x00FF_FFFF)
            .collect();
        assert_eq!(counters, expected);
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 8192);
        for id in ids {
            assert_eq!(id.time(), UNIX_EPOCH);
            assert_eq!(id.machine(), [0x60, 0xf4, 0x86]);
            assert_eq!(id.pid(), 0xe428);
        }
    }
}
