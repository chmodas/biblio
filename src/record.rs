/// The publication date of a bibliographic record.
///
/// Only the `year` is guaranteed to be present. Month and day are available when the source format
/// provides them (e.g. `NBIB`'s `DP` field often includes all three, while `BibTeX` entries may
/// only carry a year).
///
/// # Examples
///
/// ```
/// use biblio::PublicationDate;
///
/// // Year only (most common)
/// let date = PublicationDate {
///     year: 2024,
///     month: None,
///     day: None,
/// };
/// assert_eq!(date.to_string(), "2024");
///
/// // Full date
/// let date = PublicationDate {
///     year: 2024,
///     month: Some(9),
///     day: Some(1),
/// };
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

use std::collections::HashMap;

/// A single bibliographic record in a unified, format-agnostic representation.
///
/// # Construction
///
/// The struct is `#[non_exhaustive]`, so new fields can be added without breaking callers. Build
/// one with [`Record::builder`], setting only the fields you have:
///
/// ```
/// use biblio::{PublicationDate, Record};
///
/// let rec = Record::builder()
///     .title("My paper")
///     .author("Doe, Jane")
///     .date(PublicationDate {
///         year: 2024,
///         month: None,
///         day: None,
///     })
///     .build();
/// assert_eq!(rec.title.as_deref(), Some("My paper"));
/// ```
///
/// The fields stay public, so a record can also be changed after it is built.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct Record {
    /// Title of the publication, or `None` if the source has none.
    ///
    /// Parsers keep records without a title and report a [`Diagnostic::MissingTitle`] for a title
    /// that is `None` or blank. `EndNote XML` reads an empty `<title/>` as `None`; the other
    /// formats keep a blank title tag as `Some("")`.
    ///
    /// [`Diagnostic::MissingTitle`]: crate::Diagnostic::MissingTitle
    pub title: Option<String>,

    /// Authors of the publication as display-ready strings (e.g. `"Doe, Jane"`, `"Bloggs, Joe"`).
    ///
    /// The order matches the order in the source. An empty Vec means no authors were found, which
    /// is unusual but possible (e.g. institutional reports).
    pub authors: Vec<String>,

    /// Publication date.
    pub date: Option<PublicationDate>,

    /// Journal, periodical or other containing work (e.g. `"Nature"`, or a book or proceedings
    /// title). This is the full name where the source provides one, and otherwise an abbreviation
    /// (e.g. from `RIS` `JA`, `J2` or `J1`).
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

    /// The bibliographic database the record was exported from, as the source names it (e.g.
    /// `"Scopus"`, `"MEDLINE"`).
    ///
    /// `RIS` reads and writes it as `DB`, and `EndNote XML` as `<remote-database-name>`. `NBIB` and
    /// `BibTeX` neither read nor write it, so converting a record to them drops the database from
    /// the output but not from the record. `RIS` keeps a blank `DB` as `Some("")`; `EndNote XML`
    /// ignores an empty element.
    pub database: Option<String>,

    /// A user's label on the record, as written in the source (e.g. `"OLD"`, `"included"`).
    ///
    /// `RIS` reads and writes it as `LB`, and `EndNote XML` as `<label>`. `NBIB` and `BibTeX`
    /// neither read nor write it, so converting a record to them drops the label from the output
    /// but not from the record; a `BibTeX` `label` field is a substitute citation label, not a
    /// user's label, and stays in [`Record::extras`]. `RIS` keeps a blank `LB` as `Some("")`;
    /// `EndNote XML` ignores an empty element.
    pub label: Option<String>,

    /// Keywords and subject headings, in source order (e.g. `"Alzheimer Disease"`).
    ///
    /// `RIS` reads and writes them as `KW`, `EndNote XML` as `keywords/keyword`, and `BibTeX` as
    /// one `keywords` field, split on `,` and `;` outside brace groups and written with each `,`
    /// and `;` inside a keyword in braces. `NBIB` reads them from `OT` and from the subject
    /// headings in `MH`, and writes them all as `OT`. Values are trimmed and blank ones dropped;
    /// duplicates are kept. Serialisers skip blank values. `BibTeX` turns `--` into a dash and
    /// collapses whitespace in `keywords`, as in every field.
    pub keywords: Vec<String>,

    /// Links to the record, in source order (e.g. `"https://doi.org/10.1000/xyz"`).
    ///
    /// `RIS` reads and writes them as `UR`, `EndNote XML` as `urls/related-urls/url`, and `BibTeX`
    /// as `url`, which holds one link, so only the first is written. `NBIB` has no tag for links,
    /// so it never sets them or writes them. Values are trimmed and blank ones dropped;
    /// duplicates are kept. Serialisers skip blank values. `BibTeX` turns `--` into a dash and
    /// collapses whitespace in `url`, as in every field.
    pub urls: Vec<String>,

    /// Every field of the source that has no slot above, keyed by format and tag, with its values
    /// in source order.
    ///
    /// Keys have the form `"{format}:{tag}"`, so values from different formats never collide:
    ///
    /// - `RIS`: `"ris:N1"`, `"ris:L1"`, …, plus `"ris:TY"` for the record type.
    /// - `NBIB`: `"nbib:PMID"`, `"nbib:GR"`, ….
    /// - `BibTeX`: `"bibtex:note"`, `"bibtex:urldate"`, … (field names in lower case), plus
    ///   `"bibtex:@key"` for the citation key and `"bibtex:@type"` for the entry type. A field name
    ///   cannot contain `@`, so these never clash with a real field.
    /// - `EndNote XML`: the element path below `<record>`, e.g. `"endnote:urls/pdf-urls/url"`.
    ///
    /// Each serialiser writes only the keys of its own format, so values read from one format are
    /// kept on the record but not written to another. It skips keys that are not valid tags in its
    /// format, and keys for tags it reads into the fields above. `BibTeX` writes only the first
    /// value of a key, because a field can appear only once in an entry.
    pub extras: HashMap<String, Vec<String>>,
}

