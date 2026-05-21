//! Command-line interface.

use std::{
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use clap::{Args, CommandFactory, Parser, Subcommand};

use crate::{bench, cache, executor, hadd, input, inspect, planner, staging, telemetry, validate};

/// A safe Rust frontend for orchestrating ROOT hadd merges.
#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about,
    long_about = None,
    disable_version_flag = true,
    arg_required_else_help = true
)]
pub struct Cli {
    /// Increase diagnostic output.
    #[arg(long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Reduce diagnostic output.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Print version.
    #[arg(
        short = 'v',
        visible_short_alias = 'V',
        long = "version",
        action = clap::ArgAction::SetTrue,
        global = true
    )]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print radd version.
    Version,

    /// Check local radd, ROOT, and filesystem prerequisites.
    Doctor(DoctorArgs),

    /// Print the merge plan without running hadd.
    Plan(PlanArgs),

    /// Merge ROOT files by orchestrating hadd.
    Merge(MergeArgs),

    /// Validate an output ROOT file.
    Validate(ValidateArgs),

    /// Inspect input files or manifests.
    Inspect(InputListArgs),

    /// Benchmark candidate merge settings.
    Bench(BenchArgs),

    /// Manage cached partial merges.
    Cache(CacheArgs),
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// hadd executable name or path to check.
    #[arg(long, default_value = "hadd")]
    pub hadd: PathBuf,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct PlanArgs {
    /// Output ROOT file path.
    pub output: PathBuf,

    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,

    /// Number of merge jobs to plan for.
    #[arg(long)]
    pub jobs: Option<usize>,

    /// Number of first-level chunks to create.
    #[arg(long)]
    pub chunk_count: Option<usize>,

    /// Number of partial outputs to merge in one tree job.
    #[arg(long, default_value_t = planner::DEFAULT_FAN_IN)]
    pub fan_in: usize,

    /// Scratch directory for intermediate partial outputs.
    #[arg(long)]
    pub scratch: Option<PathBuf>,

    /// Merge optimization policy.
    #[arg(long, value_enum, default_value = "fastest")]
    pub policy: planner::MergePolicy,

    /// Emit the plan as JSON.
    #[arg(long)]
    pub json: bool,

    /// Include exact hadd argv vectors in the printed plan.
    #[arg(long)]
    pub commands: bool,

    /// hadd executable name or path to use in planned commands.
    #[arg(long, default_value = "hadd")]
    pub hadd: PathBuf,

    /// Continue past unreadable or corrupt input files inside hadd.
    #[arg(long)]
    pub keep_going: bool,

    /// Number of worker processes for each hadd invocation.
    #[arg(long)]
    pub hadd_jobs: Option<usize>,

    /// Maximum number of files hadd may keep open.
    #[arg(long)]
    pub max_open_files: Option<usize>,

    /// Skip `TTree` merging by passing -T to hadd.
    #[arg(long)]
    pub no_trees: bool,

    /// Only merge the named top-level object or directory; may be repeated.
    #[arg(long, value_name = "OBJECT")]
    pub only: Vec<String>,

    /// Skip the named top-level object or directory; may be repeated.
    #[arg(long, value_name = "OBJECT")]
    pub skip: Vec<String>,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct MergeArgs {
    /// Output ROOT file path.
    pub output: PathBuf,

    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,

    /// Number of merge jobs to run concurrently within each stage.
    #[arg(long)]
    pub jobs: Option<usize>,

    /// Number of first-level chunks to create.
    #[arg(long)]
    pub chunk_count: Option<usize>,

    /// Number of partial outputs to merge in one tree job.
    #[arg(long, default_value_t = planner::DEFAULT_FAN_IN)]
    pub fan_in: usize,

    /// Scratch directory for intermediate partial outputs.
    #[arg(long)]
    pub scratch: Option<PathBuf>,

    /// Merge optimization policy.
    #[arg(long, value_enum, default_value = "fastest")]
    pub policy: planner::MergePolicy,

    /// Print the planned commands without creating scratch files or running hadd.
    #[arg(long)]
    pub dry_run: bool,

    /// Allow overwriting an existing output file.
    #[arg(long)]
    pub force: bool,

    /// Emit merge telemetry as JSON.
    #[arg(long)]
    pub json: bool,

    /// Write a reproducibility manifest JSON file.
    #[arg(long)]
    pub manifest: Option<PathBuf>,

    /// Write planned hadd commands as JSON Lines.
    #[arg(long)]
    pub command_log: Option<PathBuf>,

    /// Reuse and populate cached first-stage partial merges.
    #[arg(long)]
    pub cache: bool,

    /// Skip the default post-merge basic output validation.
    #[arg(long)]
    pub no_validate: bool,

    /// Copy or hardlink inputs into scratch before running hadd.
    #[arg(long)]
    pub stage_inputs: bool,

    /// Keep staged input files after a successful staged merge.
    #[arg(long)]
    pub keep_staged_inputs: bool,

    /// hadd executable name or path to run.
    #[arg(long, default_value = "hadd")]
    pub hadd: PathBuf,

    /// Ask hadd to continue past unreadable or corrupt input files; radd still stops if a stage output is not produced.
    #[arg(long)]
    pub keep_going: bool,

    /// Number of worker processes for each hadd invocation.
    #[arg(long)]
    pub hadd_jobs: Option<usize>,

    /// Maximum number of files hadd may keep open.
    #[arg(long)]
    pub max_open_files: Option<usize>,

    /// Skip `TTree` merging by passing -T to hadd.
    #[arg(long)]
    pub no_trees: bool,

    /// Only merge the named top-level object or directory; may be repeated.
    #[arg(long, value_name = "OBJECT")]
    pub only: Vec<String>,

    /// Skip the named top-level object or directory; may be repeated.
    #[arg(long, value_name = "OBJECT")]
    pub skip: Vec<String>,
}

#[derive(Debug, Args)]
pub struct InputListArgs {
    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,

    /// Attempt optional ROOT-backed metadata inspection.
    #[arg(long)]
    pub root_metadata: bool,

    /// ROOT executable name or path to use for metadata inspection.
    #[arg(long, default_value = "root")]
    pub root: PathBuf,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct BenchArgs {
    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,

    /// Comma-separated radd job counts to benchmark.
    #[arg(long, default_value = bench::DEFAULT_JOB_CANDIDATES)]
    pub jobs_candidates: String,

    /// Maximum number of input files to sample deterministically.
    #[arg(long, default_value_t = bench::DEFAULT_SAMPLE_SIZE)]
    pub sample_size: usize,

    /// Scratch directory for benchmark outputs.
    #[arg(long)]
    pub scratch: Option<PathBuf>,

    /// Number of partial outputs to merge in one tree job.
    #[arg(long, default_value_t = planner::DEFAULT_FAN_IN)]
    pub fan_in: usize,

    /// Merge optimization policy.
    #[arg(long, value_enum, default_value = "fastest")]
    pub policy: planner::MergePolicy,

    /// Emit benchmark results as JSON.
    #[arg(long)]
    pub json: bool,

    /// Keep benchmark scratch files after successful benchmarking.
    #[arg(long)]
    pub keep_bench_files: bool,

    /// hadd executable name or path to run.
    #[arg(long, default_value = "hadd")]
    pub hadd: PathBuf,

    /// Ask hadd to continue past unreadable or corrupt input files; radd still stops if a stage output is not produced.
    #[arg(long)]
    pub keep_going: bool,

    /// Number of worker processes for each hadd invocation.
    #[arg(long)]
    pub hadd_jobs: Option<usize>,

    /// Maximum number of files hadd may keep open.
    #[arg(long)]
    pub max_open_files: Option<usize>,

    /// Skip `TTree` merging by passing -T to hadd.
    #[arg(long)]
    pub no_trees: bool,

    /// Only merge the named top-level object or directory; may be repeated.
    #[arg(long, value_name = "OBJECT")]
    pub only: Vec<String>,

    /// Skip the named top-level object or directory; may be repeated.
    #[arg(long, value_name = "OBJECT")]
    pub skip: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Output ROOT file path to validate.
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommand,
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// List cached partial merges.
    List,

    /// Clean cached partial merges.
    Clean,
}

pub fn run() -> Result<()> {
    run_with(Cli::parse())
}

fn run_with(cli: Cli) -> Result<()> {
    if cli.version {
        run_version();
        return Ok(());
    }

    match cli.command {
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
        Some(command) => run_command(command),
    }
}

fn run_command(command: Commands) -> Result<()> {
    match command {
        Commands::Version => {
            run_version();
            Ok(())
        }
        Commands::Doctor(args) => run_doctor(&args),
        Commands::Plan(args) => run_plan(&args),
        Commands::Merge(args) => run_merge(&args),
        Commands::Validate(args) => run_validate(&args),
        Commands::Inspect(args) => run_inspect(&args),
        Commands::Bench(args) => run_bench(&args),
        Commands::Cache(CacheArgs { command }) => match command {
            CacheCommand::List => run_cache_list(),
            CacheCommand::Clean => run_cache_clean(),
        },
    }
}

fn run_version() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

fn run_doctor(args: &DoctorArgs) -> Result<()> {
    let report = DoctorReport::check(&args.hadd);
    print!("{report}");

    if report.ok {
        Ok(())
    } else {
        bail!("doctor checks failed")
    }
}

fn run_inspect(args: &InputListArgs) -> Result<()> {
    let input_set = input::resolve_inputs(&args.inputs)?;
    let report = inspect::inspect_inputs(
        &input_set,
        &inspect::InspectOptions {
            root_metadata: args.root_metadata,
            root: args.root.clone(),
        },
    );
    print!("{}", inspect::format_inspect_report(&report));
    Ok(())
}

fn run_validate(args: &ValidateArgs) -> Result<()> {
    let report = validate::validate_basic(&args.output)?;
    print!("{}", validate::format_validation_report(&report));
    Ok(())
}

fn run_bench(args: &BenchArgs) -> Result<()> {
    let input_set = input::resolve_inputs(&args.inputs)?;
    let job_candidates = bench::parse_job_candidates(&args.jobs_candidates)?;
    let object_selection = bench_object_selection_from_values(&args.only, &args.skip)?;
    if object_selection.is_some() {
        let capabilities = hadd::detect_capabilities(&args.hadd)?;
        if !capabilities.object_lists {
            bail!(
                "`{}` does not appear to support hadd object-list options (-L and -Ltype)",
                args.hadd.display()
            );
        }
    }
    let report = bench::run_benchmark(
        &input_set,
        &bench::BenchmarkOptions {
            job_candidates,
            sample_size: args.sample_size,
            scratch: args
                .scratch
                .clone()
                .unwrap_or_else(|| std::env::temp_dir().join("radd-bench")),
            policy: args.policy,
            fan_in: args.fan_in,
            hadd: args.hadd.clone(),
            keep_going: args.keep_going,
            hadd_jobs: args.hadd_jobs,
            max_open_files: args.max_open_files,
            no_trees: args.no_trees,
            object_selection,
            keep_bench_files: args.keep_bench_files,
        },
    )?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&bench::benchmark_report_json(&report))?
        );
    } else {
        print!("{}", bench::format_benchmark_report(&report));
    }
    Ok(())
}

