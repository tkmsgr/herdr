//! Each case runs in a child test process so PATH never changes under other tests
//! and no test can contact the user's real input method.
use super::{PrefixInputSource, RealPrefixInputSource};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn fcitx_prefix_preserves_input_state() {
    const TEST: &str = "platform::linux_prefix_tests::fcitx_prefix_preserves_input_state";
    if let Ok(case) = std::env::var("HERDR_TEST_FCITX_CASE") {
        let dir = std::env::var("HERDR_TEST_FCITX_DIR").unwrap();
        let state = std::path::Path::new(&dir).join("state");
        let initial = fs::read_to_string(&state).unwrap();
        let started = Instant::now();
        let mut source = RealPrefixInputSource::default();
        source.switch_to_ascii();
        if case == "active" {
            assert_eq!(fs::read_to_string(&state).unwrap().trim(), "1");
            source.switch_to_ascii();
            source.restore();
            assert_eq!(fs::read_to_string(&state).unwrap().trim(), "2");
            source.restore();
            {
                let mut dropped_source = RealPrefixInputSource::default();
                dropped_source.switch_to_ascii();
            }
            assert_eq!(fs::read_to_string(&state).unwrap().trim(), "2");
            assert_eq!(
                fs::read_to_string(format!("{dir}/calls")).unwrap(),
                "-c\n-o\n-c\n-o\n"
            );
        } else {
            source.restore();
            assert_eq!(fs::read_to_string(&state).unwrap(), initial);
            assert!(!std::path::Path::new(&format!("{dir}/calls")).exists());
        }
        assert!(
            started.elapsed().as_secs_f32() < 2.0,
            "IME must not freeze the UI"
        );
        return;
    }

    for (case, initial) in [
        ("active", "2"),
        ("inactive", "1"),
        ("unavailable", "0"),
        ("missing", "2"),
        ("failed", "2"),
        ("deactivate_failed", "2"),
        ("hung", "2"),
    ] {
        let dir = std::env::temp_dir().join(format!("herdr-fcitx-{}-{case}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("state"), initial).unwrap();
        if case != "missing" {
            let script = dir.join("fcitx5-remote");
            fs::write(
                &script,
                r#"#!/bin/sh
if [ "$1" = "--check" ]; then shift; fi
case "$HERDR_TEST_FCITX_CASE" in
  failed) exit 1 ;;
  hung) while :; do :; done ;;
esac
if [ "$HERDR_TEST_FCITX_CASE" = "deactivate_failed" ] && [ "$1" = "-c" ]; then exit 1; fi
case "$1" in
  '') read -r state < "$HERDR_TEST_FCITX_DIR/state"; echo "$state" ;;
  -c) echo 1 > "$HERDR_TEST_FCITX_DIR/state"; echo -c >> "$HERDR_TEST_FCITX_DIR/calls" ;;
  -o) echo 2 > "$HERDR_TEST_FCITX_DIR/state"; echo -o >> "$HERDR_TEST_FCITX_DIR/calls" ;;
  *) exit 1 ;;
esac
"#,
            )
            .unwrap();
            fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env("HERDR_TEST_FCITX_CASE", case)
            .env("HERDR_TEST_FCITX_DIR", &dir)
            .env("PATH", &dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Also reap a stuck fake controller if the timeout contract regresses.
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        let output = child.wait_with_output().unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(
            output.status.success(),
            "{case}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
