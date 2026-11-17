//! CLI entrypoint for `xf-desktop-fixture`.

use std::env;
use std::process::ExitCode;

use dim_xf_fixture::{run_fixture_process, FixtureCliConfig};

fn main() -> ExitCode {
    let cfg = FixtureCliConfig::from_args(env::args().skip(1));
    match run_fixture_process(&cfg) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("xf-desktop-fixture fatal error: {err}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dim_xf_fixture::{
        ControlledErrorMode, CrashMode, FixtureLockRecord, LockShareMode, OsFileLock, SplitMix64,
    };

    #[test]
    fn cli_parser_extracts_all_fault_and_isolation_flags() {
        let cfg = FixtureCliConfig::from_args(
            [
                "--instance-id=alpha-01",
                "--user-id",
                "849888777",
                "--slow-startup-pre-lock-ms=250",
                "--slow-startup-post-lock-ms",
                "120",
                "--hang-on-quit",
                "--crash-mode=mid-write",
                "--crash-delay-ms=90",
                "--write-files=4",
                "--write-bytes=1024",
                "--slow-io-ms=15",
                "--seed=0xdeadbeef",
                "--lock-mode=exclusive",
                "--spawn-children=2",
                "--orphan-children",
                "--error-mode=disk-full",
                "--set-kv",
                "theme=dark",
            ]
            .into_iter()
            .map(str::to_owned),
        );

        assert_eq!(cfg.instance_id, "alpha-01");
        assert_eq!(cfg.user_id, "849888777");
        assert_eq!(cfg.slow_startup_pre_lock_ms, 250);
        assert_eq!(cfg.slow_startup_post_lock_ms, 120);
        assert!(cfg.hang_on_quit);
        assert_eq!(cfg.crash_mode, CrashMode::MidWrite);
        assert_eq!(cfg.crash_delay_ms, 90);
        assert_eq!(cfg.write_files, 4);
        assert_eq!(cfg.write_bytes, 1024);
        assert_eq!(cfg.slow_io_ms, 15);
        assert_eq!(cfg.seed, 0xdead_beef);
        assert_eq!(cfg.lock_mode, LockShareMode::Exclusive);
        assert_eq!(cfg.spawn_children, 2);
        assert!(cfg.orphan_children);
        assert_eq!(cfg.error_mode, ControlledErrorMode::DiskFull);
        assert_eq!(
            cfg.persist_kv.get("theme").map(String::as_str),
            Some("dark")
        );
    }

    #[test]
    fn splitmix64_produces_reproducible_distinct_streams_per_seed() {
        let mut a1 = SplitMix64::new(42);
        let mut a2 = SplitMix64::new(42);
        let mut b = SplitMix64::new(43);

        let mut buf_a1 = [0u8; 64];
        let mut buf_a2 = [0u8; 64];
        let mut buf_b = [0u8; 64];
        a1.fill_bytes(&mut buf_a1);
        a2.fill_bytes(&mut buf_a2);
        b.fill_bytes(&mut buf_b);

        assert_eq!(buf_a1, buf_a2);
        assert_ne!(buf_a1, buf_b);
    }

    #[test]
    fn os_file_lock_blocks_second_holder_until_released() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let zdata = tmp.path().join("ZaloData");
        let lockfile = zdata.join("lockfile");
        let rec = FixtureLockRecord {
            pid: std::process::id(),
            instance_id: "lock-test".to_owned(),
            profile_id: Some("aabbccdd".to_owned()),
            started_at: "2026-11-17T00:00:00Z".to_owned(),
            app: "xf-desktop-fixture".to_owned(),
            lock_mode: LockShareMode::Exclusive,
        };

        let mut first = OsFileLock::try_acquire(&lockfile, &zdata, &rec)
            .expect("first acquire io")
            .expect("first lock should succeed");

        let second = OsFileLock::try_acquire(&lockfile, &zdata, &rec).expect("second acquire io");
        assert!(second.is_none(), "second concurrent OS lock must be denied");

        first.release_and_unlink();
        assert!(!lockfile.exists());

        let mut third = OsFileLock::try_acquire(&lockfile, &zdata, &rec)
            .expect("third acquire io")
            .expect("lock must succeed after release");
        third.release_and_unlink();
    }
}
