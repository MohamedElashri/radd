//! Input inspection.

use crate::input::InputSet;

/// Format a concise human-readable summary for resolved inputs.
pub fn format_input_summary(input_set: &InputSet) -> String {
    let file_label = if input_set.files.len() == 1 {
        "file"
    } else {
        "files"
    };

    format!(
        "inputs: {} {file_label}\ntotal size: {} bytes\n",
        input_set.files.len(),
        input_set.total_size_bytes
    )
}
