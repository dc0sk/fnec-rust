// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Simon Keimer (DC0SK)

//! FND-326, end to end: `fnec --hosts` through `SshWorkerHandle::dispatch`,
//! with a fake `ssh` that prints a login-shell banner before running the
//! worker.
//!
//! The first CI test that reaches the SSH handle's dispatch at all: every
//! other SSH test needs a host (FND-244's row). `ssh_command` runs
//! `Command::new("ssh")`, a `PATH` lookup, so a fake `ssh` first on the CHILD's
//! `PATH` stands in for the connection and nothing in production changes for
//! it. What it cannot stand in for is real ssh's own exit-255 semantics
//! (FND-176), which stay host-bound.
//!
//! The CLI runs as a child process with its own environment — no
//! process-global `env::set_var`, so no race with any other test. **Own test
//! binary**, per the `ETXTBSY` analysis in `worker_task_fault.rs`, and one test
//! in it: the fake `ssh` is written once, before anything is spawned.

use std::io::Write;
use std::process::Command;

mod common;

/// The 21-segment dipole and four-point sweep the finding was reproduced on.
const DECK: &str =
    "CM dipole\nCE\nGW 1 21 0 0 -5.3 0 0 5.3 0.001\nGE 0\nEX 0 1 11 0 1 0\nFR 0 4 0 0 13 0.5\nEN\n";

/// What the fake `ssh` does with `ssh -o … -o … <host> <binary> worker --stdio`:
/// drop the options and the destination, print the banner when `BANNER` is set
/// (a `.bashrc` `echo`), then run the command.
const FAKE_SSH: &str = "#!/bin/sh\n\
    while [ \"$1\" = \"-o\" ]; do shift 2; done\n\
    shift\n\
    if [ -n \"$BANNER\" ]; then echo 'Welcome to the worker host'; fi\n\
    exec \"$@\"\n";

struct Run {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(dir: &std::path::Path, banner: bool) -> Run {
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fnec"));
    cmd.arg("--hosts")
        .arg(dir.join("hosts.toml"))
        .arg(dir.join("dipole.nec"))
        .env("PATH", path);
    if banner {
        cmd.env("BANNER", "1");
    } else {
        cmd.env_remove("BANNER");
    }
    let out = cmd.output().expect("run fnec --hosts");
    Run {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// The `Z` lines of the run, in order.
fn z_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| l.starts_with("Z "))
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
#[test]
fn a_login_banner_over_ssh_does_not_shift_the_sweep() {
    let dir = common::TempDir::new("fake-ssh");
    std::fs::write(dir.join("dipole.nec"), DECK).expect("write deck");
    std::fs::write(
        dir.join("hosts.toml"),
        format!(
            "[[worker]]\nhostname = \"banner\"\nbinary_path = \"{}\"\n",
            env!("CARGO_BIN_EXE_fnec")
        ),
    )
    .expect("write hosts.toml");
    {
        let staged = dir.join("ssh.staging");
        let mut f = std::fs::File::create(&staged).expect("create fake ssh");
        f.write_all(FAKE_SSH.as_bytes()).expect("write fake ssh");
        f.sync_all().expect("sync fake ssh");
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake ssh");
        drop(f);
        std::fs::rename(&staged, dir.join("ssh")).expect("stage fake ssh");
    }

    let clean = run(&dir, false);
    assert_eq!(clean.status, Some(0), "clean run:\n{}", clean.stderr);
    let want = z_lines(&clean.stdout);
    assert_eq!(
        want.len(),
        4,
        "clean run prints four points:\n{}",
        clean.stdout
    );

    let banner = run(&dir, true);
    assert_eq!(
        banner.status,
        Some(0),
        "a banner must not fail the run — the numbers are right:\n{}",
        banner.stderr
    );
    assert!(
        banner.stdout.contains("N_POINTS 4"),
        "all four points:\n{}",
        banner.stdout
    );
    assert_eq!(
        z_lines(&banner.stdout),
        want,
        "every point must carry its own impedance, not its predecessor's"
    );
    assert!(
        banner
            .stderr
            .contains("warning: worker 'ssh:banner' wrote 1 line")
            && banner.stderr.contains("Welcome to the worker host"),
        "the operator is told which host writes to stdout:\n{}",
        banner.stderr
    );
    assert!(
        !clean.stderr.contains("wrote 1 line"),
        "no stray warning without a banner:\n{}",
        clean.stderr
    );
}
