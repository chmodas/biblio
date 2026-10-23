//! Parser and serializer for the `NBIB` (PubMed/MEDLINE) bibliographic format.
//!
//! # References
//!
//! - [PubMed Help](https://pubmed.ncbi.nlm.nih.gov/help/)
//! - [PubMed XML DTD (field definitions)](https://dtd.nlm.nih.gov/ncbi/pubmed/doc/out/250101/index.html)

use std::collections::HashMap;
use std::fmt::Write as _;

use winnow::Parser;
use winnow::ascii::space0;
use winnow::token::{rest, take_while};

use crate::parse_util::{
    append_to_last, append_to_opt, check_empty, first_non_blank, normalize_line_endings,
    own_extras, parsed, push_extra, report_missing_title, strip_byte_order_mark,
};
use crate::{Diagnostic, Error, Parsed, PublicationDate, Record};

/// Parse `NBIB`-formatted text into zero or more [`Record`]s.
///
/// Every record is kept, even one without a title. Problems are reported as diagnostics:
///
/// - [`Diagnostic::MissingTitle`]: a record has no `TI` or `BTI`, or only a blank one.
/// - [`Diagnostic::IgnoredLine`]: a line that is not a tag line, a continuation line or blank, or a
///   continuation line with no tag before it in its record.
///
/// A leading byte-order mark is ignored.
///
/// # Extras
///
/// Every tag without a field is kept in [`Record::extras`] as `"nbib:{tag}"` (e.g. `"nbib:PMID"`,
/// `"nbib:OT"`), one value per occurrence, with its continuation lines joined. Tags that are read
/// but lose to another are dropped: `AU` when `FAU` is present, `BTI` when `TI` is present, `CTI`
/// when `JT` is present, `LID` and `AID` values that are not the DOI, all but the last `IS`, and
/// dates in `DP` that cannot be read.
///
/// `NBIB` has no tag for the source database or a user's label, so [`Record::database`] and
/// [`Record::label`] are never set.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input, after any leading byte-order mark, is empty or
/// whitespace-only.
///
/// # Title and journal
///
/// The title is read from `TI`, falling back to the book title (`BTI`) for books, which have no
/// `TI`. The journal is read from `JT`, falling back to the collection title (`CTI`), the series a
/// book belongs to. The collection title is never used as a title, because unrelated books in the
/// same series share it. A blank tag never hides its fallback when the fallback has a value.
pub fn parse(input: &str) -> Result<Parsed, Error> {
    let input = strip_byte_order_mark(input);
    check_empty(input)?;
    let input = normalize_line_endings(input);

    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    let mut pending = PendingRecord::default();
    let mut has_content = false;

    // Each NBIB line is one of three kinds:
    //   1. Tag line:          "TAG - value"  (2-4 uppercase chars, padded to 4, then "- ")
    //   2. Continuation line: starts with 6+ spaces, appends to the previous tag's value
    //   3. Blank line:        record boundary
    for (line_idx, line) in input.lines().enumerate() {
        let line_number = line_idx + 1;

        // Blank line – finalize the current record (if any) and start a new one.
        if line.trim().is_empty() {
            if has_content {
                records.push(std::mem::take(&mut pending).finish(&mut diagnostics));
                has_content = false;
            }
            continue;
        }

        // Continuation line – append to whatever field the previous tag started.
        if line.starts_with("      ") {
            if !pending.append_continuation(line.trim()) {
                diagnostics.push(Diagnostic::IgnoredLine { line: line_number });
            }
            continue;
        }

        // Tag line – parse the tag and value, then route to the appropriate field.
        if let Ok((tag, value)) = tag_line.parse(line) {
            if !has_content {
                pending.line_start = line_number;
                has_content = true;
            }
            pending.dispatch(tag, value);
        } else {
            diagnostics.push(Diagnostic::IgnoredLine { line: line_number });
        }
    }

    // The last record may not be followed by a blank line.
    if has_content {
        records.push(pending.finish(&mut diagnostics));
    }

    Ok(parsed(records, diagnostics))
}

