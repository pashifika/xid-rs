use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    process::Command,
    sync::{Arc, Barrier},
    thread,
};

use xid::{GenerationError, Id};

fn child(value: &OsStr, scenario: &str) {
    let output = Command::new(env::current_exe().unwrap())
        .args(["--ignored", "--exact", "provider_child", "--nocapture"])
        .env("XID_MACHINE_ID", value)
        .env("XID_TEST_SCENARIO", scenario)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn checked_and_legacy_generation_share_the_override_and_counter() {
    for value in &["0", "66051", "16777215"] {
        child(OsStr::new(value), "valid");
    }
}

#[test]
fn invalid_override_returns_an_error_without_poisoning_initialization() {
    child(OsStr::new("invalid"), "invalid");
}

#[cfg(unix)]
#[test]
fn non_unicode_override_returns_an_error() {
    use std::os::unix::ffi::OsStrExt;

    child(OsStr::from_bytes(&[0xff]), "invalid");
}

#[test]
fn concurrent_first_use_shares_one_generator() {
    child(OsStr::new("66051"), "concurrent");
}

// Each scenario runs in a fresh process: no shared environment or singleton
// mutation can race with another test, including under the default test runner.
#[test]
#[ignore = "invoked by parent scenarios with an isolated environment"]
fn provider_child() {
    match env::var("XID_TEST_SCENARIO").unwrap().as_str() {
        "invalid" => {
            // Parsing does not initialize the generator.
            assert_eq!("00000000000000000000".parse::<Id>().unwrap(), Id::default());
            for _ in 0..2 {
                assert!(matches!(
                    xid::try_new(),
                    Err(GenerationError::InvalidMachineIdOverride)
                ));
            }
        }
        "valid" => {
            let number: u32 = env::var("XID_MACHINE_ID").unwrap().parse().unwrap();
            let bytes = number.to_be_bytes();
            let first = xid::try_new().unwrap();
            let second = xid::new();
            assert_eq!(first.machine(), [bytes[1], bytes[2], bytes[3]]);
            assert_eq!(second.machine(), first.machine());
            assert_eq!(second.pid(), first.pid());
            assert_eq!(second.counter(), (first.counter() + 1) & 0x00FF_FFFF);
            assert_eq!(first.to_string().parse::<Id>().unwrap(), first);
        }
        "concurrent" => {
            let barrier = Arc::new(Barrier::new(8));
            let workers: Vec<_> = (0..8)
                .map(|_| {
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        (0..128)
                            .map(|_| xid::try_new().unwrap())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            let ids: Vec<_> = workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap())
                .collect();
            assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 1024);
            let counters: HashSet<_> = ids.iter().map(Id::counter).collect();
            assert_eq!(counters.len(), 1024);
            // A single contiguous run modulo 2^24 has exactly one counter
            // whose predecessor is absent, even when its random seed wraps.
            let starts = counters
                .iter()
                .filter(|counter| !counters.contains(&(counter.wrapping_sub(1) & 0x00FF_FFFF)))
                .count();
            assert_eq!(starts, 1);
            let pid = ids[0].pid();
            for id in ids {
                assert_eq!(id.machine(), [1, 2, 3]);
                assert_eq!(id.pid(), pid);
                assert_eq!(id.to_string().parse::<Id>().unwrap(), id);
            }
        }
        scenario => panic!("unknown child scenario: {}", scenario),
    }
}
