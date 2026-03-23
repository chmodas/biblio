/// The publication date of a bibliographic record.
///
/// Only the `year` is guaranteed to be present. Month and day are available when the source
/// format provides them (e.g. NBIB's `DP` field often includes all three, while BibTeX
/// entries may only carry a year).
///
/// # Examples
///
/// ```
/// use biblio::PublicationDate;
///
/// // Year only (most common)
/// let date = PublicationDate { year: 2024, month: None, day: None };
/// assert_eq!(date.to_string(), "2024");
///
/// // Full date
/// let date = PublicationDate { year: 2024, month: Some(9), day: Some(1) };
/// assert_eq!(date.to_string(), "2024-09-01");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicationDate {
    /// The year of publication.
    pub year: i32,
    /// Month of publication (1–12), if known.
    pub month: Option<u8>,
    /// Day of publication (1–31), if known.
    pub day: Option<u8>,
}

impl std::fmt::Display for PublicationDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.month, self.day) {
            (Some(m), Some(d)) => write!(f, "{}-{:02}-{:02}", self.year, m, d),
            (Some(m), None) => write!(f, "{}-{:02}", self.year, m),
            _ => write!(f, "{}", self.year),
        }
    }
}

/// A single bibliographic record in a unified, format-agnostic representation.
///
/// # Construction
///
/// The struct derives [`Default`], so the idiomatic way to build one
/// is to start from the default and set only the fields you have:
///
/// ```
/// use biblio::{Record, PublicationDate};
///
/// let rec = Record {
///     title: "My paper".into(),
///     authors: vec!["Doe, Jane".into()],
///     date: Some(PublicationDate { year: 2024, month: None, day: None }),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    /// Title of the publication. Present in all formats and always expected to be non-empty.
    pub title: String,

    /// Authors of the publication as display-ready strings (e.g. `"Doe, Jane"`, `"Bloggs, Joe"`).
    ///
    /// The order matches the order in the source. An empty Vec means no authors were found,
    /// which is unusual but possible (e.g. institutional reports).
    pub authors: Vec<String>,

    /// Publication date.
    pub date: Option<PublicationDate>,

    /// Full journal or periodical name (e.g. `"Nature"`).
    pub journal: Option<String>,

    /// Digital Object Identifier, without a URL prefix (e.g. `"10.1038/s41586-024-07386-0"`).
    pub doi: Option<String>,

    /// Page range or article number as it appears in the source (e.g. `"123-130"`, `"e12345"`).
    pub pages: Option<String>,

    /// Volume identifier (e.g. `"12"`, `"S1"`).
    pub volume: Option<String>,

    /// Issue or number within a volume (e.g. `"3"`, `"Special Issue"`).
    pub number: Option<String>,

    /// Abstract text.
    pub abstract_text: Option<String>,

    /// ISBN (books) or ISSN (journals), depending on the publication type and what the source
    /// format provides.
    pub isbn: Option<String>,
}
