//! `hadd` subprocess command construction.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::planner::{MergeJob, MergePolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HaddOptions {
    pub executable: PathBuf,
    pub version: Option<String>,
    pub policy: MergePolicy,
    pub hadd_jobs: Option<usize>,
    pub temp_dir: Option<PathBuf>,
    pub keep_going: bool,
    pub max_open_files: Option<usize>,
    pub no_trees: bool,
    pub object_selection: Option<ObjectSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HaddCommand {
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectSelectionMode {
    OnlyListed,
    SkipListed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSelection {
    pub mode: ObjectSelectionMode,
    pub objects: Vec<String>,
    pub list_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HaddCapabilities {
    pub no_trees: bool,
    pub object_lists: bool,
}

pub fn build_hadd_command(job: &MergeJob, options: &HaddOptions) -> Result<HaddCommand> {
    validate_job(job)?;
    validate_hadd_options(options)?;

    let mut argv = Vec::new();
    argv.push(path_to_arg(&options.executable));

    argv.push("-f".to_string());
    if preserve_input_compression(options.policy) {
        argv.push("-fk".to_string());
    }

    if let Some(hadd_jobs) = options.hadd_jobs {
        argv.push("-j".to_string());
        argv.push(hadd_jobs.to_string());
    }

    if let Some(temp_dir) = &options.temp_dir {
        argv.push("-d".to_string());
        argv.push(path_to_arg(temp_dir));
    }

    if options.keep_going {
        argv.push("-k".to_string());
    }

    if let Some(max_open_files) = options.max_open_files {
        argv.push("-n".to_string());
        argv.push(max_open_files.to_string());
    }

    if options.no_trees {
        argv.push("-T".to_string());
    }

    if let Some(selection) = &options.object_selection {
        argv.push("-L".to_string());
        argv.push(path_to_arg(&selection.list_path));
        argv.push("-Ltype".to_string());
        argv.push(selection.mode.as_hadd_ltype().to_string());
    }

    argv.push(path_to_arg(&job.output));
    argv.extend(job.inputs.iter().map(|input| path_to_arg(input)));

    Ok(HaddCommand { argv })
}

pub fn detect_capabilities(executable: &Path) -> Result<HaddCapabilities> {
    let output = Command::new(executable)
        .arg("-h")
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!(
                "could not run `{}` to detect hadd capabilities",
                executable.display()
            )
        })?;

    if !output.status.success() {
        bail!(
            "could not detect hadd capabilities because `{}` -h exited with status {}",
            executable.display(),
            output.status
        );
    }

    let mut help = String::from_utf8_lossy(&output.stdout).into_owned();
    help.push_str(&String::from_utf8_lossy(&output.stderr));

    Ok(HaddCapabilities {
        no_trees: help.contains("-T"),
        object_lists: help.contains("-Ltype")
            && help.contains("OnlyListed")
            && help.contains("SkipListed"),
    })
}

#[must_use]
pub fn detect_version(executable: &Path) -> Option<String> {
    detect_direct_version(executable).or_else(|| detect_root_config_version(executable))
}

#[must_use]
pub fn object_list_path(scratch: &Path) -> PathBuf {
    scratch.join("radd-object-selection.txt")
}

pub fn write_object_list_file(selection: &ObjectSelection) -> Result<()> {
    validate_object_names(&selection.objects)?;

    if let Some(parent) = selection.list_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "could not create object-selection directory: {}",
                parent.display()
            )
        })?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&selection.list_path)
        .with_context(|| {
            format!(
                "could not create object-selection list file without overwriting: {}",
                selection.list_path.display()
            )
        })?;

    for object in &selection.objects {
        writeln!(file, "{object}").with_context(|| {
            format!(
                "could not write object-selection list file: {}",
                selection.list_path.display()
            )
        })?;
    }

    Ok(())
}

pub fn cleanup_object_list_file(selection: Option<&ObjectSelection>) -> Result<()> {
    let Some(selection) = selection else {
        return Ok(());
    };

    if let Err(error) = fs::remove_file(&selection.list_path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error).with_context(|| {
            format!(
                "could not remove object-selection list file: {}",
                selection.list_path.display()
            )
        });
    }

    if let Some(parent) = selection.list_path.parent()
        && let Err(error) = fs::remove_dir(parent)
        && error.kind() != std::io::ErrorKind::NotFound
        && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
    {
        return Err(error).with_context(|| {
            format!(
                "could not remove object-selection directory: {}",
                parent.display()
            )
        });
    }

    Ok(())
}

