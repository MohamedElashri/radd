//! Input resolution.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};

/// One resolved input file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified_time: Option<SystemTime>,
}

/// Fully resolved input collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSet {
    pub files: Vec<InputFile>,
    pub total_size_bytes: u64,
}

/// Resolve direct file arguments and `@manifest` arguments into an input set.
pub fn resolve_inputs(arguments: &[String]) -> Result<InputSet> {
    let cwd = std::env::current_dir().context("could not determine current directory")?;
    resolve_inputs_from(arguments, &cwd)
}

pub(crate) fn resolve_inputs_from(arguments: &[String], cwd: &Path) -> Result<InputSet> {
    let entries = expand_arguments(arguments, cwd)?;

    if entries.is_empty() {
        bail!("no input files were provided");
    }

    let mut files = Vec::with_capacity(entries.len());
    let mut seen = Vec::<(PathBuf, PathBuf)>::new();
    let mut total_size_bytes = 0_u64;

    for entry in entries {
        let metadata = fs::metadata(&entry.path)
            .with_context(|| format!("input file does not exist: {}", entry.path.display()))?;

        if !metadata.is_file() {
            bail!("input path is not a file: {}", entry.path.display());
        }

        let canonical_path = fs::canonicalize(&entry.path)
            .with_context(|| format!("could not canonicalize input: {}", entry.path.display()))?;

        if let Some((_, first_display_path)) = seen
            .iter()
            .find(|(seen_path, _)| seen_path == &canonical_path)
        {
            bail!(
                "duplicate input file: {} was already listed as {}",
                entry.path.display(),
                first_display_path.display()
            );
        }

        seen.push((canonical_path.clone(), entry.path.clone()));
        total_size_bytes = total_size_bytes.saturating_add(metadata.len());
        files.push(InputFile {
            path: canonical_path,
            size_bytes: metadata.len(),
            modified_time: metadata.modified().ok(),
        });
    }

    Ok(InputSet {
        files,
        total_size_bytes,
    })
}

#[derive(Debug)]
struct InputEntry {
    path: PathBuf,
}

fn expand_arguments(arguments: &[String], cwd: &Path) -> Result<Vec<InputEntry>> {
    let mut entries = Vec::new();

    for argument in arguments {
        if let Some(manifest) = argument.strip_prefix('@') {
            if manifest.is_empty() {
                bail!("manifest argument `@` is missing a path");
            }

            entries.extend(read_manifest(&resolve_relative_to_cwd(manifest, cwd), cwd)?);
        } else {
            entries.push(InputEntry {
                path: resolve_relative_to_cwd(argument, cwd),
            });
        }
    }

    Ok(entries)
}

fn read_manifest(manifest: &Path, cwd: &Path) -> Result<Vec<InputEntry>> {
    let contents = fs::read_to_string(manifest)
        .with_context(|| format!("could not read input manifest: {}", manifest.display()))?;

    let mut entries = Vec::new();

    for (index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('@') {
            bail!(
                "nested manifests are not supported in {} at line {}",
                manifest.display(),
                index + 1
            );
        }

        entries.push(InputEntry {
            path: resolve_relative_to_cwd(trimmed, cwd),
        });
    }

    Ok(entries)
}

fn resolve_relative_to_cwd(path: impl AsRef<Path>, cwd: &Path) -> PathBuf {
    let path = path.as_ref();

    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use assert_fs::TempDir;

    use super::resolve_inputs_from;

    #[test]
    fn resolves_direct_files_and_collects_sizes() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(temp.path().join("a.root"), b"abcd").expect("write a");
        fs::write(temp.path().join("b.root"), b"xy").expect("write b");

        let input_set =
            resolve_inputs_from(&["a.root".to_string(), "b.root".to_string()], temp.path())
                .expect("resolve inputs");

        assert_eq!(input_set.files.len(), 2);
        assert_eq!(input_set.total_size_bytes, 6);
        assert!(input_set.files[0].path.is_absolute());
    }

    #[test]
    fn manifest_ignores_comments_and_blank_lines() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(temp.path().join("a.root"), b"a").expect("write a");
        fs::write(temp.path().join("b.root"), b"bb").expect("write b");
        fs::write(
            temp.path().join("inputs.txt"),
            "# comment\n\n a.root \n\n# another\nb.root\n",
        )
        .expect("write manifest");

        let input_set =
            resolve_inputs_from(&["@inputs.txt".to_string()], temp.path()).expect("resolve inputs");

        assert_eq!(input_set.files.len(), 2);
        assert_eq!(input_set.total_size_bytes, 3);
    }

    #[test]
    fn missing_files_are_reported() {
        let temp = TempDir::new().expect("temp dir");
        let error = resolve_inputs_from(&["missing.root".to_string()], temp.path())
            .expect_err("missing input should fail");

        assert!(
            error.to_string().contains("input file does not exist"),
            "{error:?}"
        );
    }

    #[test]
    fn duplicate_files_are_reported() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(temp.path().join("a.root"), b"a").expect("write a");

        let error =
            resolve_inputs_from(&["a.root".to_string(), "./a.root".to_string()], temp.path())
                .expect_err("duplicate input should fail");

        assert!(error.to_string().contains("duplicate input file"));
    }
}
