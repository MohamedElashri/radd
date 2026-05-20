use std::{env, fs, path::Path};

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

#[test]
fn help_works() {
    let mut command = Command::cargo_bin("radd").expect("binary exists");

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("doctor"));
}

#[test]
fn version_works() {
    let mut command = Command::cargo_bin("radd").expect("binary exists");

    command
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[cfg(unix)]
#[test]
fn doctor_succeeds_with_fake_hadd() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.env("PATH", prepend_to_path(temp.path()));

    command
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("hadd: found at"))
        .stdout(predicate::str::contains("hadd executable: yes"))
        .stdout(predicate::str::contains("hadd help: yes"))
        .stdout(predicate::str::contains("status: ok"));
}

#[test]
fn doctor_fails_when_hadd_is_missing() {
    let temp = TempDir::new().expect("temp dir");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.env("PATH", temp.path());

    command
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("hadd: not found"))
        .stdout(predicate::str::contains("status: failed"))
        .stderr(predicate::str::contains("doctor checks failed"));
}

#[test]
fn inspect_resolves_manifest_inputs() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abcd").expect("write a");
    fs::write(temp.path().join("b.root"), b"xy").expect("write b");
    fs::write(
        temp.path().join("inputs.txt"),
        "# sample manifest\n\na.root\nb.root\n",
    )
    .expect("write manifest");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["inspect", "@inputs.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inputs: 2 files"))
        .stdout(predicate::str::contains("total size: 6 bytes"));
}

#[test]
fn plan_resolves_inputs_before_full_planning_exists() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["plan", "out.root", "a.root"])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd plan"))
        .stdout(predicate::str::contains("output: out.root"))
        .stdout(predicate::str::contains("inputs: 1 file"))
        .stdout(predicate::str::contains("planning: merge topology"));
}

#[test]
fn inspect_reports_duplicate_inputs() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["inspect", "a.root", "./a.root"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate input file"));
}

#[test]
fn future_commands_report_not_implemented() {
    let mut command = Command::cargo_bin("radd").expect("binary exists");

    command
        .args(["bench", "input.root"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not implemented yet"));
}

#[cfg(unix)]
fn write_fake_hadd(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join("hadd");
    fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"-h\" ]; then echo 'fake hadd help'; exit 0; fi\nexit 0\n",
    )
    .expect("write fake hadd");

    let mut permissions = fs::metadata(&path).expect("fake metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake hadd executable");
}

fn prepend_to_path(directory: &Path) -> std::ffi::OsString {
    let old_path = env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(directory.to_path_buf()).chain(env::split_paths(&old_path));

    env::join_paths(paths).expect("join PATH")
}