fn run_plan(args: &PlanArgs) -> Result<()> {
    let input_set = input::resolve_inputs(&args.inputs)?;
    let mut plan = planner::build_merge_plan(
        &input_set,
        planner::PlanOptions {
            output: args.output.clone(),
            jobs: args.jobs.unwrap_or_else(planner::default_jobs),
            chunk_count: args.chunk_count,
            fan_in: args.fan_in,
            scratch: args
                .scratch
                .clone()
                .unwrap_or_else(planner::default_scratch),
            policy: args.policy,
        },
    )?;

    let hadd_options = hadd_options_from_plan(&plan, args)?;
    hadd::validate_hadd_options(&hadd_options)?;
    ensure_requested_hadd_capabilities(&hadd_options)?;

    if args.commands {
        attach_hadd_commands(&mut plan, &hadd_options, None)?;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        print!("{}", planner::format_human_plan(&plan));
    }

    Ok(())
}

fn run_merge(args: &MergeArgs) -> Result<()> {
    let input_set = input::resolve_inputs(&args.inputs)?;
    if !args.dry_run {
        validate_merge_output_safety(&args.output, &input_set, args.force)?;
    }

    let mut plan = planner::build_merge_plan(
        &input_set,
        planner::PlanOptions {
            output: args.output.clone(),
            jobs: args.jobs.unwrap_or_else(planner::default_jobs),
            chunk_count: args.chunk_count,
            fan_in: args.fan_in,
            scratch: args
                .scratch
                .clone()
                .unwrap_or_else(planner::default_scratch),
            policy: args.policy,
        },
    )?;

    let mut hadd_options = hadd_options_from_merge(&plan, args)?;
    hadd_options.version = hadd::detect_version(&hadd_options.executable);
    hadd::validate_hadd_options(&hadd_options)?;
    ensure_requested_hadd_capabilities(&hadd_options)?;

    let input_staging_plan = args
        .stage_inputs
        .then(|| staging::plan_input_staging(&input_set, &plan.scratch, args.keep_staged_inputs));
    let stages = executable_stages(&plan, &hadd_options, input_staging_plan.as_ref())?;
    let command_records = telemetry::command_log_records(&stages);
    attach_hadd_commands(&mut plan, &hadd_options, input_staging_plan.as_ref())?;

    if args.dry_run {
        let started_at = SystemTime::now();
        let report = executor::execute_plan(
            &plan,
            &stages,
            &executor::ExecuteOptions {
                jobs: plan.jobs,
                dry_run: true,
            },
        )?;
        let ended_at = SystemTime::now();
        let run_telemetry = telemetry::build_telemetry(
            &input_set,
            &plan,
            &hadd_options,
            &report,
            telemetry::RunTiming {
                started_at,
                ended_at,
            },
            None,
            input_staging_plan
                .as_ref()
                .map(telemetry::StagingTelemetry::planned),
        );
        write_requested_artifacts(
            args,
            &input_set,
            &plan,
            &hadd_options,
            &command_records,
            input_staging_plan
                .as_ref()
                .map(telemetry::StagingTelemetry::planned),
        )?;

        if args.json {
            println!("{}", serde_json::to_string_pretty(&run_telemetry)?);
        } else {
            print!("{}", planner::format_human_plan(&plan));
            println!("\ndry-run: no scratch directories created and no hadd commands executed");
        }

        return Ok(());
    }

    run_executed_merge(
        args,
        &input_set,
        &plan,
        &hadd_options,
        &stages,
        &command_records,
        input_staging_plan.as_ref(),
    )
}

