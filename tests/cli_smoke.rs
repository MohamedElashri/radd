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

#[test]
fn short_version_works() {
    let mut command = Command::cargo_bin("radd").expect("binary exists");

    command
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn conventional_short_version_works() {
    let mut command = Command::cargo_bin("radd").expect("binary exists");

    command
        .arg("-V")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_subcommand_works() {
    let mut command = Command::cargo_bin("radd").expect("binary exists");

    command
        .arg("version")
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

#[cfg(unix)]
#[test]
fn inspect_root_metadata_uses_optional_root_command() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_root(temp.path());
    fs::write(temp.path().join("a.root"), b"fake root").expect("write input");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args(["inspect", "a.root", "--root-metadata"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ROOT metadata:"))
        .stdout(predicate::str::contains(
            "compression: algorithm 1 level 4 settings 104",
        ))
        .stdout(predicate::str::contains("Events (TTree;1)"))
        .stdout(predicate::str::contains("trees: Events"));
}

#[cfg(unix)]
#[test]
fn inspect_root_metadata_warns_when_root_is_unavailable() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"fake root").expect("write input");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args([
            "inspect",
            "a.root",
            "--root-metadata",
            "--root",
            "/definitely/missing/root",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ROOT metadata:"))
        .stdout(predicate::str::contains("none available"))
        .stdout(predicate::str::contains("Warnings:"))
        .stdout(predicate::str::contains("ROOT metadata unavailable"));
}

#[test]
fn plan_prints_human_readable_topology() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");
    fs::write(temp.path().join("b.root"), b"de").expect("write b");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args([
            "plan",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--fan-in",
            "2",
            "--scratch",
            "scratch",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd plan"))
        .stdout(predicate::str::contains("output: out.root"))
        .stdout(predicate::str::contains("policy: fastest"))
        .stdout(predicate::str::contains(
            "chunks: 2 non-empty of 2 requested",
        ))
        .stdout(predicate::str::contains("stage 0: 2 jobs"))
        .stdout(predicate::str::contains("stage 1: 1 job"));
}

#[test]
fn plan_prints_json_topology() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");
    fs::write(temp.path().join("b.root"), b"de").expect("write b");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    let output = command
        .args([
            "plan",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).expect("valid json");

    assert_eq!(json["output"], "out.root");
    assert_eq!(json["policy"], "fastest");
    assert_eq!(json["chunk_count"], 2);
    assert_eq!(json["stages"].as_array().expect("stages").len(), 2);
}

#[test]
fn plan_prints_hadd_commands_when_requested() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a root.root"), b"abc").expect("write a");
    fs::write(temp.path().join("b.root"), b"de").expect("write b");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args([
            "plan",
            "out root.root",
            "a root.root",
            "b.root",
            "--jobs",
            "1",
            "--commands",
            "--hadd",
            "custom-hadd",
            "--keep-going",
            "--max-open-files",
            "64",
            "--no-trees",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "command: custom-hadd -f -fk -k -n 64 -T 'out root.root'",
        ))
        .stdout(predicate::str::contains("a root.root"));
}

#[cfg(unix)]
#[test]
fn plan_prints_object_selection_commands_when_supported() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "plan",
            "out.root",
            "a.root",
            "--jobs",
            "1",
            "--scratch",
            "scratch",
            "--commands",
            "--only",
            "DecayTree",
            "--only",
            "Events",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "-L scratch/radd-object-selection.txt -Ltype OnlyListed",
        ));
}

#[cfg(unix)]
#[test]
fn merge_succeeds_with_fake_hadd_and_cleans_temporary_outputs() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    fs::write(temp.path().join("c.root"), b"c").expect("write c");
    let log = temp.path().join("hadd.log");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_LOG", &log);

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "c.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--fan-in",
            "2",
            "--scratch",
            "scratch",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd merge complete"))
        .stdout(predicate::str::contains("hadd commands: 3"));

    assert!(temp.path().join("out.root").is_file());
    assert!(!temp.path().join("scratch/radd-stage-0-job-0.root").exists());
    assert!(!temp.path().join("scratch/radd-stage-0-job-1.root").exists());

    let log = fs::read_to_string(log).expect("fake hadd log");
    assert_eq!(log.lines().count(), 3);
}

