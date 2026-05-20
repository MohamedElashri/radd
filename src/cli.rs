//! Command-line interface.

use std::{
    env,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::{input, inspect};

/// A safe Rust frontend for orchestrating ROOT hadd merges.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Increase diagnostic output.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Reduce diagnostic output.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Check local radd, ROOT, and filesystem prerequisites.
    Doctor(DoctorArgs),

    /// Print the merge plan without running hadd.
    Plan(PlanArgs),

    /// Merge ROOT files by orchestrating hadd.
    Merge(MergeArgs),

    /// Inspect input files or manifests.
    Inspect(InputListArgs),

    /// Benchmark candidate merge settings.
    Bench(InputListArgs),

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
pub struct PlanArgs {
    /// Output ROOT file path.
    pub output: PathBuf,

    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Output ROOT file path.
    pub output: PathBuf,

    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct InputListArgs {
    /// Input ROOT files or @manifest paths.
    #[arg(required = true)]
    pub inputs: Vec<String>,
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
    match cli.command {
        Commands::Doctor(args) => run_doctor(&args),
        Commands::Plan(args) => run_plan(&args),
        Commands::Merge(_) => not_implemented("merge", "phase 5"),
        Commands::Inspect(args) => run_inspect(&args),
        Commands::Bench(_) => not_implemented("bench", "phase 9"),
        Commands::Cache(CacheArgs { command }) => match command {
            CacheCommand::List => not_implemented("cache list", "phase 7"),
            CacheCommand::Clean => not_implemented("cache clean", "phase 7"),
        },
    }
}

fn not_implemented(command: &str, phase: &str) -> Result<()> {
    bail!("`radd {command}` is not implemented yet; it is planned for {phase}")
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
    print!("{}", inspect::format_input_summary(&input_set));
    Ok(())
}

fn run_plan(args: &PlanArgs) -> Result<()> {
    let input_set = input::resolve_inputs(&args.inputs)?;
    println!("radd plan");
    println!();
    println!("output: {}", args.output.display());
    print!("{}", inspect::format_input_summary(&input_set));
    println!("planning: merge topology is planned for phase 3");
    Ok(())
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
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
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
    use clap::Parser;

    #[test]
    fn parses_doctor_command() {
        let cli = Cli::parse_from(["radd", "doctor", "--hadd", "/tmp/fake-hadd"]);

        match cli.command {
            Commands::Doctor(args) => assert_eq!(args.hadd.to_string_lossy(), "/tmp/fake-hadd"),
            other => panic!("expected doctor command, got {other:?}"),
        }
    }

    #[test]
    fn parses_plan_command() {
        let cli = Cli::parse_from(["radd", "plan", "out.root", "a.root", "@inputs.txt"]);

        match cli.command {
            Commands::Plan(args) => {
                assert_eq!(args.output.to_string_lossy(), "out.root");
                assert_eq!(args.inputs, ["a.root", "@inputs.txt"]);
            }
            other => panic!("expected plan command, got {other:?}"),
        }
    }

    #[test]
    fn parses_cache_list_command() {
        let cli = Cli::parse_from(["radd", "cache", "list"]);

        match cli.command {
            Commands::Cache(args) => match args.command {
                CacheCommand::List => {}
                other @ CacheCommand::Clean => {
                    panic!("expected cache list command, got {other:?}");
                }
            },
            other => panic!("expected cache command, got {other:?}"),
        }
    }
}