fn run_cache_list() -> Result<()> {
    let cache = cache::list_cache(&cache::default_cache_root())?;
    print!("{}", cache::format_cache_list(&cache));
    Ok(())
}

fn run_cache_clean() -> Result<()> {
    let report = cache::clean_cache(&cache::default_cache_root())?;
    print!("{}", cache::format_clean_report(&report));
    Ok(())
}

fn run_executed_merge(
    args: &MergeArgs,
    input_set: &input::InputSet,
    plan: &planner::MergePlan,
    hadd_options: &hadd::HaddOptions,
    stages: &[Vec<executor::ExecutableJob>],
    command_records: &[telemetry::CommandLogRecord],
    input_staging_plan: Option<&staging::InputStagingPlan>,
) -> Result<()> {
    let started_at = SystemTime::now();
    staging::prepare_scratch(plan)?;
    let input_staging = input_staging_plan
        .map(staging::prepare_input_staging)
        .transpose()?;
    if let Some(selection) = &hadd_options.object_selection {
        hadd::write_object_list_file(selection)?;
    }
    let prepared_cache = prepare_cache(args, input_set, plan, hadd_options, stages)?;
    let execution_stages = prepared_cache
        .as_ref()
        .map_or(stages, |prepared| prepared.stages.as_slice());
    let mut report = executor::execute_plan(
        plan,
        execution_stages,
        &executor::ExecuteOptions {
            jobs: plan.jobs,
            dry_run: false,
        },
    )?;
    apply_cache_report(&mut report, prepared_cache.as_ref())?;

    let validation = if args.no_validate {
        None
    } else {
        Some(validate::validate_basic(&plan.output)?)
    };
    let output_size_bytes = telemetry::output_size(&plan.output);
    hadd::cleanup_object_list_file(hadd_options.object_selection.as_ref())?;
    executor::cleanup_temporary_outputs(plan)?;
    let ended_at = SystemTime::now();
    let run_telemetry = telemetry::build_telemetry(
        input_set,
        plan,
        hadd_options,
        &report,
        telemetry::RunTiming {
            started_at,
            ended_at,
        },
        output_size_bytes,
        input_staging
            .as_ref()
            .map(telemetry::StagingTelemetry::executed),
    );
    write_requested_artifacts(
        args,
        input_set,
        plan,
        hadd_options,
        command_records,
        input_staging
            .as_ref()
            .map(telemetry::StagingTelemetry::executed),
    )?;
    if let Some(input_staging) = &input_staging
        && !input_staging.plan.keep_after_success
    {
        staging::cleanup_staged_inputs(&input_staging.plan)?;
    }
    print_merge_result(
        args,
        plan,
        &report,
        output_size_bytes,
        validation.as_ref(),
        &run_telemetry,
    )
}