#[cfg(unix)]
#[test]
fn merge_refuses_existing_output_without_force() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("out.root"), b"existing").expect("write out");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args(["merge", "out.root", "a.root", "--jobs", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --force to overwrite it"));

    assert_eq!(
        fs::read(temp.path().join("out.root")).expect("read out"),
        b"existing"
    );
}

#[cfg(unix)]
#[test]
fn merge_force_allows_existing_output_overwrite() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("out.root"), b"existing").expect("write out");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args(["merge", "out.root", "a.root", "--jobs", "1", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd merge complete"));

    assert_eq!(
        fs::read_to_string(temp.path().join("out.root")).expect("read out"),
        "fake root output\n"
    );
}

#[cfg(unix)]
#[test]
fn merge_refuses_output_that_is_also_an_input_even_with_force() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args(["merge", "a.root", "a.root", "--jobs", "1", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "output path is also an input file",
        ));

    assert_eq!(
        fs::read(temp.path().join("a.root")).expect("read a"),
        b"aaa"
    );
}

#[cfg(unix)]
#[test]
fn merge_object_selection_uses_list_file_and_cleans_it() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    let command_log = temp.path().join("commands.jsonl");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "--jobs",
            "1",
            "--scratch",
            "scratch",
            "--only",
            "DecayTree",
            "--command-log",
            command_log.to_str().expect("utf-8 log"),
        ])
        .assert()
        .success();

    let commands = fs::read_to_string(command_log).expect("read command log");
    assert!(commands.contains("scratch/radd-object-selection.txt"));
    assert!(commands.contains("OnlyListed"));
    assert!(
        !temp
            .path()
            .join("scratch/radd-object-selection.txt")
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn merge_object_selection_errors_when_hadd_does_not_support_lists() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd_without_object_lists(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args(["merge", "out.root", "a.root", "--only", "DecayTree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not appear to support hadd object-list options",
        ));
}

#[cfg(unix)]
#[test]
fn merge_dry_run_does_not_create_outputs_or_scratch() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let log = temp.path().join("hadd.log");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_LOG", &log);

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run"))
        .stdout(predicate::str::contains("command: hadd -f -fk"));

    assert!(!temp.path().join("out.root").exists());
    assert!(!temp.path().join("scratch").exists());
    assert!(!log.exists());
}

#[cfg(unix)]
#[test]
fn merge_writes_command_log_json_lines() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let command_log = temp.path().join("radd-commands.jsonl");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--command-log",
            command_log.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(command_log).expect("read command log");
    assert_eq!(contents.lines().count(), 3);

    let first: serde_json::Value =
        serde_json::from_str(contents.lines().next().expect("first line")).expect("json line");
    assert_eq!(first["stage"], 0);
    assert_eq!(first["argv"][0], "hadd");
}

