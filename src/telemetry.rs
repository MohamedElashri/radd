//! Merge telemetry and reproducibility artifacts.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    executor::{ExecutableJob, ExecutionReport},
    hadd::HaddOptions,
    input::{InputFile, InputSet},
    planner::{MergePlan, MergePolicy},
    staging::{InputStagingPlan, InputStagingReport},
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MergeTelemetry {
    pub radd_version: &'static str,
    pub hadd_path: PathBuf,
    pub hadd_version: Option<String>,
    pub start_time: Option<UnixTime>,
    pub end_time: Option<UnixTime>,
    pub elapsed_seconds: f64,
    pub input_file_count: usize,
    pub total_input_bytes: u64,
    pub output_file: PathBuf,
    pub output_size_bytes: Option<u64>,
    pub scratch_directory: PathBuf,
    pub policy: MergePolicy,
    pub jobs: usize,
    pub fan_in: usize,
    pub stage_count: usize,
    pub hadd_command_count: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub input_staging: Option<StagingTelemetry>,
    pub failed_jobs: Vec<FailedJob>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReproducibilityManifest {
    pub radd_version: &'static str,
    pub inputs: Vec<ManifestInput>,
    pub options: ManifestOptions,
    pub plan: MergePlan,
    pub commands: Vec<CommandLogRecord>,
    pub input_staging: Option<StagingTelemetry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestInput {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_time: Option<UnixTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ManifestOptions {
    pub output: PathBuf,
    pub policy: MergePolicy,
    pub jobs: usize,
    pub fan_in: usize,
    pub scratch: PathBuf,
    pub hadd_path: PathBuf,
    pub hadd_version: Option<String>,
    pub hadd_jobs: Option<usize>,
    pub keep_going: bool,
    pub max_open_files: Option<usize>,
    pub no_trees: bool,
    pub object_selection: Option<ManifestObjectSelection>,
    pub stage_inputs: bool,
    pub keep_staged_inputs: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestObjectSelection {
    pub mode: crate::hadd::ObjectSelectionMode,
    pub objects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandLogRecord {
    pub stage: usize,
    pub job: usize,
    pub output: PathBuf,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnixTime {
    pub seconds: u64,
    pub nanos: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailedJob {
    pub stage: usize,
    pub job: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StagingTelemetry {
    pub enabled: bool,
    pub staging_directory: PathBuf,
    pub input_count: usize,
    pub total_bytes: u64,
    pub hardlinks: usize,
    pub copies: usize,
    pub kept_after_success: bool,
}

impl StagingTelemetry {
    #[must_use]
    pub fn planned(plan: &InputStagingPlan) -> Self {
        Self {
            enabled: true,
            staging_directory: plan.staging_dir.clone(),
            input_count: plan.inputs.len(),
            total_bytes: plan.total_bytes,
            hardlinks: 0,
            copies: 0,
            kept_after_success: plan.keep_after_success,
        }
    }

    #[must_use]
    pub fn executed(report: &InputStagingReport) -> Self {
        Self {
            enabled: true,
            staging_directory: report.plan.staging_dir.clone(),
            input_count: report.plan.inputs.len(),
            total_bytes: report.plan.total_bytes,
            hardlinks: report.hardlinks,
            copies: report.copies,
            kept_after_success: report.plan.keep_after_success,
        }
    }
}

pub fn build_telemetry(
    input_set: &InputSet,
    plan: &MergePlan,
    hadd_options: &HaddOptions,
    report: &ExecutionReport,
    timing: RunTiming,
    output_size_bytes: Option<u64>,
    input_staging: Option<StagingTelemetry>,
) -> MergeTelemetry {
    MergeTelemetry {
        radd_version: env!("CARGO_PKG_VERSION"),
        hadd_path: hadd_options.executable.clone(),
        hadd_version: hadd_options.version.clone(),
        start_time: unix_time(timing.started_at),
        end_time: unix_time(timing.ended_at),
        elapsed_seconds: duration_seconds(report.elapsed),
        input_file_count: input_set.files.len(),
        total_input_bytes: input_set.total_size_bytes,
        output_file: plan.output.clone(),
        output_size_bytes,
        scratch_directory: plan.scratch.clone(),
        policy: plan.policy,
        jobs: plan.jobs,
        fan_in: plan.fan_in,
        stage_count: report.stage_count,
        hadd_command_count: report.command_count,
        cache_hits: report.cache_hits,
        cache_misses: report.cache_misses,
        input_staging,
        failed_jobs: Vec::new(),
        dry_run: report.dry_run,
    }
}

pub fn build_manifest(
    input_set: &InputSet,
    plan: &MergePlan,
    hadd_options: &HaddOptions,
    command_records: Vec<CommandLogRecord>,
    dry_run: bool,
    input_staging: Option<StagingTelemetry>,
) -> ReproducibilityManifest {
    let stage_inputs = input_staging.is_some();
    let keep_staged_inputs = input_staging
        .as_ref()
        .is_some_and(|staging| staging.kept_after_success);

    ReproducibilityManifest {
        radd_version: env!("CARGO_PKG_VERSION"),
        inputs: input_set.files.iter().map(manifest_input).collect(),
        options: ManifestOptions {
            output: plan.output.clone(),
            policy: plan.policy,
            jobs: plan.jobs,
            fan_in: plan.fan_in,
            scratch: plan.scratch.clone(),
            hadd_path: hadd_options.executable.clone(),
            hadd_version: hadd_options.version.clone(),
            hadd_jobs: hadd_options.hadd_jobs,
            keep_going: hadd_options.keep_going,
            max_open_files: hadd_options.max_open_files,
            no_trees: hadd_options.no_trees,
            object_selection: hadd_options.object_selection.as_ref().map(|selection| {
                ManifestObjectSelection {
                    mode: selection.mode,
                    objects: selection.objects.clone(),
                }
            }),
            stage_inputs,
            keep_staged_inputs,
            dry_run,
        },
        plan: plan.clone(),
        commands: command_records,
        input_staging,
    }
}

pub fn command_log_records(stages: &[Vec<ExecutableJob>]) -> Vec<CommandLogRecord> {
    stages
        .iter()
        .flat_map(|stage| {
            stage.iter().map(|job| CommandLogRecord {
                stage: job.stage_level,
                job: job.job_id,
                output: job.output.clone(),
                argv: job.command.argv.clone(),
            })
        })
        .collect()
}

pub fn write_command_log(path: &Path, records: &[CommandLogRecord]) -> Result<()> {
    let mut contents = String::new();

    for record in records {
        contents.push_str(&serde_json::to_string(record)?);
        contents.push('\n');
    }

    write_text(path, &contents)
}

pub fn write_manifest(path: &Path, manifest: &ReproducibilityManifest) -> Result<()> {
    write_text(path, &serde_json::to_string_pretty(manifest)?)
}

pub fn output_size(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map(|metadata| metadata.len())
}

#[derive(Debug, Clone, Copy)]
pub struct RunTiming {
    pub started_at: SystemTime,
    pub ended_at: SystemTime,
}

fn manifest_input(input: &InputFile) -> ManifestInput {
    ManifestInput {
        path: input.path.clone(),
        size_bytes: input.size_bytes,
        modified_time: input.modified_time.and_then(unix_time),
    }
}

fn unix_time(time: SystemTime) -> Option<UnixTime> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;

    Some(UnixTime {
        seconds: duration.as_secs(),
        nanos: duration.subsec_nanos(),
    })
}

fn duration_seconds(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

fn write_text(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!("could not create artifact directory: {}", parent.display())
        })?;
    }

    fs::write(path, contents)
        .with_context(|| format!("could not write artifact file: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use super::{
        RunTiming, build_manifest, build_telemetry, command_log_records, write_command_log,
    };
    use crate::{
        executor::{ExecutableJob, ExecutionReport},
        hadd::{HaddCommand, HaddOptions},
        input::{InputFile, InputSet},
        planner::{MergePlan, MergePolicy, MergeStage},
    };

    #[test]
    fn telemetry_serializes_expected_summary_fields() {
        let input_set = input_set();
        let plan = plan();
        let report = ExecutionReport {
            stage_count: 1,
            command_count: 1,
            elapsed: std::time::Duration::from_millis(1500),
            dry_run: false,
            cache_hits: 2,
            cache_misses: 3,
        };
        let telemetry = build_telemetry(
            &input_set,
            &plan,
            &hadd_options(),
            &report,
            RunTiming {
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: SystemTime::UNIX_EPOCH,
            },
            Some(42),
            None,
        );
        let json = serde_json::to_value(telemetry).expect("json");

        assert_eq!(json["radd_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["hadd_version"], "hadd fake 1.2.3");
        assert_eq!(json["input_file_count"], 1);
        assert_eq!(json["output_size_bytes"], 42);
        assert_eq!(json["cache_hits"], 2);
        assert_eq!(json["cache_misses"], 3);
    }

    #[test]
    fn manifest_contains_inputs_options_plan_and_commands() {
        let records = command_log_records(&[vec![executable_job()]]);
        let manifest = build_manifest(&input_set(), &plan(), &hadd_options(), records, false, None);
        let json = serde_json::to_value(manifest).expect("json");

        assert_eq!(json["inputs"].as_array().expect("inputs").len(), 1);
        assert_eq!(json["options"]["policy"], "fastest");
        assert_eq!(json["options"]["hadd_version"], "hadd fake 1.2.3");
        assert_eq!(json["plan"]["output"], "out.root");
        assert_eq!(json["commands"].as_array().expect("commands").len(), 1);
    }

    #[test]
    fn command_log_is_json_lines() {
        let temp = assert_fs::TempDir::new().expect("temp dir");
        let records = command_log_records(&[vec![executable_job()]]);

        write_command_log(&temp.path().join("commands.jsonl"), &records).expect("write log");
        let contents =
            std::fs::read_to_string(temp.path().join("commands.jsonl")).expect("read log");

        assert_eq!(contents.lines().count(), 1);
        let value: serde_json::Value =
            serde_json::from_str(contents.lines().next().expect("line")).expect("json");
        assert_eq!(value["argv"][0], "hadd");
    }

    fn input_set() -> InputSet {
        InputSet {
            files: vec![InputFile {
                path: PathBuf::from("input.root"),
                size_bytes: 5,
                modified_time: Some(SystemTime::UNIX_EPOCH),
            }],
            total_size_bytes: 5,
        }
    }

    fn plan() -> MergePlan {
        MergePlan {
            output: PathBuf::from("out.root"),
            scratch: PathBuf::from("scratch"),
            policy: MergePolicy::Fastest,
            jobs: 1,
            requested_chunk_count: 1,
            chunk_count: 1,
            fan_in: 8,
            input_count: 1,
            total_input_size_bytes: 5,
            stages: vec![MergeStage {
                level: 0,
                jobs: Vec::new(),
            }],
        }
    }

    fn hadd_options() -> HaddOptions {
        HaddOptions {
            executable: PathBuf::from("hadd"),
            version: Some("hadd fake 1.2.3".to_string()),
            policy: MergePolicy::Fastest,
            hadd_jobs: None,
            temp_dir: None,
            keep_going: false,
            max_open_files: None,
            no_trees: false,
            object_selection: None,
        }
    }

    fn executable_job() -> ExecutableJob {
        ExecutableJob {
            stage_level: 0,
            job_id: 0,
            output: PathBuf::from("out.root"),
            command: HaddCommand {
                argv: vec!["hadd".to_string(), "-f".to_string(), "out.root".to_string()],
            },
        }
    }
}
