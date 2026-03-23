use std::fmt;

/// Errors produced when parsing or serialising bibliographic data.
///
/// # Matching
///
/// ```
/// use biblio::Error;
///
/// let err = Error::MissingRequiredField { tag: "TY", line: 1 };
/// assert_eq!(err.to_string(), "missing required field 'TY' at line 1");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input was empty or only contained whitespace.
    EmptyInput,

    /// A record was found but lacked a mandatory field.
    ///
    /// `tag` names the missing field in the source format's own vocabulary
    /// (e.g. `"TY"` for RIS, `"title"` for BibTeX). `line` is the
    /// one-indexed line number where the record started.
    MissingRequiredField { tag: &'static str, line: usize },

    /// The format was recognised, but the data is syntactically broken.
    ///
    /// `line` points to the offending line (one-indexed) and `message`
    /// describes what was expected versus what was found.
    MalformedSyntax { line: usize, message: String },

    /// An underlying system or third-party error that does not map to a
    /// more specific variant.
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "input was empty or only contained whitespace"),
            Self::MissingRequiredField { tag, line } => {
                write!(f, "missing required field '{tag}' at line {line}")
            }
            Self::MalformedSyntax { line, message } => {
                write!(f, "malformed syntax at line {line}: {message}")
            }
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