#[cfg(unix)]
#[test]
fn merge_writes_reproducibility_manifest() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let manifest = temp.path().join("radd-manifest.json");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--manifest",
            manifest.to_str().expect("utf-8 path"),
        ])
        .assert()
        .success();

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest).expect("read manifest")).expect("manifest json");

    assert_eq!(manifest["radd_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["options"]["hadd_version"], "fake hadd 1.2.3");
    assert_eq!(manifest["inputs"].as_array().expect("inputs").len(), 2);
    assert_eq!(manifest["options"]["output"], "out.root");
    assert_eq!(manifest["plan"]["chunk_count"], 2);
    assert_eq!(manifest["commands"].as_array().expect("commands").len(), 3);
}

#[cfg(unix)]
#[test]
fn merge_json_prints_telemetry() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    let stdout = command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let telemetry: serde_json::Value = serde_json::from_slice(&stdout).expect("telemetry json");
    assert_eq!(telemetry["radd_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(telemetry["hadd_version"], "fake hadd 1.2.3");
    assert_eq!(telemetry["input_file_count"], 2);
    assert_eq!(telemetry["stage_count"], 2);
    assert_eq!(telemetry["hadd_command_count"], 3);
    assert_eq!(telemetry["output_size_bytes"], 17);
    assert_eq!(telemetry["cache_hits"], 0);
}

#[cfg(unix)]
#[test]
fn merge_stage_inputs_uses_scratch_paths_and_cleans_staged_inputs() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let command_log = temp.path().join("commands.jsonl");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "1",
            "--scratch",
            "scratch",
            "--stage-inputs",
            "--command-log",
            command_log.to_str().expect("utf-8 log"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("input staging: 2 files"))
        .stdout(predicate::str::contains("staged inputs kept: no"));

    assert!(temp.path().join("out.root").is_file());
    assert!(!temp.path().join("scratch/radd-staged-inputs").exists());

    let commands = fs::read_to_string(command_log).expect("read command log");
    assert!(commands.contains("scratch/radd-staged-inputs/000000-a.root"));
    assert!(commands.contains("scratch/radd-staged-inputs/000001-b.root"));
}

#[cfg(unix)]
#[test]
fn merge_stage_inputs_can_keep_staged_inputs() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "--jobs",
            "1",
            "--scratch",
            "scratch",
            "--stage-inputs",
            "--keep-staged-inputs",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("staged inputs kept: yes"));

    let staged = temp.path().join("scratch/radd-staged-inputs/000000-a.root");
    assert!(staged.is_file());
    assert_eq!(fs::read(staged).expect("read staged"), b"aaa");
}

#[cfg(unix)]
#[test]
fn merge_stage_inputs_fails_before_hadd_when_scratch_is_not_directory() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("scratch"), b"not a directory").expect("write scratch file");
    let log = temp.path().join("hadd.log");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_LOG", &log);

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "--jobs",
            "1",
            "--scratch",
            "scratch",
            "--stage-inputs",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "could not create input staging directory",
        ));

    assert!(!log.exists());
    assert!(!temp.path().join("out.root").exists());
}

#[cfg(unix)]
#[test]
fn merge_cache_miss_populates_cache() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let cache_dir = temp.path().join("cache");
    let log = temp.path().join("hadd.log");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_CACHE_DIR", &cache_dir)
        .env("RADD_FAKE_HADD_LOG", &log);

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--cache",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("cache hits: 0"))
        .stdout(predicate::str::contains("cache misses: 2"));

    assert_eq!(
        fs::read_to_string(log)
            .expect("fake hadd log")
            .lines()
            .count(),
        3
    );
    assert_eq!(cache_file_count(&cache_dir.join("chunks")), 2);
    assert_eq!(cache_file_count(&cache_dir.join("manifests")), 2);
}

#[cfg(unix)]
#[test]
fn merge_cache_hit_reuses_cached_first_stage_outputs() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let cache_dir = temp.path().join("cache");
    let log = temp.path().join("hadd.log");

    for _ in 0..2 {
        let mut command = Command::cargo_bin("radd").expect("binary exists");
        command
            .current_dir(temp.path())
            .env("PATH", prepend_to_path(temp.path()))
            .env("RADD_CACHE_DIR", &cache_dir)
            .env("RADD_FAKE_HADD_LOG", &log);

        command
            .args([
                "merge",
                "out.root",
                "a.root",
                "b.root",
                "--jobs",
                "2",
                "--chunk-count",
                "2",
                "--scratch",
                "scratch",
                "--cache",
                "--force",
            ])
            .assert()
            .success();
    }

    let log = fs::read_to_string(log).expect("fake hadd log");
    assert_eq!(log.lines().count(), 4);
}

#[cfg(unix)]
#[test]
fn merge_cache_misses_when_hadd_version_changes() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let cache_dir = temp.path().join("cache");
    let log = temp.path().join("hadd.log");

    for version in ["fake hadd 1.2.3", "fake hadd 2.0.0"] {
        let mut command = Command::cargo_bin("radd").expect("binary exists");
        command
            .current_dir(temp.path())
            .env("PATH", prepend_to_path(temp.path()))
            .env("RADD_CACHE_DIR", &cache_dir)
            .env("RADD_FAKE_HADD_LOG", &log)
            .env("RADD_FAKE_HADD_VERSION", version);

        command
            .args([
                "merge",
                "out.root",
                "a.root",
                "b.root",
                "--jobs",
                "2",
                "--chunk-count",
                "2",
                "--scratch",
                "scratch",
                "--cache",
                "--force",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("cache hits: 0"))
            .stdout(predicate::str::contains("cache misses: 2"));
    }

    let log = fs::read_to_string(log).expect("fake hadd log");
    assert_eq!(log.lines().count(), 6);
}