pub fn validate_hadd_options(options: &HaddOptions) -> Result<()> {
    if options.executable.as_os_str().is_empty() {
        bail!("hadd executable path must not be empty");
    }

    if let Some(hadd_jobs) = options.hadd_jobs
        && hadd_jobs == 0
    {
        bail!("--hadd-jobs must be greater than zero");
    }

    if let Some(max_open_files) = options.max_open_files
        && max_open_files == 0
    {
        bail!("--max-open-files must be greater than zero");
    }

    if let Some(selection) = &options.object_selection {
        if selection.list_path.as_os_str().is_empty() {
            bail!("object-selection list path must not be empty");
        }
        validate_object_names(&selection.objects)?;
    }

    Ok(())
}

#[must_use]
pub fn format_argv_for_display(argv: &[String]) -> String {
    argv.iter()
        .map(|argument| quote_arg(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_job(job: &MergeJob) -> Result<()> {
    if job.inputs.is_empty() {
        bail!(
            "cannot build hadd command for job {} with no inputs",
            job.id
        );
    }

    Ok(())
}

fn preserve_input_compression(policy: MergePolicy) -> bool {
    match policy {
        MergePolicy::Fastest
        | MergePolicy::Balanced
        | MergePolicy::Smallest
        | MergePolicy::Reproducible => true,
    }
}

pub fn validate_object_names(objects: &[String]) -> Result<()> {
    if objects.is_empty() {
        bail!("object selection requires at least one object name");
    }

    for object in objects {
        if object.is_empty() {
            bail!("object-selection names must not be empty");
        }

        if object.chars().any(char::is_whitespace) {
            bail!("object-selection name `{object}` must not contain whitespace");
        }

        if object.contains('/') {
            bail!("object-selection name `{object}` must not contain `/`");
        }

        if object.chars().any(char::is_control) {
            bail!("object-selection name `{object}` must not contain control characters");
        }
    }

    Ok(())
}

impl ObjectSelectionMode {
    #[must_use]
    pub fn as_hadd_ltype(self) -> &'static str {
        match self {
            Self::OnlyListed => "OnlyListed",
            Self::SkipListed => "SkipListed",
        }
    }
}

fn path_to_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn detect_direct_version(executable: &Path) -> Option<String> {
    let output = Command::new(executable)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| first_output_line(&output.stdout, &output.stderr))
        .flatten()
}

fn detect_root_config_version(executable: &Path) -> Option<String> {
    let root_config = root_config_candidate(executable);
    let output = Command::new(root_config)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;

    let version = output
        .status
        .success()
        .then(|| first_output_line(&output.stdout, &output.stderr))
        .flatten()?;

    Some(format!("ROOT {version}"))
}

fn root_config_candidate(executable: &Path) -> PathBuf {
    if let Some(parent) = executable.parent()
        && !parent.as_os_str().is_empty()
    {
        let sibling = parent.join("root-config");
        if sibling.is_file() {
            return sibling;
        }
    }

    PathBuf::from("root-config")
}

fn first_output_line(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let mut output = String::from_utf8_lossy(stdout).into_owned();
    output.push_str(&String::from_utf8_lossy(stderr));

    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn quote_arg(argument: &str) -> String {
    if argument.is_empty() {
        return "''".to_string();
    }

    if argument
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_+-=.,/:@%".contains(character))
    {
        return argument.to_string();
    }

    format!("'{}'", argument.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use assert_fs::TempDir;

    use super::{
        HaddOptions, ObjectSelection, ObjectSelectionMode, build_hadd_command, detect_version,
        format_argv_for_display, write_object_list_file,
    };
    use crate::planner::{MergeJob, MergePolicy};

    #[test]
    fn default_fastest_command_uses_force_and_keep_compression() {
        let command = build_hadd_command(&job(), &options()).expect("command");

        assert_eq!(
            command.argv,
            [
                "hadd",
                "-f",
                "-fk",
                "out.root",
                "input-a.root",
                "input-b.root"
            ]
        );
    }

    #[test]
    fn keep_going_adds_hadd_k_flag() {
        let command = build_hadd_command(
            &job(),
            &HaddOptions {
                keep_going: true,
                ..options()
            },
        )
        .expect("command");

        assert!(command.argv.iter().any(|argument| argument == "-k"));
    }

    #[test]
    fn no_trees_adds_hadd_t_flag() {
        let command = build_hadd_command(
            &job(),
            &HaddOptions {
                no_trees: true,
                ..options()
            },
        )
        .expect("command");

        assert!(command.argv.iter().any(|argument| argument == "-T"));
    }

    #[test]
    fn object_selection_adds_list_flags_before_output() {
        let command = build_hadd_command(
            &job(),
            &HaddOptions {
                object_selection: Some(ObjectSelection {
                    mode: ObjectSelectionMode::OnlyListed,
                    objects: vec!["DecayTree".to_string()],
                    list_path: PathBuf::from("scratch/objects.txt"),
                }),
                ..options()
            },
        )
        .expect("command");

        assert_contains_pair(&command.argv, "-L", "scratch/objects.txt");
        assert_contains_pair(&command.argv, "-Ltype", "OnlyListed");

        let output_position = command
            .argv
            .iter()
            .position(|argument| argument == "out.root")
            .expect("output position");
        let list_type_position = command
            .argv
            .iter()
            .position(|argument| argument == "-Ltype")
            .expect("Ltype position");
        assert!(list_type_position < output_position);
    }

    #[test]
    fn object_selection_rejects_hadd_unsupported_names() {
        let error = build_hadd_command(
            &job(),
            &HaddOptions {
                object_selection: Some(ObjectSelection {
                    mode: ObjectSelectionMode::SkipListed,
                    objects: vec!["dir/tree".to_string()],
                    list_path: PathBuf::from("scratch/objects.txt"),
                }),
                ..options()
            },
        )
        .expect_err("slash should fail");

        assert!(error.to_string().contains("must not contain `/`"));
    }

    #[test]
    fn writes_object_list_without_overwriting() {
        let temp = TempDir::new().expect("temp dir");
        let selection = ObjectSelection {
            mode: ObjectSelectionMode::OnlyListed,
            objects: vec!["DecayTree".to_string(), "Events".to_string()],
            list_path: temp.path().join("objects.txt"),
        };

        write_object_list_file(&selection).expect("write list");

        let contents = std::fs::read_to_string(&selection.list_path).expect("read list");
        assert_eq!(contents, "DecayTree\nEvents\n");
        assert!(write_object_list_file(&selection).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn detects_hadd_version_from_direct_version_output() {
        use std::{fs, os::unix::fs::PermissionsExt};

        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("hadd");
        fs::write(
            &path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'hadd fake 9.9'; exit 0; fi\nexit 1\n",
        )
        .expect("write fake hadd");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("chmod");

        assert_eq!(detect_version(&path).as_deref(), Some("hadd fake 9.9"));
    }

    #[test]
    fn max_open_files_adds_hadd_n_flag() {
        let command = build_hadd_command(
            &job(),
            &HaddOptions {
                max_open_files: Some(64),
                ..options()
            },
        )
        .expect("command");

        assert_contains_pair(&command.argv, "-n", "64");
    }

    #[test]
    fn hadd_jobs_and_temp_dir_add_j_and_d_flags() {
        let command = build_hadd_command(
            &job(),
            &HaddOptions {
                hadd_jobs: Some(4),
                temp_dir: Some(PathBuf::from("/tmp/radd scratch")),
                ..options()
            },
        )
        .expect("command");

        assert_contains_pair(&command.argv, "-j", "4");
        assert_contains_pair(&command.argv, "-d", "/tmp/radd scratch");
    }

    #[test]
    fn argv_preserves_paths_with_spaces_as_single_arguments() {
        let command = build_hadd_command(
            &MergeJob {
                id: 7,
                output: PathBuf::from("my output.root"),
                inputs: vec![PathBuf::from("first input.root")],
                input_size_bytes: 10,
                hadd_argv: None,
            },
            &options(),
        )
        .expect("command");

        assert_eq!(command.argv[3], "my output.root");
        assert_eq!(command.argv[4], "first input.root");
        assert_eq!(
            format_argv_for_display(&command.argv),
            "hadd -f -fk 'my output.root' 'first input.root'"
        );
    }

    fn assert_contains_pair(argv: &[String], flag: &str, value: &str) {
        let flag_position = argv
            .iter()
            .position(|argument| argument == flag)
            .unwrap_or_else(|| panic!("missing flag {flag} in {argv:?}"));
        assert_eq!(argv.get(flag_position + 1).map(String::as_str), Some(value));
    }

    fn options() -> HaddOptions {
        HaddOptions {
            executable: PathBuf::from("hadd"),
            version: None,
            policy: MergePolicy::Fastest,
            hadd_jobs: None,
            temp_dir: None,
            keep_going: false,
            max_open_files: None,
            no_trees: false,
            object_selection: None,
        }
    }

    fn job() -> MergeJob {
        MergeJob {
            id: 0,
            output: PathBuf::from("out.root"),
            inputs: vec![PathBuf::from("input-a.root"), PathBuf::from("input-b.root")],
            input_size_bytes: 20,
            hadd_argv: None,
        }
    }
}
