use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

const LIVE_TEST_ENV: &str = "RADD_REAL_ROOT_TESTS";
const FILE_COUNT: usize = 4;
const ENTRIES_PER_FILE: usize = 25;

#[test]
fn real_root_merge_inspect_and_validate() {
    let Some(tools) = real_root_tools() else {
        return;
    };
    let temp = TempDir::new().expect("temp dir");
    let inputs = create_root_inputs(temp.path(), &tools);
    let manifest = write_manifest(temp.path(), &inputs);
    let output = temp.path().join("merged.root");
    let scratch = temp.path().join("scratch");
    let run_manifest = temp.path().join("radd-manifest.json");
    let command_log = temp.path().join("radd-commands.jsonl");

    Command::cargo_bin("radd")
        .expect("binary exists")
        .args([
            "inspect",
            "--root-metadata",
            "--root",
            tools.root.to_str().expect("root path"),
            manifest_arg(&manifest).as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ROOT metadata:"))
        .stdout(predicate::str::contains("Events (TTree;1)"))
        .stdout(predicate::str::contains("trees: Events"));

    Command::cargo_bin("radd")
        .expect("binary exists")
        .args([
            "merge",
            output.to_str().expect("output path"),
            manifest_arg(&manifest).as_str(),
            "--jobs",
            "2",
            "--chunk-count",
            "2",
            "--scratch",
            scratch.to_str().expect("scratch path"),
            "--manifest",
            run_manifest.to_str().expect("manifest path"),
            "--command-log",
            command_log.to_str().expect("command log path"),
            "--hadd",
            tools.hadd.to_str().expect("hadd path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("radd merge complete"))
        .stdout(predicate::str::contains("hadd commands: 3"));

    Command::cargo_bin("radd")
        .expect("binary exists")
        .args(["validate", output.to_str().expect("output path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("status: ok"));

    assert_root_output(&tools, &output, FILE_COUNT * ENTRIES_PER_FILE);
    assert!(run_manifest.is_file());
    assert!(command_log.is_file());
}

#[test]
fn real_root_benchmark_reports_candidate_results() {
    let Some(tools) = real_root_tools() else {
        return;
    };
    let temp = TempDir::new().expect("temp dir");
    let inputs = create_root_inputs(temp.path(), &tools);
    let manifest = write_manifest(temp.path(), &inputs);
    let scratch = temp.path().join("bench-scratch");

    let output = Command::cargo_bin("radd")
        .expect("binary exists")
        .args([
            "bench",
            manifest_arg(&manifest).as_str(),
            "--jobs-candidates",
            "1,2",
            "--sample-size",
            &FILE_COUNT.to_string(),
            "--scratch",
            scratch.to_str().expect("scratch path"),
            "--hadd",
            tools.hadd.to_str().expect("hadd path"),
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).expect("benchmark json");

    assert_eq!(json["input_file_count"], FILE_COUNT);
    assert_eq!(json["sample_file_count"], FILE_COUNT);
    assert!(matches!(json["recommended_jobs"].as_u64(), Some(1 | 2)));
    assert_eq!(
        json["candidates"]
            .as_array()
            .expect("candidate array")
            .len(),
        2
    );
    for candidate in json["candidates"].as_array().expect("candidate array") {
        assert!(matches!(candidate["jobs"].as_u64(), Some(1 | 2)));
        assert!(candidate["elapsed_seconds"].as_f64().expect("elapsed") >= 0.0);
        assert!(
            candidate["output_size_bytes"]
                .as_u64()
                .expect("output size")
                > 0
        );
    }

    let run_scratch = json["scratch"].as_str().expect("scratch path");
    assert!(
        !Path::new(run_scratch).exists(),
        "bench scratch should be cleaned unless --keep-bench-files is used"
    );
}

#[derive(Debug, Clone)]
struct RootTools {
    root: PathBuf,
    hadd: PathBuf,
}

fn real_root_tools() -> Option<RootTools> {
    if env::var_os(LIVE_TEST_ENV).is_none() {
        eprintln!("skipping real ROOT integration test; set {LIVE_TEST_ENV}=1 to run it");
        return None;
    }

    Some(RootTools {
        root: which("root").expect("RADD_REAL_ROOT_TESTS=1 requires root on PATH"),
        hadd: which("hadd").expect("RADD_REAL_ROOT_TESTS=1 requires hadd on PATH"),
    })
}

fn create_root_inputs(temp: &Path, tools: &RootTools) -> Vec<PathBuf> {
    let inputs = (0..FILE_COUNT)
        .map(|index| temp.join(format!("input-{index}.root")))
        .collect::<Vec<_>>();
    let macro_path = temp.join("make_radd_real_root_fixtures.C");
    fs::write(&macro_path, fixture_macro_source(&inputs)).expect("write ROOT fixture macro");

    let output = ProcessCommand::new(&tools.root)
        .arg("-l")
        .arg("-b")
        .arg("-q")
        .arg(&macro_path)
        .output()
        .expect("run ROOT fixture macro");

    assert!(
        output.status.success(),
        "ROOT fixture macro failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for input in &inputs {
        assert!(
            input.is_file(),
            "fixture was not created: {}",
            input.display()
        );
    }

    inputs
}

fn write_manifest(temp: &Path, inputs: &[PathBuf]) -> PathBuf {
    let manifest = temp.join("inputs.txt");
    let contents = inputs
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&manifest, format!("{contents}\n")).expect("write manifest");
    manifest
}

fn assert_root_output(tools: &RootTools, output: &Path, expected_entries: usize) {
    let macro_path = output
        .parent()
        .expect("output parent")
        .join("check_radd_real_root_output.C");
    fs::write(&macro_path, check_macro_source(output, expected_entries))
        .expect("write ROOT check macro");

    let root_output = ProcessCommand::new(&tools.root)
        .arg("-l")
        .arg("-b")
        .arg("-q")
        .arg(&macro_path)
        .output()
        .expect("run ROOT check macro");

    assert!(
        root_output.status.success(),
        "ROOT output check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&root_output.stdout),
        String::from_utf8_lossy(&root_output.stderr)
    );
}

fn manifest_arg(manifest: &Path) -> String {
    format!("@{}", manifest.display())
}

fn fixture_macro_source(inputs: &[PathBuf]) -> String {
    let bodies = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            format!(
                r#"
  {{
    TFile file({path}, "RECREATE");
    TTree tree("Events", "Synthetic radd integration data");
    TH1D counts("Counts", "Synthetic radd integration counts", 16, 0.0, 16.0);
    int file_index = {index};
    int value = 0;
    double weight = 0.0;
    tree.Branch("file_index", &file_index);
    tree.Branch("value", &value);
    tree.Branch("weight", &weight);
    for (int entry = 0; entry < {entries}; ++entry) {{
      value = file_index * 1000 + entry;
      weight = 0.5 + entry;
      counts.Fill(entry % 16);
      tree.Fill();
    }}
    tree.Write();
    counts.Write();
    file.Close();
  }}"#,
                path = cxx_string_literal(&input.to_string_lossy()),
                index = index,
                entries = ENTRIES_PER_FILE
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#include "TFile.h"
#include "TH1D.h"
#include "TTree.h"

void make_radd_real_root_fixtures() {{
{bodies}
}}
"#
    )
}

fn check_macro_source(output: &Path, expected_entries: usize) -> String {
    format!(
        r#"#include <iostream>
#include "TFile.h"
#include "TH1.h"
#include "TSystem.h"
#include "TTree.h"

void check_radd_real_root_output() {{
  TFile file({path}, "READ");
  if (file.IsZombie()) {{
    std::cerr << "merged file is a zombie" << std::endl;
    gSystem->Exit(2);
  }}

  TTree* tree = nullptr;
  file.GetObject("Events", tree);
  if (!tree) {{
    std::cerr << "missing Events tree" << std::endl;
    gSystem->Exit(3);
  }}
  if (tree->GetEntries() != {expected_entries}) {{
    std::cerr << "wrong Events entries: " << tree->GetEntries() << std::endl;
    gSystem->Exit(4);
  }}

  TH1* counts = nullptr;
  file.GetObject("Counts", counts);
  if (!counts) {{
    std::cerr << "missing Counts histogram" << std::endl;
    gSystem->Exit(5);
  }}
  if (counts->GetEntries() != {expected_entries}) {{
    std::cerr << "wrong Counts entries: " << counts->GetEntries() << std::endl;
    gSystem->Exit(6);
  }}

  std::cout << "radd ROOT output ok" << std::endl;
  gSystem->Exit(0);
}}
"#,
        path = cxx_string_literal(&output.to_string_lossy()),
        expected_entries = expected_entries
    )
}

fn cxx_string_literal(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other => output.push(other),
        }
    }
    output.push('"');
    output
}

fn which(executable: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(executable))
            .find(|candidate| candidate.is_file())
    })
}
