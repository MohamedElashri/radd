//! Merge planning.

use std::{
    cmp::Reverse,
    fmt::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::input::{InputFile, InputSet};

pub const DEFAULT_FAN_IN: usize = 8;
pub const DEFAULT_MAX_JOBS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum MergePolicy {
    Fastest,
    Balanced,
    Smallest,
    Reproducible,
}

impl fmt::Display for MergePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Fastest => "fastest",
            Self::Balanced => "balanced",
            Self::Smallest => "smallest",
            Self::Reproducible => "reproducible",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOptions {
    pub output: PathBuf,
    pub jobs: usize,
    pub chunk_count: Option<usize>,
    pub fan_in: usize,
    pub scratch: PathBuf,
    pub policy: MergePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergePlan {
    pub output: PathBuf,
    pub scratch: PathBuf,
    pub policy: MergePolicy,
    pub jobs: usize,
    pub requested_chunk_count: usize,
    pub chunk_count: usize,
    pub fan_in: usize,
    pub input_count: usize,
    pub total_input_size_bytes: u64,
    pub stages: Vec<MergeStage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergeStage {
    pub level: usize,
    pub jobs: Vec<MergeJob>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergeJob {
    pub id: usize,
    pub output: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub input_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hadd_argv: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct Bucket {
    inputs: Vec<InputFile>,
    total_size_bytes: u64,
}

#[derive(Debug, Clone)]
struct PartialOutput {
    path: PathBuf,
    size_bytes: u64,
}

pub fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |parallelism| {
        parallelism.get().clamp(1, DEFAULT_MAX_JOBS)
    })
}

pub fn default_scratch() -> PathBuf {
    std::env::temp_dir().join("radd")
}

pub fn build_merge_plan(input_set: &InputSet, options: PlanOptions) -> Result<MergePlan> {
    validate_options(input_set, &options)?;

    let requested_chunk_count = options.chunk_count.unwrap_or(options.jobs);
    let buckets = size_balanced_chunks(&input_set.files, requested_chunk_count);
    let chunk_count = buckets.len();
    let mut stages = Vec::new();

    let first_stage = build_first_stage(&buckets, &options);
    let mut current_partials = first_stage
        .jobs
        .iter()
        .map(|job| PartialOutput {
            path: job.output.clone(),
            size_bytes: job.input_size_bytes,
        })
        .collect::<Vec<_>>();

    stages.push(first_stage);

    let mut level = 1;
    while current_partials.len() > 1 {
        let stage = build_tree_stage(level, &current_partials, &options);
        current_partials = stage
            .jobs
            .iter()
            .map(|job| PartialOutput {
                path: job.output.clone(),
                size_bytes: job.input_size_bytes,
            })
            .collect();
        stages.push(stage);
        level += 1;
    }

    Ok(MergePlan {
        output: options.output,
        scratch: options.scratch,
        policy: options.policy,
        jobs: options.jobs,
        requested_chunk_count,
        chunk_count,
        fan_in: options.fan_in,
        input_count: input_set.files.len(),
        total_input_size_bytes: input_set.total_size_bytes,
        stages,
    })
}

pub fn format_human_plan(plan: &MergePlan) -> String {
    let mut output = String::new();

    output.push_str("radd plan\n\n");
    writeln!(&mut output, "output: {}", plan.output.display()).expect("write to string");
    writeln!(&mut output, "policy: {}", plan.policy).expect("write to string");
    writeln!(&mut output, "jobs: {}", plan.jobs).expect("write to string");
    writeln!(
        &mut output,
        "chunks: {} non-empty of {} requested",
        plan.chunk_count, plan.requested_chunk_count
    )
    .expect("write to string");
    writeln!(&mut output, "fan-in: {}", plan.fan_in).expect("write to string");
    writeln!(&mut output, "scratch: {}", plan.scratch.display()).expect("write to string");
    writeln!(
        &mut output,
        "inputs: {} {}",
        plan.input_count,
        plural(plan.input_count, "file", "files")
    )
    .expect("write to string");
    writeln!(
        &mut output,
        "total size: {} bytes",
        plan.total_input_size_bytes
    )
    .expect("write to string");
    writeln!(
        &mut output,
        "stages: {} {}",
        plan.stages.len(),
        plural(plan.stages.len(), "stage", "stages")
    )
    .expect("write to string");

    for stage in &plan.stages {
        writeln!(
            &mut output,
            "\nstage {}: {} {}",
            stage.level,
            stage.jobs.len(),
            plural(stage.jobs.len(), "job", "jobs")
        )
        .expect("write to string");

        for job in &stage.jobs {
            writeln!(
                &mut output,
                "  job {}: {} {}, {} bytes -> {}",
                job.id,
                job.inputs.len(),
                plural(job.inputs.len(), "input", "inputs"),
                job.input_size_bytes,
                job.output.display()
            )
            .expect("write to string");

            if let Some(hadd_argv) = &job.hadd_argv {
                writeln!(
                    &mut output,
                    "    command: {}",
                    crate::hadd::format_argv_for_display(hadd_argv)
                )
                .expect("write to string");
            }
        }
    }

    output
}

fn validate_options(input_set: &InputSet, options: &PlanOptions) -> Result<()> {
    if input_set.files.is_empty() {
        bail!("cannot plan a merge with no input files");
    }

    if options.jobs == 0 {
        bail!("--jobs must be greater than zero");
    }

    if let Some(chunk_count) = options.chunk_count
        && chunk_count == 0
    {
        bail!("--chunk-count must be greater than zero");
    }

    if options.fan_in < 2 {
        bail!("--fan-in must be at least 2");
    }

    Ok(())
}

fn size_balanced_chunks(files: &[InputFile], chunk_count: usize) -> Vec<Bucket> {
    let mut buckets = vec![
        Bucket {
            inputs: Vec::new(),
            total_size_bytes: 0,
        };
        chunk_count
    ];

    let mut sorted_files = files.to_vec();
    sorted_files.sort_by_key(|file| (Reverse(file.size_bytes), file.path.clone()));

    for file in sorted_files {
        let lightest_index = buckets
            .iter()
            .enumerate()
            .min_by_key(|(index, bucket)| (bucket.total_size_bytes, *index))
            .map_or(0, |(index, _)| index);

        let bucket = &mut buckets[lightest_index];
        bucket.total_size_bytes = bucket.total_size_bytes.saturating_add(file.size_bytes);
        bucket.inputs.push(file);
    }

    buckets
        .into_iter()
        .filter(|bucket| !bucket.inputs.is_empty())
        .collect()
}

fn build_first_stage(buckets: &[Bucket], options: &PlanOptions) -> MergeStage {
    let only_job = buckets.len() == 1;
    let jobs = buckets
        .iter()
        .enumerate()
        .map(|(index, bucket)| MergeJob {
            id: index,
            output: if only_job {
                options.output.clone()
            } else {
                partial_path(&options.scratch, 0, index)
            },
            inputs: bucket
                .inputs
                .iter()
                .map(|input| input.path.clone())
                .collect(),
            input_size_bytes: bucket.total_size_bytes,
            hadd_argv: None,
        })
        .collect();

    MergeStage { level: 0, jobs }
}

fn build_tree_stage(level: usize, current: &[PartialOutput], options: &PlanOptions) -> MergeStage {
    let group_count = current.len().div_ceil(options.fan_in);
    let jobs = current
        .chunks(options.fan_in)
        .enumerate()
        .map(|(index, group)| MergeJob {
            id: index,
            output: if group_count == 1 {
                options.output.clone()
            } else {
                partial_path(&options.scratch, level, index)
            },
            inputs: group.iter().map(|partial| partial.path.clone()).collect(),
            input_size_bytes: group
                .iter()
                .map(|partial| partial.size_bytes)
                .fold(0_u64, u64::saturating_add),
            hadd_argv: None,
        })
        .collect();

    MergeStage { level, jobs }
}

fn partial_path(scratch: &Path, level: usize, job: usize) -> PathBuf {
    scratch.join(format!("radd-stage-{level}-job-{job}.root"))
}

fn plural(count: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use super::{MergePolicy, PlanOptions, build_merge_plan, default_scratch, format_human_plan};
    use crate::input::{InputFile, InputSet};

    #[test]
    fn equal_size_files_are_spread_across_chunks() {
        let input_set = input_set(&[
            ("a.root", 10),
            ("b.root", 10),
            ("c.root", 10),
            ("d.root", 10),
        ]);
        let plan = build_merge_plan(&input_set, options(2, Some(2), 8)).expect("plan");

        assert_eq!(plan.chunk_count, 2);
        assert_eq!(plan.stages[0].jobs.len(), 2);
        assert_eq!(plan.stages[0].jobs[0].inputs.len(), 2);
        assert_eq!(plan.stages[0].jobs[1].inputs.len(), 2);
        assert_eq!(plan.stages[0].jobs[0].input_size_bytes, 20);
        assert_eq!(plan.stages[0].jobs[1].input_size_bytes, 20);
    }

    #[test]
    fn skewed_files_use_longest_processing_time_assignment() {
        let input_set = input_set(&[
            ("large.root", 100),
            ("medium.root", 80),
            ("small-a.root", 20),
            ("small-b.root", 10),
        ]);
        let plan = build_merge_plan(&input_set, options(2, Some(2), 8)).expect("plan");
        let stage = &plan.stages[0];

        assert_eq!(stage.jobs[0].input_size_bytes, 110);
        assert_eq!(stage.jobs[1].input_size_bytes, 100);
    }

    #[test]
    fn fewer_files_than_requested_chunks_ignores_empty_chunks() {
        let input_set = input_set(&[("a.root", 5), ("b.root", 7)]);
        let plan = build_merge_plan(&input_set, options(8, Some(8), 8)).expect("plan");

        assert_eq!(plan.requested_chunk_count, 8);
        assert_eq!(plan.chunk_count, 2);
        assert_eq!(plan.stages[0].jobs.len(), 2);
    }

    #[test]
    fn fan_in_generates_multiple_tree_levels() {
        let input_set = input_set(&[
            ("a.root", 1),
            ("b.root", 1),
            ("c.root", 1),
            ("d.root", 1),
            ("e.root", 1),
        ]);
        let plan = build_merge_plan(&input_set, options(5, Some(5), 2)).expect("plan");

        assert_eq!(plan.stages.len(), 4);
        assert_eq!(plan.stages[0].jobs.len(), 5);
        assert_eq!(plan.stages[1].jobs.len(), 3);
        assert_eq!(plan.stages[2].jobs.len(), 2);
        assert_eq!(plan.stages[3].jobs.len(), 1);
        assert_eq!(plan.stages[3].jobs[0].output, PathBuf::from("out.root"));
    }

    #[test]
    fn planning_is_deterministic() {
        let input_set = input_set(&[("b.root", 10), ("a.root", 10), ("c.root", 2)]);
        let first = build_merge_plan(&input_set, options(2, Some(2), 8)).expect("first plan");
        let second = build_merge_plan(&input_set, options(2, Some(2), 8)).expect("second plan");

        assert_eq!(first, second);
    }

    #[test]
    fn json_plan_has_expected_shape() {
        let input_set = input_set(&[("a.root", 3), ("b.root", 2)]);
        let plan = build_merge_plan(&input_set, options(2, Some(2), 8)).expect("plan");
        let json = serde_json::to_value(&plan).expect("json");

        assert_eq!(json["output"], "out.root");
        assert_eq!(json["policy"], "fastest");
        assert_eq!(json["chunk_count"], 2);
        assert!(
            json["stages"]
                .as_array()
                .is_some_and(|stages| stages.len() == 2)
        );
    }

    #[test]
    fn human_plan_mentions_topology() {
        let input_set = input_set(&[("a.root", 3), ("b.root", 2)]);
        let plan = build_merge_plan(&input_set, options(2, Some(2), 8)).expect("plan");
        let text = format_human_plan(&plan);

        assert!(text.contains("chunks: 2 non-empty of 2 requested"));
        assert!(text.contains("stage 0: 2 jobs"));
        assert!(text.contains("stage 1: 1 job"));
    }

    fn options(jobs: usize, chunk_count: Option<usize>, fan_in: usize) -> PlanOptions {
        PlanOptions {
            output: PathBuf::from("out.root"),
            jobs,
            chunk_count,
            fan_in,
            scratch: default_scratch(),
            policy: MergePolicy::Fastest,
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