fn prepare_cache(
    args: &MergeArgs,
    input_set: &input::InputSet,
    plan: &planner::MergePlan,
    hadd_options: &hadd::HaddOptions,
    stages: &[Vec<executor::ExecutableJob>],
) -> Result<Option<cache::PreparedExecution>> {
    if !args.cache {
        return Ok(None);
    }

    Ok(Some(cache::prepare_execution(
        &cache::default_cache_root(),
        input_set,
        plan,
        stages,
        hadd_options,
    )?))
}

fn apply_cache_report(
    report: &mut executor::ExecutionReport,
    prepared_cache: Option<&cache::PreparedExecution>,
) -> Result<()> {
    if let Some(prepared_cache) = prepared_cache {
        cache::store_pending(&prepared_cache.pending_stores)?;
        report.cache_hits = prepared_cache.hits;
        report.cache_misses = prepared_cache.misses;
    }

    Ok(())
}

fn print_merge_result(
    args: &MergeArgs,
    plan: &planner::MergePlan,
    report: &executor::ExecutionReport,
    output_size_bytes: Option<u64>,
    validation: Option<&validate::ValidationReport>,
    run_telemetry: &telemetry::MergeTelemetry,
) -> Result<()> {
    if args.json {
        println!("{}", serde_json::to_string_pretty(run_telemetry)?);
        return Ok(());
    }

    println!("radd merge complete");
    println!("output: {}", plan.output.display());
    if let Some(output_size_bytes) = output_size_bytes {
        println!("output size: {output_size_bytes} bytes");
    }
    println!("stages: {}", report.stage_count);
    println!("hadd commands: {}", report.command_count);
    if args.cache {
        println!("cache hits: {}", report.cache_hits);
        println!("cache misses: {}", report.cache_misses);
    }
    if let Some(input_staging) = &run_telemetry.input_staging
        && input_staging.enabled
    {
        println!(
            "input staging: {} files, {} bytes, {} hardlinks, {} copies",
            input_staging.input_count,
            input_staging.total_bytes,
            input_staging.hardlinks,
            input_staging.copies
        );
        println!(
            "staged inputs kept: {}",
            yes_no(input_staging.kept_after_success)
        );
    }
    if let Some(validation) = validation {
        println!(
            "validation: {} ok ({} bytes)",
            validation.level, validation.size_bytes
        );
    } else {
        println!("validation: skipped");
    }
    println!("elapsed: {:.3}s", report.elapsed.as_secs_f64());

    Ok(())
}