/// Serialize [`Record`]s to `NBIB` format.
///
/// A record with none of the fields written here has no lines, so it is not read back by
/// [`parse`].
///
/// Authors are emitted as `FAU` (full author) tags. DOIs are written as `LID` tags with a `[doi]`
/// suffix. `nbib:` extras are written as tags before the fields, `PMID` first and the rest sorted
/// by tag; keys that are not 2–4 character tags, and tags the parser reads into fields, are
/// skipped. Extras of other formats are not written, nor are [`Record::database`] and
/// [`Record::label`]. Records are separated by blank lines.
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    let mut buf = String::new();

    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }

        // Extras of this format: PMID first, as PubMed writes it, then the rest sorted by tag.
        let mut extras = own_extras(&record.extras, "nbib");
        extras.retain(|&(tag, _)| is_tag_shape(tag) && !is_read_tag(tag));
        extras.sort_by_key(|&(tag, _)| tag != "PMID");
        for (tag, values) in extras {
            for value in values {
                write_field(&mut buf, tag, value);
            }
        }

        if let Some(title) = &record.title {
            write_field(&mut buf, "TI", title);
        }

        for author in &record.authors {
            write_field(&mut buf, "FAU", author);
        }

        if let Some(date) = record.date {
            write_field(&mut buf, "DP", &format_date(date));
        }

        if let Some(journal) = &record.journal {
            write_field(&mut buf, "JT", journal);
        }

        if let Some(doi) = &record.doi {
            let lid = format!("{doi} [doi]");
            write_field(&mut buf, "LID", &lid);
        }

        if let Some(pages) = &record.pages {
            write_field(&mut buf, "PG", pages);
        }

        if let Some(volume) = &record.volume {
            write_field(&mut buf, "VI", volume);
        }

        if let Some(number) = &record.number {
            write_field(&mut buf, "IP", number);
        }

        if let Some(abstract_text) = &record.abstract_text {
            write_field(&mut buf, "AB", abstract_text);
        }

        if let Some(isbn) = &record.isbn {
            write_field(&mut buf, "IS", isbn);
        }

        buf.push('\n');
    }

    buf
}

fn write_field(buf: &mut String, tag: &str, value: &str) {
    let _ = writeln!(buf, "{tag:<4}- {value}");
}

fn format_date(date: PublicationDate) -> String {
    let month_name = date.month.and_then(month_to_abbrev);
    match (month_name, date.day) {
        (Some(m), Some(d)) => format!("{} {m} {d}", date.year),
        (Some(m), None) => format!("{} {m}", date.year),
        _ => format!("{}", date.year),
    }
}

fn month_to_abbrev(month: u8) -> Option<&'static str> {
    match month {
        1 => Some("Jan"),
        2 => Some("Feb"),
        3 => Some("Mar"),
        4 => Some("Apr"),
        5 => Some("May"),
        6 => Some("Jun"),
        7 => Some("Jul"),
        8 => Some("Aug"),
        9 => Some("Sep"),
        10 => Some("Oct"),
        11 => Some("Nov"),
        12 => Some("Dec"),
        _ => None,
    }
}

fn abbrev_to_month(s: &str) -> Option<u8> {
    let s = s.get(..3)?;
    match s.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn parse_date(value: &str) -> Option<PublicationDate> {
    let mut parts = value.split_whitespace();
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next().and_then(abbrev_to_month);
    let day = parts.next().and_then(|d| d.parse::<u8>().ok());
    Some(PublicationDate { year, month, day })
}

fn extract_doi(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .find(|c| c.ends_with(" [doi]"))
        .map(|c| c[..c.len() - 6].to_owned())
}

/// Parse a single `NBIB` tag line into its (tag, value) pair.
///
/// The `NBIB` line format is: a 2-4 character uppercase tag, left-justified in a 4-character field,
/// followed by `"- "` and the value. For example:
///
/// ```text
/// TI  - Article title here
/// PMID- 12345678
/// FAU - Doe, Jane
/// ```
///
/// The `winnow` parser consumes `input` left to right through a chain of `parse_next` calls. Each
/// call advances the input past what it matched or returns `Err` to signal the line isn't a valid
/// tag line.
fn tag_line<'a>(input: &mut &'a str) -> winnow::ModalResult<(&'a str, &'a str)> {
    // Consume the tag: 2-4 uppercase ASCII letters or digits (e.g. "TI", "PMID", "FAU").
    let tag = take_while(2..=4, |c: char| {
        c.is_ascii_uppercase() || c.is_ascii_digit()
    })
    .parse_next(input)?;
    // Skip optional padding spaces between tag and separator (e.g. "TI  " vs "PMID").
    space0.parse_next(input)?;
    // Match the literal separator.
    "- ".parse_next(input)?;
    // Everything remaining on the line is the value.
    let value = rest.parse_next(input)?;
    Ok((tag, value.trim_end()))
}

