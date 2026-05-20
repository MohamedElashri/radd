//! Input inspection.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::input::InputSet;

const ROOT_METADATA_BEGIN: &str = "__RADD_ROOT_METADATA_BEGIN__";
const ROOT_METADATA_END: &str = "__RADD_ROOT_METADATA_END__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectOptions {
    pub root_metadata: bool,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectReport {
    pub input_count: usize,
    pub total_size_bytes: u64,
    pub root_metadata_enabled: bool,
    pub root_metadata: Vec<RootFileMetadata>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootFileMetadata {
    pub path: PathBuf,
    pub compression_algorithm: Option<i32>,
    pub compression_level: Option<i32>,
    pub compression_settings: Option<i32>,
    pub file_uuid: Option<String>,
    #[serde(default)]
    pub top_level_keys: Vec<RootKeyMetadata>,
    #[serde(default)]
    pub tree_names: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootKeyMetadata {
    pub name: String,
    pub class_name: String,
    pub cycle: i32,
}

pub fn inspect_inputs(input_set: &InputSet, options: &InspectOptions) -> InspectReport {
    let mut report = InspectReport {
        input_count: input_set.files.len(),
        total_size_bytes: input_set.total_size_bytes,
        root_metadata_enabled: options.root_metadata,
        root_metadata: Vec::new(),
        warnings: Vec::new(),
    };

    if !options.root_metadata {
        return report;
    }

    for input in &input_set.files {
        match inspect_root_file(&input.path, &options.root) {
            Ok(metadata) => report.root_metadata.push(metadata),
            Err(error) => report.warnings.push(format!(
                "ROOT metadata unavailable for {}: {error}",
                input.path.display()
            )),
        }
    }

    report
}

#[must_use]
pub fn format_inspect_report(report: &InspectReport) -> String {
    let mut output = String::new();
    let file_label = if report.input_count == 1 {
        "file"
    } else {
        "files"
    };

    writeln!(&mut output, "inputs: {} {file_label}", report.input_count).expect("write to string");
    writeln!(&mut output, "total size: {} bytes", report.total_size_bytes)
        .expect("write to string");

    if report.root_metadata_enabled {
        output.push('\n');
        output.push_str("ROOT metadata:\n");

        if report.root_metadata.is_empty() {
            output.push_str("  none available\n");
        }

        for metadata in &report.root_metadata {
            writeln!(&mut output, "file: {}", metadata.path.display()).expect("write to string");

            if let Some(uuid) = &metadata.file_uuid {
                writeln!(&mut output, "  uuid: {uuid}").expect("write to string");
            }

            write_compression(&mut output, metadata);
            write_keys(&mut output, metadata);
            write_trees(&mut output, metadata);

            for warning in &metadata.warnings {
                writeln!(&mut output, "  warning: {warning}").expect("write to string");
            }
        }
    }

    if !report.warnings.is_empty() {
        output.push('\n');
        output.push_str("Warnings:\n");
        for warning in &report.warnings {
            writeln!(&mut output, "- {warning}").expect("write to string");
        }
    }

    output
}

pub fn parse_root_metadata_output(output: &str) -> Result<RootFileMetadata> {
    let start = output
        .find(ROOT_METADATA_BEGIN)
        .context("ROOT output did not include radd metadata start marker")?;
    let after_start = &output[start + ROOT_METADATA_BEGIN.len()..];
    let end = after_start
        .find(ROOT_METADATA_END)
        .context("ROOT output did not include radd metadata end marker")?;
    let json = after_start[..end].trim();

    serde_json::from_str(json).context("could not parse ROOT metadata JSON")
}

fn inspect_root_file(path: &Path, root: &Path) -> Result<RootFileMetadata> {
    let macro_path = temporary_macro_path();
    let macro_source = root_macro_source(path, &macro_function_name(&macro_path));
    fs::write(&macro_path, macro_source).with_context(|| {
        format!(
            "could not write temporary ROOT inspection macro: {}",
            macro_path.display()
        )
    })?;

    let output = Command::new(resolve_root_executable(root))
        .arg("-l")
        .arg("-b")
        .arg("-q")
        .arg(&macro_path)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not run ROOT executable `{}`", root.display()));

    let cleanup_result = fs::remove_file(&macro_path);

    let output = output?;
    if let Err(error) = cleanup_result
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error).with_context(|| {
            format!(
                "could not remove temporary ROOT inspection macro: {}",
                macro_path.display()
            )
        });
    }

    let mut combined_output = String::from_utf8_lossy(&output.stdout).into_owned();
    combined_output.push_str(&String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        bail!(
            "ROOT exited with status {}: {}",
            output.status,
            first_nonempty_line(&combined_output).unwrap_or("no diagnostic output")
        );
    }

    parse_root_metadata_output(&combined_output)
}