fn write_requested_artifacts(
    args: &MergeArgs,
    input_set: &input::InputSet,
    plan: &planner::MergePlan,
    hadd_options: &hadd::HaddOptions,
    command_records: &[telemetry::CommandLogRecord],
    input_staging: Option<telemetry::StagingTelemetry>,
) -> Result<()> {
    if let Some(path) = &args.command_log {
        telemetry::write_command_log(path, command_records)?;
    }

    if let Some(path) = &args.manifest {
        let manifest = telemetry::build_manifest(
            input_set,
            plan,
            hadd_options,
            command_records.to_vec(),
            args.dry_run,
            input_staging,
        );
        telemetry::write_manifest(path, &manifest)?;
    }

    Ok(())
}

fn validate_merge_output_safety(
    output: &Path,
    input_set: &input::InputSet,
    force: bool,
) -> Result<()> {
    if !output.exists() {
        return Ok(());
    }

    let link_metadata = fs::symlink_metadata(output)
        .with_context(|| format!("could not inspect output path: {}", output.display()))?;
    if link_metadata.file_type().is_symlink() {
        bail!(
            "refusing to overwrite symlink output path: {}",
            output.display()
        );
    }

    let metadata = fs::metadata(output)
        .with_context(|| format!("could not inspect output path: {}", output.display()))?;
    if !metadata.is_file() {
        bail!(
            "output path already exists and is not a file: {}",
            output.display()
        );
    }

    let output_canonical = fs::canonicalize(output)
        .with_context(|| format!("could not canonicalize output path: {}", output.display()))?;
    if input_set
        .files
        .iter()
        .any(|input| input.path == output_canonical)
    {
        bail!(
            "output path is also an input file; refusing to overwrite input: {}",
            output.display()
        );
    }

    if !force {
        bail!(
            "output already exists: {}; pass --force to overwrite it",
            output.display()
        );
    }

    Ok(())
}

fn hadd_options_from_plan(plan: &planner::MergePlan, args: &PlanArgs) -> Result<hadd::HaddOptions> {
    hadd_options_from_values(
        plan,
        HaddOptionValues {
            hadd: &args.hadd,
            keep_going: args.keep_going,
            hadd_jobs: args.hadd_jobs,
            max_open_files: args.max_open_files,
            no_trees: args.no_trees,
            only: &args.only,
            skip: &args.skip,
        },
    )
}

fn hadd_options_from_merge(
    plan: &planner::MergePlan,
    args: &MergeArgs,
) -> Result<hadd::HaddOptions> {
    hadd_options_from_values(
        plan,
        HaddOptionValues {
            hadd: &args.hadd,
            keep_going: args.keep_going,
            hadd_jobs: args.hadd_jobs,
            max_open_files: args.max_open_files,
            no_trees: args.no_trees,
            only: &args.only,
            skip: &args.skip,
        },
    )
}

