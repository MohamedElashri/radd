//! Output validation.

use std::{
    fmt::{self, Write as _},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub path: PathBuf,
    pub level: ValidationLevel,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationLevel {
    Basic,
}

impl fmt::Display for ValidationLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Basic => "basic",
        })
    }
}

pub fn validate_basic(path: &Path) -> Result<ValidationReport> {
    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "validation failed: output does not exist: {}",
            path.display()
        )
    })?;

    if !metadata.is_file() {
        bail!(
            "validation failed: output is not a file: {}",
            path.display()
        );
    }

    let size_bytes = metadata.len();
    if size_bytes == 0 {
        bail!("validation failed: output is empty: {}", path.display());
    }

    Ok(ValidationReport {
        path: path.to_path_buf(),
        level: ValidationLevel::Basic,
        size_bytes,
    })
}

#[must_use]
pub fn format_validation_report(report: &ValidationReport) -> String {
    let mut output = String::new();

    output.push_str("radd validate\n\n");
    writeln!(&mut output, "output: {}", report.path.display()).expect("write to string");
    writeln!(&mut output, "level: {}", report.level).expect("write to string");
    writeln!(&mut output, "size: {} bytes", report.size_bytes).expect("write to string");
    output.push_str("status: ok\n");

    output
}

#[cfg(test)]
mod tests {
    use std::fs;

    use assert_fs::TempDir;

    use super::validate_basic;

    #[test]
    fn basic_validation_accepts_nonempty_file() {
        let temp = TempDir::new().expect("temp dir");
        let output = temp.path().join("out.root");
        fs::write(&output, b"root-ish").expect("write output");

        let report = validate_basic(&output).expect("validation");

        assert_eq!(report.path, output);
        assert_eq!(report.size_bytes, 8);
    }

    #[test]
    fn basic_validation_rejects_missing_file() {
        let temp = TempDir::new().expect("temp dir");
        let output = temp.path().join("missing.root");

        let error = validate_basic(&output).expect_err("missing output should fail");

        assert!(error.to_string().contains("output does not exist"));
    }

    #[test]
    fn basic_validation_rejects_empty_file() {
        let temp = TempDir::new().expect("temp dir");
        let output = temp.path().join("empty.root");
        fs::write(&output, b"").expect("write empty output");

        let error = validate_basic(&output).expect_err("empty output should fail");

        assert!(error.to_string().contains("output is empty"));
    }
}