/// Whether the parser reads `tag` into a field, rather than keeping it in `extras`. The serialiser
/// never writes such a tag from `extras`.
fn is_read_tag(tag: &str) -> bool {
    matches!(
        tag,
        "TI" | "BTI"
            | "CTI"
            | "FAU"
            | "AU"
            | "DP"
            | "JT"
            | "LID"
            | "AID"
            | "PG"
            | "VI"
            | "IP"
            | "AB"
            | "IS"
    )
}

/// Whether `tag` has the shape of an `NBIB` tag: two to four uppercase ASCII letters or digits.
fn is_tag_shape(tag: &str) -> bool {
    (2..=4).contains(&tag.len())
        && tag
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// A record being read, before its fields are combined into a [`Record`].
#[derive(Default)]
struct PendingRecord {
    /// Line of the record's first tag.
    line_start: usize,
    title: Option<String>,
    /// Book title (`BTI`), the title of a book, which has no `TI`.
    book_title: Option<String>,
    fau_authors: Vec<String>,
    au_authors: Vec<String>,
    date: Option<PublicationDate>,
    journal: Option<String>,
    /// Collection title (`CTI`), the series a book belongs to. Used as the journal when there is
    /// no `JT`.
    collection_title: Option<String>,
    doi_candidates: Vec<String>,
    pages: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    abstract_text: Option<String>,
    isbn: Option<String>,
    /// Values of tags that are not read into a field, keyed `"nbib:{tag}"`.
    extras: HashMap<String, Vec<String>>,
    last_tag: Option<String>,
}

impl PendingRecord {
    fn dispatch(&mut self, tag: &str, value: &str) {
        match tag {
            "TI" => self.title = Some(value.to_owned()),
            "BTI" => self.book_title = Some(value.to_owned()),
            "CTI" => self.collection_title = Some(value.to_owned()),
            "FAU" => self.fau_authors.push(value.to_owned()),
            "AU" => self.au_authors.push(value.to_owned()),
            "DP" => self.date = parse_date(value),
            "JT" => self.journal = Some(value.to_owned()),
            "LID" | "AID" => self.doi_candidates.push(value.to_owned()),
            "PG" => self.pages = Some(value.to_owned()),
            "VI" => self.volume = Some(value.to_owned()),
            "IP" => self.number = Some(value.to_owned()),
            "AB" => self.abstract_text = Some(value.to_owned()),
            "IS" => self.isbn = Some(value.to_owned()),
            // Keep this list in step with `is_read_tag`.
            _ => push_extra(&mut self.extras, format!("nbib:{tag}"), value),
        }
        self.last_tag = Some(tag.to_owned());
    }

    /// Append a continuation line to the field of the previous tag. Returns `false` if there is no
    /// previous tag to continue; a tag whose field is not read still takes its continuation lines.
    fn append_continuation(&mut self, text: &str) -> bool {
        let Some(tag) = self.last_tag.as_deref() else {
            return false;
        };
        match tag {
            "TI" => append_to_opt(&mut self.title, text),
            "BTI" => append_to_opt(&mut self.book_title, text),
            "CTI" => append_to_opt(&mut self.collection_title, text),
            "AB" => append_to_opt(&mut self.abstract_text, text),
            "JT" => append_to_opt(&mut self.journal, text),
            "FAU" => append_to_last(&mut self.fau_authors, text),
            "AU" => append_to_last(&mut self.au_authors, text),
            "LID" | "AID" => append_to_last(&mut self.doi_candidates, text),
            _ => {
                if let Some(values) = self.extras.get_mut(&format!("nbib:{tag}")) {
                    append_to_last(values, text);
                }
            }
        }
        true
    }

    /// Combine the fields into a [`Record`], reporting a missing title.
    fn finish(self, diagnostics: &mut Vec<Diagnostic>) -> Record {
        let title = first_non_blank([self.title, self.book_title]);
        report_missing_title(title.as_deref(), self.line_start, diagnostics);

        let authors = if self.fau_authors.is_empty() {
            self.au_authors
        } else {
            self.fau_authors
        };

        let doi = extract_doi(&self.doi_candidates);

        Record {
            title,
            authors,
            date: self.date,
            journal: first_non_blank([self.journal, self.collection_title]),
            doi,
            pages: self.pages,
            volume: self.volume,
            number: self.number,
            abstract_text: self.abstract_text,
            isbn: self.isbn,
            database: None,
            label: None,
            extras: self.extras,
        }
    }
}