#[derive(Clone, Copy)]
struct HaddOptionValues<'a> {
    hadd: &'a Path,
    keep_going: bool,
    hadd_jobs: Option<usize>,
    max_open_files: Option<usize>,
    no_trees: bool,
    only: &'a [String],
    skip: &'a [String],
}

fn hadd_options_from_values(
    plan: &planner::MergePlan,
    values: HaddOptionValues<'_>,
) -> Result<hadd::HaddOptions> {
    Ok(hadd::HaddOptions {
        executable: values.hadd.to_path_buf(),
        version: None,
        policy: plan.policy,
        hadd_jobs: values.hadd_jobs,
        temp_dir: values.hadd_jobs.map(|_| plan.scratch.clone()),
        keep_going: values.keep_going,
        max_open_files: values.max_open_files,
        no_trees: values.no_trees,
        object_selection: object_selection_from_values(values.only, values.skip, &plan.scratch)?,
    })
}

fn object_selection_from_values(
    only: &[String],
    skip: &[String],
    scratch: &Path,
) -> Result<Option<hadd::ObjectSelection>> {
    if !only.is_empty() && !skip.is_empty() {
        bail!("--only and --skip cannot be used together");
    }

    let (mode, objects) = if !only.is_empty() {
        (hadd::ObjectSelectionMode::OnlyListed, only)
    } else if !skip.is_empty() {
        (hadd::ObjectSelectionMode::SkipListed, skip)
    } else {
        return Ok(None);
    };

    hadd::validate_object_names(objects)?;

    Ok(Some(hadd::ObjectSelection {
        mode,
        objects: objects.to_vec(),
        list_path: hadd::object_list_path(scratch),
    }))
}

fn bench_object_selection_from_values(
    only: &[String],
    skip: &[String],
) -> Result<Option<bench::BenchmarkObjectSelection>> {
    if !only.is_empty() && !skip.is_empty() {
        bail!("--only and --skip cannot be used together");
    }

    let (mode, objects) = if !only.is_empty() {
        (hadd::ObjectSelectionMode::OnlyListed, only)
    } else if !skip.is_empty() {
        (hadd::ObjectSelectionMode::SkipListed, skip)
    } else {
        return Ok(None);
    };

    hadd::validate_object_names(objects)?;

    Ok(Some(bench::BenchmarkObjectSelection {
        mode,
        objects: objects.to_vec(),
    }))
}

fn ensure_requested_hadd_capabilities(options: &hadd::HaddOptions) -> Result<()> {
    if options.object_selection.is_none() {
        return Ok(());
    }

    let capabilities = hadd::detect_capabilities(&options.executable)?;
    if !capabilities.object_lists {
        bail!(
            "`{}` does not appear to support hadd object-list options (-L and -Ltype)",
            options.executable.display()
        );
    }

    Ok(())
}

fn attach_hadd_commands(
    plan: &mut planner::MergePlan,
    options: &hadd::HaddOptions,
    input_staging: Option<&staging::InputStagingPlan>,
) -> Result<()> {
    for stage in &mut plan.stages {
        for job in &mut stage.jobs {
            let command_job = command_job(stage.level, job, input_staging);
            job.hadd_argv = Some(hadd::build_hadd_command(&command_job, options)?.argv);
        }
    }

    Ok(())
}

fn executable_stages(
    plan: &planner::MergePlan,
    options: &hadd::HaddOptions,
    input_staging: Option<&staging::InputStagingPlan>,
) -> Result<Vec<Vec<executor::ExecutableJob>>> {
    plan.stages
        .iter()
        .map(|stage| {
            stage
                .jobs
                .iter()
                .map(|job| {
                    let command_job = command_job(stage.level, job, input_staging);
                    Ok(executor::ExecutableJob {
                        stage_level: stage.level,
                        job_id: job.id,
                        output: job.output.clone(),
                        command: hadd::build_hadd_command(&command_job, options)?,
                    })
                })
                .collect()
        })
        .collect()
}

fn command_job(
    stage_level: usize,
    job: &planner::MergeJob,
    input_staging: Option<&staging::InputStagingPlan>,
) -> planner::MergeJob {
    let mut command_job = job.clone();

    if stage_level == 0
        && let Some(input_staging) = input_staging
    {
        command_job.inputs = command_job
            .inputs
            .iter()
            .map(|input| {
                staged_path(input_staging, input)
                    .unwrap_or(input)
                    .to_path_buf()
            })
            .collect();
    }

    command_job
}

fn staged_path<'a>(
    input_staging: &'a staging::InputStagingPlan,
    original: &Path,
) -> Option<&'a Path> {
    input_staging
        .inputs
        .iter()
        .find(|input| input.original_path == original)
        .map(|input| input.staged_path.as_path())
}

