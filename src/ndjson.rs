use std::{fmt, fs, io, path::Path};

/// A reader error which makes an evidence stream unusable.
#[derive(Debug)]
pub enum NdjsonReadError {
    Io(io::Error),
    InvalidUtf8(std::str::Utf8Error),
    /// A non-final record was not valid JSON. Only physical EOF can be treated
    /// as a crash boundary and only its unfinished record may be discarded.
    InteriorCorruption {
        line: usize,
        detail: String,
    },
}

impl fmt::Display for NdjsonReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::InvalidUtf8(error) => write!(formatter, "invalid UTF-8: {error}"),
            Self::InteriorCorruption { line, detail } => {
                write!(
                    formatter,
                    "interior NDJSON corruption at line {line}: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for NdjsonReadError {}

impl From<io::Error> for NdjsonReadError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Returns every complete JSON record in an NDJSON file.
///
/// The final non-newline-terminated fragment is accepted if it is complete
/// JSON; otherwise it is treated as a torn record caused by a writer crash.
/// No other malformed line is recoverable.
pub fn read_complete_records(path: &Path) -> Result<Vec<String>, NdjsonReadError> {
    let bytes = fs::read(path)?;
    let text = std::str::from_utf8(&bytes).map_err(NdjsonReadError::InvalidUtf8)?;
    let has_final_newline = text.ends_with('\n');
    let physical_lines: Vec<&str> = text.split('\n').collect();
    let last_index = physical_lines.len().saturating_sub(1);
    let mut records = Vec::new();

    for (index, original_line) in physical_lines.iter().enumerate() {
        if has_final_newline && index == last_index && original_line.is_empty() {
            continue;
        }
        let line = original_line.strip_suffix('\r').unwrap_or(original_line);
        if line.is_empty() {
            return Err(NdjsonReadError::InteriorCorruption {
                line: index + 1,
                detail: "empty line is not a JSON record".to_owned(),
            });
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(_) => records.push(line.to_owned()),
            Err(error) if index == last_index && !has_final_newline => {
                // Physical EOF is the sole recovery boundary. Deliberately
                // discard precisely this torn final record.
                let _ = error;
            }
            Err(error) => {
                return Err(NdjsonReadError::InteriorCorruption {
                    line: index + 1,
                    detail: error.to_string(),
                });
            }
        }
    }
    Ok(records)
}
