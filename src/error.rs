use std::fmt;

/// Errors produced when parsing or serializing bibliographic data.
///
/// # Matching
///
/// ```
/// use biblio::Error;
///
/// let err = Error::MalformedSyntax {
///     line: 3,
///     message: "unexpected end tag".into(),
/// };
/// assert_eq!(
///     err.to_string(),
///     "malformed syntax at line 3: unexpected end tag"
/// );
/// ```
///
/// Problems with single records are not errors; they are reported as
/// [`Diagnostic`](crate::Diagnostic)s alongside the records that could be read.
///
/// With the `serde` feature, an error is a JSON object whose `kind` names the variant in snake
/// case: `{"kind": "empty_input"}`, `{"kind": "malformed_syntax", "line": 3, "message": "..."}`,
/// `{"kind": "internal", "message": "..."}`. An unknown `kind`, or a missing field, cannot be
/// read; members the variant has no field for are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(into = "ErrorRepr", from = "ErrorRepr")
)]
#[non_exhaustive]
pub enum Error {
    /// The input was empty or only contained whitespace.
    EmptyInput,

    /// The format was recognised, but the data is syntactically broken.
    ///
    /// `line` points to the offending line (one-indexed) and `message` describes what was expected
    /// versus what was found.
    MalformedSyntax { line: usize, message: String },

    /// An underlying system or third-party error that does not map to a more specific variant.
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "input was empty or only contained whitespace"),
            Self::MalformedSyntax { line, message } => {
                write!(f, "malformed syntax at line {line}: {message}")
            }
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// The JSON form of [`Error`], tagged by `kind` like [`Diagnostic`](crate::Diagnostic).
///
/// [`Error::Internal`] holds its message directly, and serde can only put a `kind` next to the
/// fields of a variant that has named fields; it would fail when writing, not when compiling. So
/// errors are written and read through this mirror, whose variants all have named fields.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ErrorRepr {
    /// [`Error::EmptyInput`].
    EmptyInput,
    /// [`Error::MalformedSyntax`].
    MalformedSyntax {
        /// The line of the error.
        line: usize,
        /// What was expected and what was found.
        message: String,
    },
    /// [`Error::Internal`].
    Internal {
        /// The underlying error's description.
        message: String,
    },
}

#[cfg(feature = "serde")]
impl From<Error> for ErrorRepr {
    fn from(error: Error) -> Self {
        match error {
            Error::EmptyInput => Self::EmptyInput,
            Error::MalformedSyntax { line, message } => Self::MalformedSyntax { line, message },
            Error::Internal(message) => Self::Internal { message },
        }
    }
}

#[cfg(feature = "serde")]
impl From<ErrorRepr> for Error {
    fn from(repr: ErrorRepr) -> Self {
        match repr {
            ErrorRepr::EmptyInput => Self::EmptyInput,
            ErrorRepr::MalformedSyntax { line, message } => Self::MalformedSyntax { line, message },
            ErrorRepr::Internal { message } => Self::Internal(message),
        }
    }
}
