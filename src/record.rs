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
///
/// With the `serde` feature, a date is `{"year": 2024, "month": 9, "day": null}` in JSON. `month`
/// and `day` may be left out; `year` may not. Any value of a field's type is read, so a month or
/// day out of range is kept as given, but one that does not fit the type, or a number with a
/// fraction, cannot be read. Members the date has no field for are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
///
/// # JSON
///
/// With the `serde` feature, a record is a JSON object with a member for every field, named as
/// here: `null` for a field without a value, `[]` for an empty list, and `extras` as an object of
/// string arrays, whose members come in no particular order. When reading, a member that is left
/// out takes its empty value, and a member the record has no field for is ignored, so JSON written
/// by a release with more fields can still be read. `null` is accepted only for optional fields.
/// The record may also be read from an array of its fields in order, missing ones taking their
/// empty value; it is never written that way.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
#[non_exhaustive]
pub struct Record {
    /// Title of the publication, or `None` if the source has none.
    ///
    /// Parsers keep records without a title and report a [`Diagnostic::MissingTitle`] for a title
    /// that is `None` or blank. `EndNote XML` reads an empty `<title/>` as `None`, and CSV a blank
    /// title cell; the other formats keep a blank title tag as `Some("")`.
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
    ///
    /// A record can list several, such as a journal's print and electronic ISSNs, and every format
    /// keeps one here by the same rule, so the order they are listed in does not matter:
    ///
    /// - **Splitting:** each value is split at `,`, `;` and line breaks, and then at whitespace.
    ///   Text in parentheses, such as `(Print)`, is one piece, and a label even if it holds digits;
    ///   so is a word with fewer than eight digits, such as `ISBN` or `ISBN-13:`. A label belongs
    ///   to the identifier before it, or to the first identifier when it comes before every one.
    /// - **Choosing:** the identifier whose digits, with an `X` check digit, come first when
    ///   compared as text is kept, without its labels or the characters around its digits, so
    ///   `0891-6640 (Print)` gives `0891-6640`. A value with no identifier, such as `n/a`, is kept
    ///   whole, but only when there is no identifier at all.
    /// - **The others** are kept, as written with their labels, in the format's own extra:
    ///   `"ris:SN"`, `"nbib:IS"`, `"endnote:isbn"`, `"bibtex:isbn"` or `"bibtex:issn"`, or the CSV
    ///   column's. The serialiser writes them back after this identifier. Like other extras, they
    ///   are not written to another format.
    ///
    /// `BibTeX` and CSV read the identifier from the `ISBN` field or column, and from `ISSN` only
    /// when that is blank, so an ISBN is kept even when a lower ISSN is listed beside it.
    ///
    /// Some identifiers do not come back as set. A record whose identifier is not the lowest of
    /// those it lists, or holds several, gets the lowest when it is written and read again. Blank
    /// values are not kept; `EndNote XML` and CSV, which cannot write an empty identifier, give
    /// none for a value of only whitespace or separators. An ISBN written with spaces between its
    /// groups (`978 3 16 148410 0`) is kept whole only when nothing else is listed. Two exports of
    /// one article agree only when both list its lowest identifier: one that leaves out the
    /// electronic ISSN, when that ISSN is the lowest, gives the print ISSN instead.
    pub isbn: Option<String>,

    /// The bibliographic database the record was exported from, as the source names it (e.g.
    /// `"Scopus"`, `"MEDLINE"`).
    ///
    /// `RIS` reads and writes it as `DB`, and `EndNote XML` as `<remote-database-name>`. `NBIB` and
    /// `BibTeX` neither read nor write it, so converting a record to them drops the database from
    /// the output but not from the record. `RIS` keeps a blank `DB` as `Some("")`; `EndNote XML`
    /// ignores an empty element. The generic CSV layout reads it from a `source` or `database`
    /// column and writes it as `source`; the other CSV layouts have no column for it.
    pub database: Option<String>,

    /// A user's label on the record, as written in the source (e.g. `"OLD"`, `"included"`).
    ///
    /// `RIS` reads and writes it as `LB`, and `EndNote XML` as `<label>`. `NBIB` and `BibTeX`
    /// neither read nor write it, so converting a record to them drops the label from the output
    /// but not from the record; a `BibTeX` `label` field is a substitute citation label, not a
    /// user's label, and stays in [`Record::extras`]. `RIS` keeps a blank `LB` as `Some("")`;
    /// `EndNote XML` ignores an empty element. The generic CSV layout reads and writes it as
    /// `label`; the other CSV layouts have no column for it.
    pub label: Option<String>,

    /// Keywords and subject headings, in source order (e.g. `"Alzheimer Disease"`).
    ///
    /// `RIS` reads and writes them as `KW`, `EndNote XML` as `keywords/keyword`, and `BibTeX` as
    /// one `keywords` field, split on `,` and `;` outside brace groups and written with each `,`
    /// and `;` inside a keyword in braces. `NBIB` reads them from `OT` and from the subject
    /// headings in `MH`, and writes them all as `OT`. Values are trimmed and blank ones dropped;
    /// duplicates are kept. Serialisers skip blank values. `BibTeX` turns `--` into a dash and
    /// collapses whitespace in `keywords`, as in every field. CSV holds them in one cell,
    /// separated by `;`.
    pub keywords: Vec<String>,

    /// Links to the record, in source order (e.g. `"https://doi.org/10.1000/xyz"`).
    ///
    /// `RIS` reads and writes them as `UR`, `EndNote XML` as `urls/related-urls/url`, and `BibTeX`
    /// as `url`, which holds one link, so only the first is written. `NBIB` has no tag for links,
    /// so it never sets them or writes them. Values are trimmed and blank ones dropped;
    /// duplicates are kept. Serialisers skip blank values. `BibTeX` turns `--` into a dash and
    /// collapses whitespace in `url`, as in every field. The generic CSV layout holds them in one
    /// cell, separated by `;`; the Zotero and `SyRF` layouts hold one link, so only the first is
    /// written.
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
    /// - CSV: the column's header as written, e.g. `"csv:record_id"`, `"csv:Item Type"`.
    ///
    /// Each serialiser writes only the keys of its own format, so values read from one format are
    /// kept on the record but not written to another. It skips keys that are not valid tags in its
    /// format, and keys for tags it reads into the fields above, except the identifiers listed
    /// beside [`Record::isbn`]. `BibTeX` and CSV write only the first value of a key, because a
    /// field or cell holds one value, except for those identifiers, which they write together.
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