#[derive(Debug)]
struct DoctorReport {
    hadd: ToolCheck,
    root_config: Option<PathBuf>,
    current_dir: PathBuf,
    current_dir_writable: bool,
    temp_dir: PathBuf,
    temp_dir_writable: bool,
    ok: bool,
}

impl DoctorReport {
    fn check(hadd: &Path) -> Self {
        let hadd = ToolCheck::check(hadd);
        let root_config = which::which("root-config").ok();
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let current_dir_writable = directory_writable(&current_dir);
        let temp_dir = env::temp_dir();
        let temp_dir_writable = directory_writable(&temp_dir);
        let ok = hadd.found_path.is_some()
            && hadd.executable
            && hadd.help_invoked
            && current_dir_writable
            && temp_dir_writable;

        Self {
            hadd,
            root_config,
            current_dir,
            current_dir_writable,
            temp_dir,
            temp_dir_writable,
            ok,
        }
    }
}

impl std::fmt::Display for DoctorReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "radd doctor")?;
        writeln!(formatter)?;
        writeln!(
            formatter,
            "hadd: {}",
            self.hadd.found_path.as_ref().map_or_else(
                || "not found".to_string(),
                |path| format!("found at {}", path.display())
            )
        )?;
        writeln!(
            formatter,
            "hadd executable: {}",
            yes_no(self.hadd.executable)
        )?;
        writeln!(formatter, "hadd help: {}", yes_no(self.hadd.help_invoked))?;
        writeln!(
            formatter,
            "root-config: {}",
            self.root_config.as_ref().map_or_else(
                || "not found".to_string(),
                |path| format!("found at {}", path.display())
            )
        )?;
        writeln!(
            formatter,
            "current directory: {}",
            self.current_dir.display()
        )?;
        writeln!(
            formatter,
            "current directory writable: {}",
            yes_no(self.current_dir_writable)
        )?;
        writeln!(
            formatter,
            "temporary directory: {}",
            self.temp_dir.display()
        )?;
        writeln!(
            formatter,
            "temporary directory writable: {}",
            yes_no(self.temp_dir_writable)
        )?;
        writeln!(formatter)?;
        writeln!(
            formatter,
            "status: {}",
            if self.ok { "ok" } else { "failed" }
        )
    }
}

#[derive(Debug)]
struct ToolCheck {
    found_path: Option<PathBuf>,
    executable: bool,
    help_invoked: bool,
}

impl ToolCheck {
    fn check(tool: &Path) -> Self {
        let found_path = resolve_tool(tool);
        let executable = found_path
            .as_deref()
            .is_some_and(|path| path.is_file() && is_executable(path));
        let help_invoked = found_path.as_deref().is_some_and(invoke_help);

        Self {
            found_path,
            executable,
            help_invoked,
        }
    }
}

fn resolve_tool(tool: &Path) -> Option<PathBuf> {
    if tool.components().count() > 1 || tool.is_absolute() {
        tool.exists().then(|| tool.to_path_buf())
    } else {
        which::which(tool).ok()
    }
}