impl Record {
    /// Start building a record with every field empty.
    #[must_use]
    pub fn builder() -> RecordBuilder {
        RecordBuilder::default()
    }
}

/// Builds a [`Record`] one field at a time. Created by [`Record::builder`].
///
/// Each method sets one field and returns the builder; [`RecordBuilder::build`] returns the record.
#[derive(Debug, Clone, Default)]
pub struct RecordBuilder {
    /// The record being built.
    record: Record,
}

impl RecordBuilder {
    /// Set the title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.record.title = Some(title.into());
        self
    }

    /// Append one author, as a display-ready string such as `"Doe, Jane"`.
    #[must_use]
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.record.authors.push(author.into());
        self
    }

    /// Append several authors, in order.
    #[must_use]
    pub fn authors(mut self, authors: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.record
            .authors
            .extend(authors.into_iter().map(Into::into));
        self
    }

    /// Set the publication date.
    #[must_use]
    pub fn date(mut self, date: PublicationDate) -> Self {
        self.record.date = Some(date);
        self
    }

    /// Set the journal, periodical or other containing work.
    #[must_use]
    pub fn journal(mut self, journal: impl Into<String>) -> Self {
        self.record.journal = Some(journal.into());
        self
    }

    /// Set the DOI, without a URL prefix.
    #[must_use]
    pub fn doi(mut self, doi: impl Into<String>) -> Self {
        self.record.doi = Some(doi.into());
        self
    }

    /// Set the page range or article number.
    #[must_use]
    pub fn pages(mut self, pages: impl Into<String>) -> Self {
        self.record.pages = Some(pages.into());
        self
    }

    /// Set the volume.
    #[must_use]
    pub fn volume(mut self, volume: impl Into<String>) -> Self {
        self.record.volume = Some(volume.into());
        self
    }

    /// Set the issue or number within the volume.
    #[must_use]
    pub fn number(mut self, number: impl Into<String>) -> Self {
        self.record.number = Some(number.into());
        self
    }

    /// Set the abstract.
    #[must_use]
    pub fn abstract_text(mut self, abstract_text: impl Into<String>) -> Self {
        self.record.abstract_text = Some(abstract_text.into());
        self
    }

    /// Set the ISBN or ISSN.
    #[must_use]
    pub fn isbn(mut self, isbn: impl Into<String>) -> Self {
        self.record.isbn = Some(isbn.into());
        self
    }

    /// Set the database the record was exported from.
    #[must_use]
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.record.database = Some(database.into());
        self
    }

    /// Set the user's label on the record.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.record.label = Some(label.into());
        self
    }

    /// Append one keyword.
    #[must_use]
    pub fn keyword(mut self, keyword: impl Into<String>) -> Self {
        self.record.keywords.push(keyword.into());
        self
    }

    /// Append several keywords, in order.
    #[must_use]
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.record
            .keywords
            .extend(keywords.into_iter().map(Into::into));
        self
    }

    /// Append one link.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.record.urls.push(url.into());
        self
    }

    /// Append several links, in order.
    #[must_use]
    pub fn urls(mut self, urls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.record.urls.extend(urls.into_iter().map(Into::into));
        self
    }

    /// Append one value to `key` in `extras`. Keys follow the scheme described on
    /// [`Record::extras`], e.g. `"ris:N1"`.
    #[must_use]
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.record
            .extras
            .entry(key.into())
            .or_default()
            .push(value.into());
        self
    }

    /// Return the built record.
    #[must_use]
    pub fn build(self) -> Record {
        self.record
    }
}
