//! Scratch directory and input staging preparation.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{input::InputSet, planner::MergePlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputStagingPlan {
    pub staging_dir: PathBuf,
    pub inputs: Vec<StagedInput>,
    pub total_bytes: u64,
    pub keep_after_success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedInput {
    pub original_path: PathBuf,
    pub staged_path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputStagingReport {
    pub plan: InputStagingPlan,
    pub hardlinks: usize,
    pub copies: usize,
}

pub fn prepare_scratch(plan: &MergePlan) -> Result<()> {
    let needs_scratch = plan
        .stages
        .iter()
        .flat_map(|stage| &stage.jobs)
        .any(|job| job.output != plan.output);

    if needs_scratch {
        fs::create_dir_all(&plan.scratch).with_context(|| {
            format!(
                "could not create scratch directory: {}",
                plan.scratch.display()
            )
        })?;
    }

    for output in plan
        .stages
        .iter()
        .flat_map(|stage| &stage.jobs)
        .filter(|job| job.output != plan.output)
        .map(|job| &job.output)
    {
        if let Some(parent) = output.parent() {
            create_parent(parent)?;
        }
    }

    Ok(())
}

pub fn plan_input_staging(
    input_set: &InputSet,
    scratch: &Path,
    keep_after_success: bool,
) -> InputStagingPlan {
    let staging_dir = scratch.join("radd-staged-inputs");
    let inputs = input_set
        .files
        .iter()
        .enumerate()
        .map(|(index, input)| StagedInput {
            original_path: input.path.clone(),
            staged_path: staging_dir.join(staged_file_name(index, &input.path)),
            size_bytes: input.size_bytes,
        })
        .collect::<Vec<_>>();

    InputStagingPlan {
        staging_dir,
        inputs,
        total_bytes: input_set.total_size_bytes,
        keep_after_success,
    }
}

pub fn prepare_input_staging(plan: &InputStagingPlan) -> Result<InputStagingReport> {
    fs::create_dir_all(&plan.staging_dir).with_context(|| {
        format!(
            "could not create input staging directory: {}",
            plan.staging_dir.display()
        )
    })?;

    for input in &plan.inputs {
        if input.staged_path.exists() {
            bail!(
                "refusing to overwrite staged input path: {}",
                input.staged_path.display()
            );
        }
    }

    let mut hardlinks = 0;
    let mut copies = 0;
    let mut created = Vec::new();

    for input in &plan.inputs {
        match stage_one_input(input) {
            Ok(StageMethod::Hardlink) => {
                hardlinks += 1;
                created.push(input.staged_path.clone());
            }
            Ok(StageMethod::Copy) => {
                copies += 1;
                created.push(input.staged_path.clone());
            }
            Err(error) => {
                cleanup_created_files(&created);
                return Err(error);
            }
        }
    }

    Ok(InputStagingReport {
        plan: plan.clone(),
        hardlinks,
        copies,
    })
}

pub fn cleanup_staged_inputs(plan: &InputStagingPlan) -> Result<()> {
    let mut first_error = None;

    for input in &plan.inputs {
        if let Err(error) = fs::remove_file(&input.staged_path)
            && error.kind() != std::io::ErrorKind::NotFound
            && first_error.is_none()
        {
            first_error = Some(anyhow::Error::new(error).context(format!(
                "could not remove staged input: {}",
                input.staged_path.display()
            )));
        }
    }

    if let Err(error) = fs::remove_dir(&plan.staging_dir)
        && error.kind() != std::io::ErrorKind::NotFound
        && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
        && first_error.is_none()
    {
        first_error = Some(anyhow::Error::new(error).context(format!(
            "could not remove input staging directory: {}",
            plan.staging_dir.display()
        )));
    }

    if let Some(scratch) = plan.staging_dir.parent()
        && let Err(error) = fs::remove_dir(scratch)
        && error.kind() != std::io::ErrorKind::NotFound
        && error.kind() != std::io::ErrorKind::DirectoryNotEmpty
        && first_error.is_none()
    {
        first_error = Some(anyhow::Error::new(error).context(format!(
            "could not remove scratch directory: {}",
            scratch.display()
        )));
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageMethod {
    Hardlink,
    Copy,
}

fn stage_one_input(input: &StagedInput) -> Result<StageMethod> {
    match fs::hard_link(&input.original_path, &input.staged_path) {
        Ok(()) => {
            if let Err(error) = verify_staged_size(input) {
                let _ = fs::remove_file(&input.staged_path);
                return Err(error);
            }
            Ok(StageMethod::Hardlink)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(error).with_context(|| {
                format!(
                    "staged input already exists: {}",
                    input.staged_path.display()
                )
            })
        }
        Err(_) => {
            copy_without_overwrite(&input.original_path, &input.staged_path).with_context(
                || {
                    format!(
                        "could not stage input {} to {}",
                        input.original_path.display(),
                        input.staged_path.display()
                    )
                },
            )?;
            if let Err(error) = verify_staged_size(input) {
                let _ = fs::remove_file(&input.staged_path);
                return Err(error);
            }
            Ok(StageMethod::Copy)
        }
    }
}

fn copy_without_overwrite(source: &Path, destination: &Path) -> Result<()> {
    let mut input = fs::File::open(source)
        .with_context(|| format!("could not open input for staging: {}", source.display()))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .with_context(|| {
            format!(
                "could not create staged input without overwrite: {}",
                destination.display()
            )
        })?;

    io::copy(&mut input, &mut output)
        .with_context(|| format!("could not copy staged input: {}", destination.display()))?;

    Ok(())
}

fn verify_staged_size(input: &StagedInput) -> Result<()> {
    let metadata = fs::metadata(&input.staged_path).with_context(|| {
        format!(
            "could not stat staged input: {}",
            input.staged_path.display()
        )
    })?;

    if !metadata.is_file() {
        bail!(
            "staged input is not a file: {}",
            input.staged_path.display()
        );
    }

    if metadata.len() != input.size_bytes {
        bail!(
            "staged input size mismatch for {}: expected {} bytes, got {} bytes",
            input.staged_path.display(),
            input.size_bytes,
            metadata.len()
        );
    }

    Ok(())
}

fn cleanup_created_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn staged_file_name(index: usize, original_path: &Path) -> String {
    let file_name = original_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "input.root".into());

    format!("{index:06}-{file_name}")
}

fn create_parent(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| {
        format!(
            "could not create scratch output parent directory: {}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use assert_fs::TempDir;

    use super::{cleanup_staged_inputs, plan_input_staging, prepare_input_staging};
    use crate::input::{InputFile, InputSet};

    #[test]
    fn staging_path_mapping_is_deterministic() {
        let input_set = input_set(&[("/data/b.root", 2), ("/data/a.root", 3)]);

        let first = plan_input_staging(&input_set, &PathBuf::from("/scratch/radd"), false);
        let second = plan_input_staging(&input_set, &PathBuf::from("/scratch/radd"), false);

        assert_eq!(first, second);
        assert_eq!(
            first.inputs[0].staged_path,
            PathBuf::from("/scratch/radd/radd-staged-inputs/000000-b.root")
        );
        assert_eq!(
            first.inputs[1].staged_path,
            PathBuf::from("/scratch/radd/radd-staged-inputs/000001-a.root")
        );
    }

    #[test]
    fn staging_creates_verified_inputs_and_cleanup_removes_them() {
        let temp = TempDir::new().expect("temp dir");
        let input = temp.path().join("a.root");
        fs::write(&input, b"abc").expect("write input");
        let input_set = InputSet {
            files: vec![InputFile {
                path: input,
                size_bytes: 3,
                modified_time: None,
            }],
            total_size_bytes: 3,
        };
        let plan = plan_input_staging(&input_set, &temp.path().join("scratch"), false);

        let report = prepare_input_staging(&plan).expect("stage inputs");
        assert_eq!(report.hardlinks + report.copies, 1);
        assert!(plan.inputs[0].staged_path.is_file());

        cleanup_staged_inputs(&plan).expect("cleanup staged inputs");
        assert!(!plan.staging_dir.exists());
    }

    fn input_set(files: &[(&str, u64)]) -> InputSet {
        InputSet {
            files: files
                .iter()
                .map(|(path, size_bytes)| InputFile {
                    path: PathBuf::from(path),
                    size_bytes: *size_bytes,
                    modified_time: None,
                })
                .collect(),
            total_size_bytes: files.iter().map(|(_, size_bytes)| *size_bytes).sum(),
        }
    }
}