fn invoke_help(path: &Path) -> bool {
    ProcessCommand::new(path)
        .arg("-h")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn directory_writable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };

    if !metadata.is_dir() {
        return false;
    }

    let probe = path.join(format!(
        ".radd-write-test-{}-{}",
        std::process::id(),
        monotonic_suffix()
    ));

    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn monotonic_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::{CacheCommand, Cli, Commands};
    use crate::planner;
    use clap::Parser;

    #[test]
    fn parses_doctor_command() {
        let cli = Cli::parse_from(["radd", "doctor", "--hadd", "/tmp/fake-hadd"]);

        match cli.command {
            Some(Commands::Doctor(args)) => {
                assert_eq!(args.hadd.to_string_lossy(), "/tmp/fake-hadd");
            }
            other => panic!("expected doctor command, got {other:?}"),
        }
    }

    #[test]
    fn parses_plan_command() {
        let cli = Cli::parse_from([
            "radd",
            "plan",
            "out.root",
            "a.root",
            "@inputs.txt",
            "--jobs",
            "4",
            "--chunk-count",
            "3",
            "--fan-in",
            "2",
            "--scratch",
            "/tmp/radd-test",
            "--policy",
            "balanced",
            "--commands",
            "--hadd",
            "/opt/root/bin/hadd",
            "--keep-going",
            "--hadd-jobs",
            "2",
            "--max-open-files",
            "64",
            "--no-trees",
            "--only",
            "DecayTree",
            "--only",
            "Events",
            "--json",
        ]);

        match cli.command {
            Some(Commands::Plan(args)) => {
                assert_eq!(args.output.to_string_lossy(), "out.root");
                assert_eq!(args.inputs, ["a.root", "@inputs.txt"]);
                assert_eq!(args.jobs, Some(4));
                assert_eq!(args.chunk_count, Some(3));
                assert_eq!(args.fan_in, 2);
                assert_eq!(
                    args.scratch.expect("scratch").to_string_lossy(),
                    "/tmp/radd-test"
                );
                assert_eq!(args.policy, planner::MergePolicy::Balanced);
                assert!(args.commands);
                assert_eq!(args.hadd.to_string_lossy(), "/opt/root/bin/hadd");
                assert!(args.keep_going);
                assert_eq!(args.hadd_jobs, Some(2));
                assert_eq!(args.max_open_files, Some(64));
                assert!(args.no_trees);
                assert_eq!(args.only, ["DecayTree", "Events"]);
                assert!(args.skip.is_empty());
                assert!(args.json);
            }
            other => panic!("expected plan command, got {other:?}"),
        }
    }

    #[test]
    fn parses_cache_list_command() {
        let cli = Cli::parse_from(["radd", "cache", "list"]);

        match cli.command {
            Some(Commands::Cache(args)) => match args.command {
                CacheCommand::List => {}
                other @ CacheCommand::Clean => {
                    panic!("expected cache list command, got {other:?}");
                }
            },
            other => panic!("expected cache command, got {other:?}"),
        }
    }

    #[test]
    fn parses_validate_command() {
        let cli = Cli::parse_from(["radd", "validate", "out.root"]);

        match cli.command {
            Some(Commands::Validate(args)) => {
                assert_eq!(args.output.to_string_lossy(), "out.root");
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_inspect_root_metadata_options() {
        let cli = Cli::parse_from([
            "radd",
            "inspect",
            "a.root",
            "--root-metadata",
            "--root",
            "/opt/root/bin/root",
        ]);

        match cli.command {
            Some(Commands::Inspect(args)) => {
                assert_eq!(args.inputs, ["a.root"]);
                assert!(args.root_metadata);
                assert_eq!(args.root.to_string_lossy(), "/opt/root/bin/root");
            }
            other => panic!("expected inspect command, got {other:?}"),
        }
    }

    #[test]
    fn parses_merge_command() {
        let cli = Cli::parse_from([
            "radd",
            "merge",
            "out.root",
            "a.root",
            "--jobs",
            "3",
            "--chunk-count",
            "2",
            "--fan-in",
            "2",
            "--scratch",
            "/tmp/radd-merge",
            "--policy",
            "reproducible",
            "--dry-run",
            "--force",
            "--json",
            "--manifest",
            "radd-manifest.json",
            "--command-log",
            "radd-commands.jsonl",
            "--cache",
            "--no-validate",
            "--stage-inputs",
            "--keep-staged-inputs",
            "--hadd",
            "/opt/root/bin/hadd",
            "--keep-going",
            "--hadd-jobs",
            "4",
            "--max-open-files",
            "32",
            "--no-trees",
            "--skip",
            "BadTree",
        ]);

        match cli.command {
            Some(Commands::Merge(args)) => {
                assert_eq!(args.output.to_string_lossy(), "out.root");
                assert_eq!(args.inputs, ["a.root"]);
                assert_eq!(args.jobs, Some(3));
                assert_eq!(args.chunk_count, Some(2));
                assert_eq!(args.fan_in, 2);
                assert_eq!(
                    args.scratch.expect("scratch").to_string_lossy(),
                    "/tmp/radd-merge"
                );
                assert_eq!(args.policy, planner::MergePolicy::Reproducible);
                assert!(args.dry_run);
                assert!(args.force);
                assert!(args.json);
                assert_eq!(
                    args.manifest.expect("manifest").to_string_lossy(),
                    "radd-manifest.json"
                );
                assert_eq!(
                    args.command_log.expect("command log").to_string_lossy(),
                    "radd-commands.jsonl"
                );
                assert!(args.cache);
                assert!(args.no_validate);
                assert!(args.stage_inputs);
                assert!(args.keep_staged_inputs);
                assert_eq!(args.hadd.to_string_lossy(), "/opt/root/bin/hadd");
                assert!(args.keep_going);
                assert_eq!(args.hadd_jobs, Some(4));
                assert_eq!(args.max_open_files, Some(32));
                assert!(args.no_trees);
                assert!(args.only.is_empty());
                assert_eq!(args.skip, ["BadTree"]);
            }
            other => panic!("expected merge command, got {other:?}"),
        }
    }
}