fn write_compression(output: &mut String, metadata: &RootFileMetadata) {
    if metadata.compression_algorithm.is_none()
        && metadata.compression_level.is_none()
        && metadata.compression_settings.is_none()
    {
        return;
    }

    output.push_str("  compression:");
    if let Some(algorithm) = metadata.compression_algorithm {
        write!(output, " algorithm {algorithm}").expect("write to string");
    }
    if let Some(level) = metadata.compression_level {
        write!(output, " level {level}").expect("write to string");
    }
    if let Some(settings) = metadata.compression_settings {
        write!(output, " settings {settings}").expect("write to string");
    }
    output.push('\n');
}

fn write_keys(output: &mut String, metadata: &RootFileMetadata) {
    if metadata.top_level_keys.is_empty() {
        output.push_str("  keys: none\n");
        return;
    }

    output.push_str("  keys:\n");
    for key in &metadata.top_level_keys {
        writeln!(
            output,
            "    {} ({};{})",
            key.name, key.class_name, key.cycle
        )
        .expect("write to string");
    }
}

fn write_trees(output: &mut String, metadata: &RootFileMetadata) {
    if metadata.tree_names.is_empty() {
        return;
    }

    writeln!(output, "  trees: {}", metadata.tree_names.join(", ")).expect("write to string");
}

fn first_nonempty_line(output: &str) -> Option<&str> {
    output.lines().map(str::trim).find(|line| !line.is_empty())
}

fn resolve_root_executable(root: &Path) -> PathBuf {
    if root.components().count() > 1 || root.is_absolute() {
        root.to_path_buf()
    } else {
        which::which(root).unwrap_or_else(|_| root.to_path_buf())
    }
}

fn temporary_macro_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "radd_root_inspect_{}_{}.C",
        std::process::id(),
        nanos
    ))
}

