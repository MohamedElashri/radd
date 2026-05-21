//! Merge execution.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::{hadd::HaddCommand, planner::MergePlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteOptions {
    pub jobs: usize,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    pub stage_count: usize,
    pub command_count: usize,
    pub elapsed: Duration,
    pub dry_run: bool,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

#[derive(Debug, Clone)]
pub struct ExecutableJob {
    pub stage_level: usize,
    pub job_id: usize,
    pub output: PathBuf,
    pub command: HaddCommand,
}

#[derive(Debug, Clone)]
struct JobFailure {
    stage_level: usize,
    job_id: usize,
    argv: Vec<String>,
    message: String,
}

pub fn execute_plan(
    plan: &MergePlan,
    stages: &[Vec<ExecutableJob>],
    options: &ExecuteOptions,
) -> Result<ExecutionReport> {
    validate_execution_inputs(stages, options)?;

    let started = Instant::now();
    let command_count = stages.iter().map(Vec::len).sum();

    if options.dry_run {
        return Ok(ExecutionReport {
            stage_count: stages.len(),
            command_count,
            elapsed: started.elapsed(),
            dry_run: true,
            cache_hits: 0,
            cache_misses: 0,
        });
    }

    for stage in stages {
        run_stage(stage, options.jobs)?;
        ensure_stage_outputs_exist(stage)?;
    }

    Ok(ExecutionReport {
        stage_count: plan.stages.len(),
        command_count,
        elapsed: started.elapsed(),
        dry_run: false,
        cache_hits: 0,
        cache_misses: 0,
    })
}

pub fn cleanup_temporary_outputs(plan: &MergePlan) -> Result<()> {
    let mut first_error = None;
    let temporary_outputs = temporary_outputs(plan);

    for path in &temporary_outputs {
        if let Err(error) = fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
            && first_error.is_none()
        {
            first_error = Some(anyhow::Error::new(error).context(format!(
                "could not remove temporary output: {}",
                path.display()
            )));
        }
    }

    if !temporary_outputs.is_empty()
        && let Err(error) = fs::remove_dir(&plan.scratch)
        && error.kind() != std::io::ErrorKind::NotFound
        && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
        && first_error.is_none()
    {
        first_error = Some(anyhow::Error::new(error).context(format!(
            "could not remove scratch directory: {}",
            plan.scratch.display()
        )));
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}

pub fn temporary_outputs(plan: &MergePlan) -> Vec<PathBuf> {
    plan.stages
        .iter()
        .flat_map(|stage| &stage.jobs)
        .filter(|job| job.output != plan.output)
        .map(|job| job.output.clone())
        .collect()
}

fn run_stage(stage: &[ExecutableJob], jobs: usize) -> Result<()> {
    if stage.is_empty() {
        return Ok(());
    }

    let worker_count = jobs.min(stage.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(stage.to_vec())));
    let failures = Arc::new(Mutex::new(Vec::<JobFailure>::new()));

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let failures = Arc::clone(&failures);

            scope.spawn(move || {
                loop {
                    if !failures.lock().expect("failure mutex").is_empty() {
                        break;
                    }

                    let Some(job) = queue.lock().expect("queue mutex").pop_front() else {
                        break;
                    };

                    if let Err(error) = run_job(&job) {
                        failures.lock().expect("failure mutex").push(JobFailure {
                            stage_level: job.stage_level,
                            job_id: job.job_id,
                            argv: job.command.argv.clone(),
                            message: error.to_string(),
                        });
                        break;
                    }
                }
            });
        }
    });

    let failures = failures.lock().expect("failure mutex");
    if let Some(failure) = failures.first() {
        bail!(
            "hadd job failed at stage {} job {}: {}\ncommand: {}",
            failure.stage_level,
            failure.job_id,
            failure.message,
            crate::hadd::format_argv_for_display(&failure.argv)
        );
    }

    Ok(())
}

fn run_job(job: &ExecutableJob) -> Result<()> {
    let argv = &job.command.argv;
    let Some(executable) = argv.first() else {
        bail!(
            "empty hadd command for stage {} job {}",
            job.stage_level,
            job.job_id
        );
    };

    let output = Command::new(executable)
        .args(&argv[1..])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not start hadd executable `{executable}`"))?;

    if !output.status.success() {
        let code = output.status.code().map_or_else(
            || "terminated by signal".to_string(),
            |code| code.to_string(),
        );
        let diagnostics = command_diagnostics(&output.stdout, &output.stderr);
        if diagnostics.is_empty() {
            bail!("process exited with status {code}");
        }
        bail!("process exited with status {code}: {diagnostics}");
    }

    Ok(())
}

fn command_diagnostics(stdout: &[u8], stderr: &[u8]) -> String {
    let mut diagnostics = String::new();
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);

    for line in stderr.lines().chain(stdout.lines()).map(str::trim) {
        if !line.is_empty() {
            diagnostics.push_str(line);
            break;
        }
    }

    diagnostics
}

fn ensure_stage_outputs_exist(stage: &[ExecutableJob]) -> Result<()> {
    for job in stage {
        ensure_output_exists(&job.output).with_context(|| {
            format!(
                "stage {} job {} did not produce expected output {}",
                job.stage_level,
                job.job_id,
                job.output.display()
            )
        })?;
    }

    Ok(())
}

fn ensure_output_exists(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("expected output does not exist: {}", path.display()))?;

    if !metadata.is_file() {
        bail!("expected output is not a file: {}", path.display());
    }

    Ok(())
}

fn validate_execution_inputs(
    stages: &[Vec<ExecutableJob>],
    options: &ExecuteOptions,
) -> Result<()> {
    if options.jobs == 0 {
        bail!("--jobs must be greater than zero");
    }

    for stage in stages {
        for job in stage {
            if job.command.argv.is_empty() {
                bail!(
                    "empty hadd command for stage {} job {}",
                    job.stage_level,
                    job.job_id
                );
            }
        }
    }

    Ok(())
}
