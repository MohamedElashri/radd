//! Adaptive benchmark mode.

use std::{
    cmp::Reverse,
    fmt::Write as _,
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::{
    executor::{self, ExecutableJob},
    hadd::{self, HaddOptions},
    input::InputSet,
    planner::{self, MergePlan, MergePolicy},
    staging, validate,
};

pub const DEFAULT_SAMPLE_SIZE: usize = 8;
pub const DEFAULT_JOB_CANDIDATES: &str = "1,2,4,8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkOptions {
    pub job_candidates: Vec<usize>,
    pub sample_size: usize,
    pub scratch: PathBuf,
    pub policy: MergePolicy,
    pub fan_in: usize,
    pub hadd: PathBuf,
    pub keep_going: bool,
    pub hadd_jobs: Option<usize>,
    pub max_open_files: Option<usize>,
    pub no_trees: bool,
    pub object_selection: Option<BenchmarkObjectSelection>,
    pub keep_bench_files: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkObjectSelection {
    pub mode: hadd::ObjectSelectionMode,
    pub objects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkReport {
    pub input_file_count: usize,
    pub sample_file_count: usize,
    pub total_input_bytes: u64,
    pub sampled_input_bytes: u64,
    pub scratch: PathBuf,
    pub keep_bench_files: bool,
    pub candidates: Vec<BenchmarkCandidate>,
    pub recommended_jobs: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkCandidate {
    pub jobs: usize,
    pub elapsed: Duration,
    pub throughput_bytes_per_second: u128,
    pub stage_count: usize,
    pub command_count: usize,
    pub output_size_bytes: u64,
}

#[must_use]
pub fn benchmark_report_json(report: &BenchmarkReport) -> serde_json::Value {
    json!({
        "input_file_count": report.input_file_count,
        "sample_file_count": report.sample_file_count,
        "total_input_bytes": report.total_input_bytes,
        "sampled_input_bytes": report.sampled_input_bytes,
        "scratch": report.scratch,
        "keep_bench_files": report.keep_bench_files,
        "recommended_jobs": report.recommended_jobs,
        "recommended_merge_flags": [
            "--jobs",
            report.recommended_jobs.to_string(),
            "--chunk-count",
            report.recommended_jobs.to_string(),
        ],
        "candidates": report.candidates.iter().map(|candidate| {
            json!({
                "jobs": candidate.jobs,
                "elapsed_seconds": candidate.elapsed.as_secs_f64(),
                "throughput_bytes_per_second": candidate.throughput_bytes_per_second,
                "stage_count": candidate.stage_count,
                "hadd_command_count": candidate.command_count,
                "output_size_bytes": candidate.output_size_bytes,
            })
        }).collect::<Vec<_>>(),
    })
}

pub fn parse_job_candidates(value: &str) -> Result<Vec<usize>> {
    let mut candidates = Vec::new();

    for raw in value.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("--jobs-candidates contains an empty candidate");
        }

        let candidate = trimmed
            .parse::<usize>()
            .with_context(|| format!("invalid jobs candidate `{trimmed}`"))?;
        if candidate == 0 {
            bail!("--jobs-candidates values must be greater than zero");
        }

        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }

    if candidates.is_empty() {
        bail!("--jobs-candidates must include at least one value");
    }

    Ok(candidates)
}

pub fn run_benchmark(input_set: &InputSet, options: &BenchmarkOptions) -> Result<BenchmarkReport> {
    validate_options(options)?;

    let sample = deterministic_sample(input_set, options.sample_size)?;
    let run_scratch = options.scratch.join(run_directory_name());
    let candidates = run_candidates(&sample, options, &run_scratch).inspect_err(|_| {
        if !options.keep_bench_files {
            let _ = fs::remove_dir_all(&run_scratch);
        }
    })?;

    let recommended_jobs = recommend_jobs(&candidates)
        .map(|candidate| candidate.jobs)
        .context("could not recommend jobs because no candidates were benchmarked")?;

    if !options.keep_bench_files {
        fs::remove_dir_all(&run_scratch).with_context(|| {
            format!(
                "could not clean benchmark scratch directory: {}",
                run_scratch.display()
            )
        })?;
    }

    Ok(BenchmarkReport {
        input_file_count: input_set.files.len(),
        sample_file_count: sample.files.len(),
        total_input_bytes: input_set.total_size_bytes,
        sampled_input_bytes: sample.total_size_bytes,
        scratch: run_scratch,
        keep_bench_files: options.keep_bench_files,
        candidates,
        recommended_jobs,
    })
}

#[must_use]
pub fn format_benchmark_report(report: &BenchmarkReport) -> String {
    let mut output = String::new();

    output.push_str("radd bench\n\n");
    output.push_str("results are approximate and depend on current machine and filesystem load\n");
    writeln!(
        &mut output,
        "sample: {} of {} files, {} of {} bytes",
        report.sample_file_count,
        report.input_file_count,
        report.sampled_input_bytes,
        report.total_input_bytes
    )
    .expect("write to string");
    writeln!(&mut output, "scratch: {}", report.scratch.display()).expect("write to string");

    for candidate in &report.candidates {
        writeln!(&mut output, "\ncandidate jobs: {}", candidate.jobs).expect("write to string");
        writeln!(
            &mut output,
            "elapsed: {:.3}s",
            candidate.elapsed.as_secs_f64()
        )
        .expect("write to string");
        writeln!(
            &mut output,
            "throughput: {} MB/s",
            ThroughputDisplay(candidate.throughput_bytes_per_second)
        )
        .expect("write to string");
        writeln!(&mut output, "stages: {}", candidate.stage_count).expect("write to string");
        writeln!(&mut output, "hadd commands: {}", candidate.command_count)
            .expect("write to string");
    }

    writeln!(
        &mut output,
        "\nrecommended jobs: {}",
        report.recommended_jobs
    )
    .expect("write to string");
    writeln!(
        &mut output,
        "recommended merge flags: --jobs {} --chunk-count {}",
        report.recommended_jobs, report.recommended_jobs
    )
    .expect("write to string");

    if report.keep_bench_files {
        output.push_str("benchmark files kept: yes\n");
    } else {
        output.push_str("benchmark files kept: no\n");
    }

    output
}

fn run_candidates(
    sample: &InputSet,
    options: &BenchmarkOptions,
    run_scratch: &std::path::Path,
) -> Result<Vec<BenchmarkCandidate>> {
    let mut candidates = Vec::with_capacity(options.job_candidates.len());

    for jobs in &options.job_candidates {
        candidates.push(run_candidate(sample, options, *jobs, run_scratch)?);
    }

    Ok(candidates)
}

fn run_candidate(
    sample: &InputSet,
    options: &BenchmarkOptions,
    jobs: usize,
    run_scratch: &std::path::Path,
) -> Result<BenchmarkCandidate> {
    let candidate_scratch = run_scratch.join(format!("jobs-{jobs}"));
    fs::create_dir_all(&candidate_scratch).with_context(|| {
        format!(
            "could not create benchmark candidate directory: {}",
            candidate_scratch.display()
        )
    })?;
    let mut plan = planner::build_merge_plan(
        sample,
        planner::PlanOptions {
            output: candidate_scratch.join("radd-bench-output.root"),
            jobs,
            chunk_count: Some(jobs),
            fan_in: options.fan_in,
            scratch: candidate_scratch.join("partials"),
            policy: options.policy,
        },
    )?;

    let hadd_options = HaddOptions {
        executable: options.hadd.clone(),
        version: None,
        policy: plan.policy,
        hadd_jobs: options.hadd_jobs,
        temp_dir: options.hadd_jobs.map(|_| plan.scratch.clone()),
        keep_going: options.keep_going,
        max_open_files: options.max_open_files,
        no_trees: options.no_trees,
        object_selection: options.object_selection.as_ref().map(|selection| {
            hadd::ObjectSelection {
                mode: selection.mode,
                objects: selection.objects.clone(),
                list_path: hadd::object_list_path(&candidate_scratch),
            }
        }),
    };
    hadd::validate_hadd_options(&hadd_options)?;
    attach_hadd_commands(&mut plan, &hadd_options)?;
    let stages = executable_stages(&plan, &hadd_options)?;

    staging::prepare_scratch(&plan)?;
    if let Some(selection) = &hadd_options.object_selection {
        hadd::write_object_list_file(selection)?;
    }
    let report = executor::execute_plan(
        &plan,
        &stages,
        &executor::ExecuteOptions {
            jobs,
            dry_run: false,
        },
    )?;
    let validation = validate::validate_basic(&plan.output)?;
    hadd::cleanup_object_list_file(hadd_options.object_selection.as_ref())?;
    executor::cleanup_temporary_outputs(&plan)?;

    Ok(BenchmarkCandidate {
        jobs,
        elapsed: report.elapsed,
        throughput_bytes_per_second: throughput(sample.total_size_bytes, report.elapsed),
        stage_count: report.stage_count,
        command_count: report.command_count,
        output_size_bytes: validation.size_bytes,
    })
}

fn deterministic_sample(input_set: &InputSet, sample_size: usize) -> Result<InputSet> {
    if sample_size == 0 {
        bail!("--sample-size must be greater than zero");
    }

    let mut files = input_set.files.clone();
    files.sort_by_key(|file| (Reverse(file.size_bytes), file.path.clone()));
    files.truncate(sample_size.min(files.len()));
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let total_size_bytes = files
        .iter()
        .map(|file| file.size_bytes)
        .fold(0_u64, u64::saturating_add);

    Ok(InputSet {
        files,
        total_size_bytes,
    })
}

fn recommend_jobs(candidates: &[BenchmarkCandidate]) -> Option<&BenchmarkCandidate> {
    candidates.iter().max_by(|left, right| {
        left.throughput_bytes_per_second
            .cmp(&right.throughput_bytes_per_second)
            .then_with(|| right.jobs.cmp(&left.jobs))
    })
}

fn executable_stages(plan: &MergePlan, options: &HaddOptions) -> Result<Vec<Vec<ExecutableJob>>> {
    plan.stages
        .iter()
        .map(|stage| {
            stage
                .jobs
                .iter()
                .map(|job| {
                    Ok(ExecutableJob {
                        stage_level: stage.level,
                        job_id: job.id,
                        output: job.output.clone(),
                        command: hadd::build_hadd_command(job, options)?,
                    })
                })
                .collect()
        })
        .collect()
}

fn attach_hadd_commands(plan: &mut MergePlan, options: &HaddOptions) -> Result<()> {
    for stage in &mut plan.stages {
        for job in &mut stage.jobs {
            job.hadd_argv = Some(hadd::build_hadd_command(job, options)?.argv);
        }
    }

    Ok(())
}

fn validate_options(options: &BenchmarkOptions) -> Result<()> {
    if options.job_candidates.is_empty() {
        bail!("--jobs-candidates must include at least one value");
    }

    if options.job_candidates.contains(&0) {
        bail!("--jobs-candidates values must be greater than zero");
    }

    if options.sample_size == 0 {
        bail!("--sample-size must be greater than zero");
    }

    if options.fan_in < 2 {
        bail!("--fan-in must be at least 2");
    }

    if let Some(selection) = &options.object_selection {
        hadd::validate_object_names(&selection.objects)?;
    }

    Ok(())
}

fn throughput(bytes: u64, elapsed: Duration) -> u128 {
    let nanos = elapsed.as_nanos();
    if nanos == 0 {
        return 0;
    }

    u128::from(bytes).saturating_mul(1_000_000_000) / nanos
}

struct ThroughputDisplay(u128);

impl std::fmt::Display for ThroughputDisplay {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let thousandths = self.0.saturating_mul(1_000) / 1_048_576;
        let whole = thousandths / 1_000;
        let fraction = thousandths % 1_000;
        write!(formatter, "{whole}.{fraction:03}")
    }
}