#[cfg(unix)]
#[test]
fn corrupt_cache_entry_is_rebuilt() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let cache_dir = temp.path().join("cache");
    let log = temp.path().join("hadd.log");

    let mut first = Command::cargo_bin("radd").expect("binary exists");
    first
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_CACHE_DIR", &cache_dir);
    first
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--cache",
        ])
        .assert()
        .success();

    for chunk in fs::read_dir(cache_dir.join("chunks")).expect("chunks") {
        fs::write(chunk.expect("chunk").path(), b"").expect("corrupt chunk");
    }

    let mut second = Command::cargo_bin("radd").expect("binary exists");
    second
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_CACHE_DIR", &cache_dir)
        .env("RADD_FAKE_HADD_LOG", &log);

    second
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--cache",
            "--force",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("cache hits: 0"))
        .stdout(predicate::str::contains("cache misses: 2"));

    assert_eq!(
        fs::read_to_string(log)
            .expect("fake hadd log")
            .lines()
            .count(),
        3
    );
}

#[cfg(unix)]
#[test]
fn cache_list_and_clean_use_cache_root() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    let cache_dir = temp.path().join("cache");

    let mut merge = Command::cargo_bin("radd").expect("binary exists");
    merge
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_CACHE_DIR", &cache_dir);
    merge
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            "scratch",
            "--cache",
        ])
        .assert()
        .success();

    let mut list = Command::cargo_bin("radd").expect("binary exists");
    list.env("RADD_CACHE_DIR", &cache_dir)
        .args(["cache", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("entries: 2"))
        .stdout(predicate::str::contains("complete"));

    let mut clean = Command::cargo_bin("radd").expect("binary exists");
    clean
        .env("RADD_CACHE_DIR", &cache_dir)
        .args(["cache", "clean"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed files: 4"));
}

#[test]
fn validate_accepts_nonempty_output() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("out.root"), b"fake root output\n").expect("write output");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["validate", "out.root"])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd validate"))
        .stdout(predicate::str::contains("level: basic"))
        .stdout(predicate::str::contains("status: ok"));
}

#[test]
fn validate_rejects_missing_output() {
    let temp = TempDir::new().expect("temp dir");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["validate", "missing.root"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("output does not exist"));
}

#[test]
fn validate_rejects_zero_size_output() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("empty.root"), b"").expect("write output");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["validate", "empty.root"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("output is empty"));
}