fn macro_function_name(path: &Path) -> String {
    path.file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("radd_root_inspect")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn root_macro_source(path: &Path, function_name: &str) -> String {
    let path = cxx_string_literal(&path.to_string_lossy());

    format!(
        r#"#include <iostream>
#include <string>
#include "TClass.h"
#include "TFile.h"
#include "TKey.h"
#include "TList.h"
#include "TTree.h"
#include "TUUID.h"

static void radd_json_string(const char* value) {{
  std::cout << "\"";
  if (value) {{
    for (const char* p = value; *p; ++p) {{
      switch (*p) {{
        case '\\': std::cout << "\\\\"; break;
        case '"': std::cout << "\\\""; break;
        case '\n': std::cout << "\\n"; break;
        case '\r': std::cout << "\\r"; break;
        case '\t': std::cout << "\\t"; break;
        default: std::cout << *p; break;
      }}
    }}
  }}
  std::cout << "\"";
}}

void {function_name}() {{
  const char* radd_path = {path};
  std::cout << "{ROOT_METADATA_BEGIN}\n";

  TFile* file = TFile::Open(radd_path, "READ");
  if (!file || file->IsZombie()) {{
    std::cout << "{{\"path\":";
    radd_json_string(radd_path);
    std::cout << ",\"warnings\":[\"could not open file with ROOT\"]}}\n";
    std::cout << "{ROOT_METADATA_END}\n";
    if (file) {{
      file->Close();
      delete file;
    }}
    return;
  }}

  std::cout << "{{\"path\":";
  radd_json_string(radd_path);
  std::cout << ",\"compression_algorithm\":" << file->GetCompressionAlgorithm();
  std::cout << ",\"compression_level\":" << file->GetCompressionLevel();
  std::cout << ",\"compression_settings\":" << file->GetCompressionSettings();

  TUUID uuid = file->GetUUID();
  std::cout << ",\"file_uuid\":";
  radd_json_string(uuid.AsString());

  std::cout << ",\"top_level_keys\":[";
  bool first_key = true;
  TList* keys = file->GetListOfKeys();
  if (keys) {{
    TIter next(keys);
    TKey* key = nullptr;
    while ((key = static_cast<TKey*>(next()))) {{
      if (!first_key) {{
        std::cout << ",";
      }}
      first_key = false;
      std::cout << "{{\"name\":";
      radd_json_string(key->GetName());
      std::cout << ",\"class_name\":";
      radd_json_string(key->GetClassName());
      std::cout << ",\"cycle\":" << key->GetCycle() << "}}";

    }}
  }}

  std::cout << "],\"tree_names\":[";
  bool first_tree = true;
  if (keys) {{
    TIter next_tree(keys);
    TKey* key = nullptr;
    while ((key = static_cast<TKey*>(next_tree()))) {{
      TClass* key_class = TClass::GetClass(key->GetClassName());
      if (key_class && key_class->InheritsFrom(TTree::Class())) {{
        if (!first_tree) {{
          std::cout << ",";
        }}
        first_tree = false;
        radd_json_string(key->GetName());
      }}
    }}
  }}
  std::cout << "],\"warnings\":[]}}\n";
  std::cout << "{ROOT_METADATA_END}\n";
  file->Close();
  delete file;
}}
"#
    )
}

fn cxx_string_literal(value: &str) -> String {
    let mut escaped = String::from("\"");

    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(escaped, "\\x{:02x}", u32::from(character)).expect("write to string");
            }
            character => escaped.push(character),
        }
    }

    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use super::{
        InspectOptions, cxx_string_literal, format_inspect_report, inspect_inputs,
        parse_root_metadata_output,
    };
    use crate::input::{InputFile, InputSet};

    #[test]
    fn parses_root_metadata_between_markers() {
        let metadata = parse_root_metadata_output(
            r#"ROOT banner
__RADD_ROOT_METADATA_BEGIN__
{"path":"/tmp/a.root","compression_algorithm":1,"compression_level":4,"compression_settings":104,"file_uuid":"uuid","top_level_keys":[{"name":"Events","class_name":"TTree","cycle":1}],"tree_names":["Events"],"warnings":[]}
__RADD_ROOT_METADATA_END__
"#,
        )
        .expect("metadata");

        assert_eq!(metadata.path, PathBuf::from("/tmp/a.root"));
        assert_eq!(metadata.compression_algorithm, Some(1));
        assert_eq!(metadata.top_level_keys[0].name, "Events");
        assert_eq!(metadata.tree_names, ["Events"]);
    }

    #[test]
    fn root_metadata_failures_are_reported_as_warnings() {
        let input_set = InputSet {
            files: vec![InputFile {
                path: PathBuf::from("/tmp/missing-root-file.root"),
                size_bytes: 1,
                modified_time: Some(SystemTime::UNIX_EPOCH),
            }],
            total_size_bytes: 1,
        };

        let report = inspect_inputs(
            &input_set,
            &InspectOptions {
                root_metadata: true,
                root: PathBuf::from("/definitely/missing/root"),
            },
        );

        assert!(report.root_metadata.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(
            report.warnings[0].contains("ROOT metadata unavailable"),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn formats_root_metadata() {
        let metadata = parse_root_metadata_output(
            r#"__RADD_ROOT_METADATA_BEGIN__
{"path":"a.root","compression_algorithm":1,"compression_level":4,"compression_settings":104,"file_uuid":"uuid","top_level_keys":[{"name":"Events","class_name":"TTree","cycle":1}],"tree_names":["Events"],"warnings":[]}
__RADD_ROOT_METADATA_END__"#,
        )
        .expect("metadata");

        let output = format_inspect_report(&super::InspectReport {
            input_count: 1,
            total_size_bytes: 12,
            root_metadata_enabled: true,
            root_metadata: vec![metadata],
            warnings: Vec::new(),
        });

        assert!(output.contains("ROOT metadata:"));
        assert!(output.contains("compression: algorithm 1 level 4 settings 104"));
        assert!(output.contains("Events (TTree;1)"));
        assert!(output.contains("trees: Events"));
    }

    #[test]
    fn cxx_string_literal_escapes_special_characters() {
        assert_eq!(
            cxx_string_literal("a \"quoted\" path\\file.root"),
            r#""a \"quoted\" path\\file.root""#
        );
    }
}
