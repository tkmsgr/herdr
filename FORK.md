# Linux Fcitx 5 trial

This fork adds Linux Fcitx 5 support to the existing opt-in prefix input-source
setting. The trial is based on upstream commit
`f5067ed829ae5743499916b941b2310c50681f29`, matching the existing local source's
committed base and its protocol 20. The version label is still 0.8.0, but the
official v0.8.0 tag uses protocol 19 and cannot attach to the running server.
The existing local source has additional uncommitted changes; this
trial leaves that checkout and the running server untouched and installs only
an alternate client.

```toml
[experimental]
switch_ascii_input_source_in_prefix = true
```

The foreground Herdr client needs `fcitx5-remote` on PATH and access to the
desktop session bus. The background server does not control the desktop IME.
An already inactive IME stays inactive. An active IME is temporarily deactivated
for command modes and reactivated on return to text input. Missing, failed, and
unresponsive controllers are handled without an unbounded wait. Switching an
IME with uncommitted text can affect that text; check this separately in the
actual terminal/IME combination before relying on it.

## Build and verify

Use the Rust version in `rust-toolchain.toml`, Zig 0.15.2, `just`, and
`cargo-nextest`. `just check` runs the repository checks and `just build` produces
`target/release/herdr`. Install the repository hooks with `just install-hooks`.

If a user-level tool manager sets `RUSTUP_TOOLCHAIN`, explicitly override it
**after** that manager prepares the command environment. For this baseline:

```sh
mise exec just@1.58.0 zig@0.15.2 aqua:nextest-rs/nextest/cargo-nextest@0.9.144 -- \
  env -u HERDR_SESSION -u HERDR_ENV -u HERDR_SOCKET_PATH \
    -u HERDR_CLIENT_SOCKET_PATH -u HERDR_CONFIG_PATH \
    RUSTUP_TOOLCHAIN=1.96.1 just check
```

The cleared variables prevent integration tests from inheriting the current
live session's name or socket paths.

The fork's pull requests also build a Linux x86-64 binary in GitHub Actions.
Before uploading it, `scripts/fcitx_prefix_smoke.py` starts an isolated server
and PTY client with a fake Fcitx controller, then verifies prefix entry,
movement, rename text entry, cancellation, and initially inactive input.
The artifact includes `BUILD_COMMIT` and `SHA256SUMS`. Use an artifact only after
the checks for that exact PR head pass. Keep the existing installed binary so
the trial can be rolled back by reconnecting with the original client.

## Follow upstream releases

Keep `upstream` pointing to `herdrdev/herdr` and `origin` pointing to this fork.
Keep `master` as the upstream mirror and `fork/installed-base` as the immutable
trial base.
The feature branch contains the tests, CI artifact workflow, and Fcitx patch.

For each new stable release:

1. Fetch the upstream release tag and create a new pristine `fork/vX.Y.Z` base.
2. Create a feature branch from that tag and replay only this fork's commits.
3. Resolve conflicts and check whether upstream already includes the fix.
4. Update build-tool versions to match the new upstream baseline.
5. Open a PR against the new base; run the complete checks and build the binary.
6. Keep using the previous working binary until verification succeeds.

Release detection and patch replay are not scheduled by this trial. They can be
automated later using the same PR/check/artifact path. Do not use `herdr update`
to preserve this patch: it downloads the official build. Once upstream contains
the fix, remove the patch and return to the official distribution.
