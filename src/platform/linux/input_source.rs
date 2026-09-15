//! Fcitx runs in the foreground client's desktop session, never on the server.
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const CONTROLLER_TIMEOUT: Duration = Duration::from_millis(200);

/// Present only when we deactivated a previously active input method.
#[derive(Debug)]
pub(crate) struct InputSourceRestore;

impl Drop for InputSourceRestore {
    fn drop(&mut self) {
        // Do not activate an unavailable controller or toggle an already active IME.
        if fcitx_remote(&[]).as_deref() == Some("1") && fcitx_remote(&["-o"]).is_none() {
            tracing::warn!("failed to restore Fcitx input source after prefix mode");
        }
    }
}

pub(crate) fn switch_to_ascii_input_source() -> Option<InputSourceRestore> {
    if fcitx_remote(&[]).as_deref() != Some("2") {
        return None;
    }
    fcitx_remote(&["-c"])?;
    Some(InputSourceRestore)
}

/// Avoid starting Fcitx or letting a dead desktop bus block terminal input.
fn fcitx_remote(args: &[&str]) -> Option<String> {
    let mut child = Command::new("fcitx5-remote")
        .arg("--check")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + CONTROLLER_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut output = String::new();
                child
                    .stdout
                    .take()?
                    .take(64)
                    .read_to_string(&mut output)
                    .ok()?;
                return Some(output.trim().to_owned());
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::debug!("Fcitx controller did not respond before the deadline");
                return None;
            }
        }
    }
}