#[cfg(unix)]
#[test]
fn bench_runs_candidates_and_cleans_benchmark_files() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bbb").expect("write b");
    fs::write(temp.path().join("c.root"), b"cc").expect("write c");
    fs::write(temp.path().join("d.root"), b"d").expect("write d");
    let scratch = temp.path().join("bench-scratch");
    let log = temp.path().join("hadd.log");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_LOG", &log)
        .env("RADD_FAKE_HADD_SLEEP_SECONDS", "0.01");

    command
        .args([
            "bench",
            "a.root",
            "b.root",
            "c.root",
            "d.root",
            "--jobs-candidates",
            "1,2",
            "--sample-size",
            "3",
            "--scratch",
            scratch.to_str().expect("utf-8 scratch"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd bench"))
        .stdout(predicate::str::contains("results are approximate"))
        .stdout(predicate::str::contains("sample: 3 of 4 files"))
        .stdout(predicate::str::contains("candidate jobs: 1"))
        .stdout(predicate::str::contains("candidate jobs: 2"))
        .stdout(predicate::str::contains("recommended jobs:"))
        .stdout(predicate::str::contains("benchmark files kept: no"));

    assert_eq!(
        fs::read_to_string(log)
            .expect("fake hadd log")
            .lines()
            .count(),
        4
    );
    assert_eq!(directory_entry_count(&scratch), 0);
}

#[cfg(unix)]
#[test]
fn bench_supports_object_selection_and_json_output() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaaa").expect("write a");
    let scratch = temp.path().join("bench-scratch");
    let log = temp.path().join("hadd.log");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_LOG", &log);

    let stdout = command
        .args([
            "bench",
            "a.root",
            "--jobs-candidates",
            "1",
            "--scratch",
            scratch.to_str().expect("utf-8 scratch"),
            "--only",
            "Events",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("benchmark json");
    assert_eq!(report["input_file_count"], 1);
    assert_eq!(report["recommended_merge_flags"][1], "1");
    assert_eq!(
        report["candidates"].as_array().expect("candidates").len(),
        1
    );

    let log = fs::read_to_string(log).expect("fake hadd log");
    assert!(log.contains("-Ltype OnlyListed"));
    assert!(log.contains("radd-object-selection.txt"));
    assert_eq!(directory_entry_count(&scratch), 0);
}

#[cfg(unix)]
#[test]
fn bench_json_suppresses_successful_hadd_stdout() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaaa").expect("write a");
    let scratch = temp.path().join("bench-scratch");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_STDOUT", "fake hadd noisy stdout");

    let stdout = command
        .args([
            "bench",
            "a.root",
            "--jobs-candidates",
            "1",
            "--scratch",
            scratch.to_str().expect("utf-8 scratch"),
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("fake hadd noisy stdout").not())
        .get_output()
        .stdout
        .clone();

    let report: serde_json::Value = serde_json::from_slice(&stdout).expect("benchmark json");
    assert_eq!(report["input_file_count"], 1);
}

#[cfg(unix)]
#[test]
fn bench_can_keep_benchmark_files() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaaa").expect("write a");
    let scratch = temp.path().join("bench-scratch");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "bench",
            "a.root",
            "--jobs-candidates",
            "1",
            "--scratch",
            scratch.to_str().expect("utf-8 scratch"),
            "--keep-bench-files",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("benchmark files kept: yes"));

    assert_eq!(directory_entry_count(&scratch), 1);
}

#[test]
fn bench_rejects_invalid_jobs_candidates() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["bench", "a.root", "--jobs-candidates", "1,0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--jobs-candidates values must be greater than zero",
        ));
}

#[cfg(unix)]
#[test]
fn merge_runs_basic_validation_by_default() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()));

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "--jobs",
            "1",
            "--scratch",
            "scratch",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("validation: basic ok"));
}

#[cfg(unix)]
#[test]
fn merge_fails_when_default_validation_rejects_empty_output() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_EMPTY_OUTPUT", "1");

    command
        .args(["merge", "out.root", "a.root", "--jobs", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("output is empty"));
}

#[cfg(unix)]
#[test]
fn merge_no_validate_skips_basic_validation() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_EMPTY_OUTPUT", "1");

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "--jobs",
            "1",
            "--no-validate",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("validation: skipped"));
}

