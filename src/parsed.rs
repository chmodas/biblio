use std::fmt;

use crate::Record;

/// The outcome of parsing one input: every record that could be read, and what went wrong.
///
/// Parsers keep every record they can. Anything they had to drop or patch is reported in
/// `diagnostics`, sorted by line.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Parsed {
    /// The records, in the order they appear in the input.
    pub records: Vec<Record>,

    /// Problems found while parsing, sorted by line (stable, so problems on the same line keep the
    /// order in which they were found).
    pub diagnostics: Vec<Diagnostic>,
}

/// A problem with one record or line of the input.
///
/// The record concerned is kept unless the variant says otherwise. Every variant carries a
/// one-indexed `line`: the line where the record or entry starts, or for [`Self::IgnoredLine`] and
/// [`Self::Truncated`], the line concerned. New variants may be added, so a `match` needs a
/// wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Diagnostic {
    /// The record has no title, or only a blank one. The record is kept.
    MissingTitle {
        /// Line where the record starts.
        line: usize,
    },

    /// A `RIS` record was not closed with `ER` before the next `TY` or the end of the input. The
    /// record is kept.
    UnterminatedRecord {
        /// Line where the record starts.
        line: usize,
    },

    /// A line could not be attached to any record and was ignored.
    IgnoredLine {
        /// The ignored line.
        line: usize,
    },

    /// A `BibTeX` entry or `@string` definition could not be parsed and was dropped.
    SkippedEntry {
        /// Line where the entry starts.
        line: usize,
        /// What the underlying parser reported.
        message: String,
    },

    /// A `BibTeX` entry's `crossref` or `xdata` led back to the entry itself, directly or through
    /// other entries or `ids` aliases. That field was removed in full, so the entry inherits
    /// nothing through it, and an `xdata` listing several entries loses all of them. The entry is
    /// kept.
    ReferenceCycle {
        /// Line where the entry starts.
        line: usize,
    },

    /// Resolving a `BibTeX` entry's `crossref` and `xdata` would have gone too deep, branched too
    /// much or copied too much, or the input's budget for resolving them ran out. Both fields were
    /// removed in full, so the entry inherits nothing. The entry is kept.
    ReferenceTooDeep {
        /// Line where the entry starts.
        line: usize,
    },

    /// A field of a `BibTeX` entry whose date other entries read while resolving their references
    /// held a value that the underlying parser fails on there: a `date` with digits that are not
    /// ASCII where it reads a number, with more than four year digits or month `00` among unknown
    /// digits (`X`), or with an unknown day in a month of fewer than 31 days that a further entry
    /// reads again; or a `month` followed by a day number above 255. The field was removed before
    /// resolving, so neither the entry nor those that refer to it have it. The entry is kept.
    DroppedField {
        /// Line where the entry starts.
        line: usize,
        /// The field's name, in lower case.
        field: String,
    },

    /// The input broke off: an `EndNote XML` syntax error, the end of the input inside an `EndNote
    /// XML` record, or a CSV quoted cell left open at the end of the input. The records before it
    /// are kept, the record in progress is dropped, and parsing stops.
    Truncated {
        /// Line of the syntax error, or where the unfinished record starts.
        line: usize,
        /// What went wrong.
        message: String,
    },
    /// A CSV row has more cells than the header, and at least one of the surplus cells is not
    /// blank. The surplus cells are dropped and the record is kept.
    ExtraCells {
        /// Line where the row starts.
        line: usize,
        /// How many more cells than the header the row has.
        count: usize,
    },
}

impl Diagnostic {
    /// The one-indexed line the diagnostic refers to.
    #[must_use]
    pub fn line(&self) -> usize {
        match self {
            Self::MissingTitle { line }
            | Self::UnterminatedRecord { line }
            | Self::IgnoredLine { line }
            | Self::ReferenceCycle { line }
            | Self::ReferenceTooDeep { line }
            | Self::DroppedField { line, .. }
            | Self::SkippedEntry { line, .. }
            | Self::Truncated { line, .. }
            | Self::ExtraCells { line, .. } => *line,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTitle { line } => write!(f, "record at line {line} has no title"),
            Self::UnterminatedRecord { line } => {
                write!(f, "record at line {line} was not closed with ER")
            }
            Self::IgnoredLine { line } => write!(f, "ignored line {line}"),
            Self::SkippedEntry { line, message } => {
                write!(f, "skipped entry at line {line}: {message}")
            }
            Self::ReferenceCycle { line } => {
                write!(
                    f,
                    "reference cycle at line {line}: crossref or xdata removed"
                )
            }
            Self::ReferenceTooDeep { line } => write!(
                f,
                "references at line {line} go too deep or copy too much: crossref and xdata removed"
            ),
            Self::DroppedField { line, field } => write!(
                f,
                "field {field} of the entry at line {line} could not be read and was removed"
            ),
            Self::Truncated { line, message } => {
                write!(f, "input truncated at line {line}: {message}")
            }
            Self::ExtraCells { line, count } => {
                write!(
                    f,
                    "row at line {line} has {count} more cells than the header; they were dropped"
                )
            }
        }
    }
}