fn run_directory_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("run-{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use super::{BenchmarkCandidate, deterministic_sample, parse_job_candidates, recommend_jobs};
    use crate::input::{InputFile, InputSet};

    #[test]
    fn parses_job_candidates() {
        let candidates = parse_job_candidates("1, 2,4,4").expect("candidates");

        assert_eq!(candidates, [1, 2, 4]);
    }

    #[test]
    fn rejects_invalid_job_candidates() {
        let error = parse_job_candidates("1,0").expect_err("zero should fail");
        assert!(
            error
                .to_string()
                .contains("--jobs-candidates values must be greater than zero")
        );

        let error = parse_job_candidates("1,,2").expect_err("empty should fail");
        assert!(
            error
                .to_string()
                .contains("--jobs-candidates contains an empty candidate")
        );
    }

    #[test]
    fn deterministic_sample_uses_largest_inputs() {
        let sample = deterministic_sample(
            &input_set(&[("small.root", 1), ("large.root", 10), ("mid.root", 5)]),
            2,
        )
        .expect("sample");

        assert_eq!(sample.total_size_bytes, 15);
        assert_eq!(
            sample
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            [PathBuf::from("large.root"), PathBuf::from("mid.root")]
        );
    }

    #[test]
    fn recommendation_picks_highest_throughput_and_lower_jobs_on_tie() {
        let candidates = [candidate(1, 100), candidate(4, 200), candidate(2, 200)];

        let recommendation = recommend_jobs(&candidates).expect("recommendation");

        assert_eq!(recommendation.jobs, 2);
    }

    fn candidate(jobs: usize, throughput_bytes_per_second: u128) -> BenchmarkCandidate {
        BenchmarkCandidate {
            jobs,
            elapsed: std::time::Duration::from_secs(1),
            throughput_bytes_per_second,
            stage_count: 1,
            command_count: 1,
            output_size_bytes: 1,
        }
    }

    fn input_set(files: &[(&str, u64)]) -> InputSet {
        InputSet {
            files: files
                .iter()
                .map(|(path, size_bytes)| InputFile {
                    path: PathBuf::from(path),
                    size_bytes: *size_bytes,
                    modified_time: Some(SystemTime::UNIX_EPOCH),
                })
                .collect(),
            total_size_bytes: files.iter().map(|(_, size_bytes)| *size_bytes).sum(),
        }
    }
}