#[cfg(unix)]
#[test]
fn merge_failure_preserves_successful_temporary_outputs() {
    let temp = TempDir::new().expect("temp dir");
    write_fake_hadd(temp.path());
    fs::write(temp.path().join("a.root"), b"aaa").expect("write a");
    fs::write(temp.path().join("b.root"), b"bb").expect("write b");
    fs::write(temp.path().join("c.root"), b"c").expect("write c");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command
        .current_dir(temp.path())
        .env("PATH", prepend_to_path(temp.path()))
        .env("RADD_FAKE_HADD_FAIL_CONTAINS", "radd-stage-0-job-1.root");

    command
        .args([
            "merge",
            "out.root",
            "a.root",
            "b.root",
            "c.root",
            "--jobs",
            "1",
            "--chunk-count",
            "2",
            "--fan-in",
            "2",
            "--scratch",
            "scratch",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hadd job failed at stage 0 job 1"));

    assert!(!temp.path().join("out.root").exists());
    assert!(
        temp.path()
            .join("scratch/radd-stage-0-job-0.root")
            .is_file()
    );
}

#[test]
fn plan_rejects_invalid_fan_in() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["plan", "out.root", "a.root", "--fan-in", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--fan-in must be at least 2"));
}

#[test]
fn plan_rejects_invalid_hadd_jobs() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("a.root"), b"abc").expect("write a");

    let mut command = Command::cargo_bin("radd").expect("binary exists");
    command.current_dir(temp.path());

    command
        .args(["plan", "out.root", "a.root", "--hadd-jobs", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--hadd-jobs must be greater than zero",
        ));
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

#[cfg(unix)]
fn write_fake_hadd(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join("hadd");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "$1" = "-h" ]; then
  echo 'fake hadd help -L -Ltype OnlyListed SkipListed -T'
  exit 0
fi
if [ "$1" = "--version" ]; then
  echo "${RADD_FAKE_HADD_VERSION:-fake hadd 1.2.3}"
  exit 0
fi

if [ -n "$RADD_FAKE_HADD_LOG" ]; then
  printf '%s\n' "$*" >> "$RADD_FAKE_HADD_LOG"
fi

output=
skip_next=0
for arg in "$@"; do
  if [ "$skip_next" = "1" ]; then
    skip_next=0
    continue
  fi

  case "$arg" in
    -j|-d|-n|-L|-Ltype)
      skip_next=1
      ;;
    -*)
      ;;
    *)
      output="$arg"
      break
      ;;
  esac
done

if [ -n "$RADD_FAKE_HADD_FAIL_CONTAINS" ]; then
  case "$output" in
    *"$RADD_FAKE_HADD_FAIL_CONTAINS"*)
      exit 12
      ;;
  esac
fi

if [ -n "$RADD_FAKE_HADD_SLEEP_SECONDS" ]; then
  sleep "$RADD_FAKE_HADD_SLEEP_SECONDS"
fi

if [ -n "$RADD_FAKE_HADD_STDOUT" ]; then
  printf '%s\n' "$RADD_FAKE_HADD_STDOUT"
fi

if [ -n "$output" ]; then
  mkdir -p "$(dirname "$output")"
  if [ -n "$RADD_FAKE_HADD_EMPTY_OUTPUT" ]; then
    : > "$output"
  else
    printf 'fake root output\n' > "$output"
  fi
fi
"#,
    )
    .expect("write fake hadd");

    let mut permissions = fs::metadata(&path).expect("fake metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake hadd executable");
}

#[cfg(unix)]
fn write_fake_hadd_without_object_lists(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join("hadd");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "$1" = "-h" ]; then
  echo 'fake hadd help -T'
  exit 0
fi

exit 2
"#,
    )
    .expect("write fake hadd");

    let mut permissions = fs::metadata(&path).expect("fake metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake hadd executable");
}

#[cfg(unix)]
fn write_fake_root(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join("root");
    fs::write(
        &path,
        r#"#!/bin/sh
echo 'fake ROOT banner'
echo '__RADD_ROOT_METADATA_BEGIN__'
echo '{"path":"fake.root","compression_algorithm":1,"compression_level":4,"compression_settings":104,"file_uuid":"fake-uuid","top_level_keys":[{"name":"Events","class_name":"TTree","cycle":1},{"name":"Meta","class_name":"TDirectoryFile","cycle":1}],"tree_names":["Events"],"warnings":[]}'
echo '__RADD_ROOT_METADATA_END__'
"#,
    )
    .expect("write fake root");

    let mut permissions = fs::metadata(&path).expect("fake metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake root executable");
}

fn prepend_to_path(directory: &Path) -> std::ffi::OsString {
    let old_path = env::var_os("PATH").unwrap_or_default();
    let paths = std::iter::once(directory.to_path_buf()).chain(env::split_paths(&old_path));

    env::join_paths(paths).expect("join PATH")
}

#[cfg(unix)]
fn cache_file_count(directory: &Path) -> usize {
    fs::read_dir(directory)
        .expect("cache directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .count()
}

#[cfg(unix)]
fn directory_entry_count(directory: &Path) -> usize {
    fs::read_dir(directory).map_or(0, |entries| entries.filter_map(Result::ok).count())
}
