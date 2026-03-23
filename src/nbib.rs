//! Parser and serializer for the NBIB (PubMed/MEDLINE) bibliographic format.
//!
//! # References
//!
//! - [PubMed Help](https://pubmed.ncbi.nlm.nih.gov/help/)
//! - [PubMed XML DTD (field definitions)](https://dtd.nlm.nih.gov/ncbi/pubmed/doc/out/250101/index.html)

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;

use winnow::Parser;
use winnow::ascii::space0;
use winnow::token::{rest, take_while};

use crate::{Error, PublicationDate, Record};

/// Parse NBIB-formatted text into zero or more [`Record`]s.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input is empty or whitespace-only.
/// Returns [`Error::MissingRequiredField`] if a record lacks a `TI` (title) field.
pub fn parse(input: &str) -> Result<Vec<Record>, Error> {
    if input.trim().is_empty() {
        return Err(Error::EmptyInput);
    }

    // Normalise Windows line endings. Uses Cow to avoid allocation when the
    // input already uses Unix endings (the common case).
    let input: Cow<'_, str> = if input.contains('\r') {
        Cow::Owned(input.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(input)
    };

    let mut records = Vec::new();
    let mut builder = RecordBuilder::default();
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
                records.push(builder.finish()?);
                builder = RecordBuilder::default();
                has_content = false;
            }
            continue;
        }

        // Continuation line – append to whatever field the previous tag started.
        if line.starts_with("      ") {
            builder.append_continuation(line.trim());
            continue;
        }

        // Tag line – parse the tag and value, then route to the appropriate field.
        if let Ok((tag, value)) = tag_line.parse(line) {
            if !has_content {
                builder.line_start = line_number;
                has_content = true;
            }
            builder.dispatch(tag, value);
        }
    }

    // The last record may not be followed by a blank line.
    if has_content {
        records.push(builder.finish()?);
    }

    Ok(records)
}

/// Serialize [`Record`]s to NBIB format.
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    let mut buf = String::new();

    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }

        let mut extras_sorted: Vec<_> = record.extras.iter().collect();
        extras_sorted.sort_by_key(|(k, _)| k.as_str());
        for (key, value) in extras_sorted {
            write_field(&mut buf, key, value);
        }

        write_field(&mut buf, "TI", &record.title);

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

/// Parse a single NBIB tag line into its (tag, value) pair.
///
/// The NBIB line format is: a 2-4 character uppercase tag, left-justified in a
/// 4-character field, followed by `"- "` and the value. For example:
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

#[derive(Default)]
struct RecordBuilder {
    line_start: usize,
    title: Option<String>,
    fau_authors: Vec<String>,
    au_authors: Vec<String>,
    date: Option<PublicationDate>,
    journal: Option<String>,
    doi_candidates: Vec<String>,
    pages: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    abstract_text: Option<String>,
    isbn: Option<String>,
    extras: HashMap<String, String>,
    last_tag: Option<String>,
}

impl RecordBuilder {
    fn dispatch(&mut self, tag: &str, value: &str) {
        match tag {
            "TI" => self.title = Some(value.to_owned()),
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
            "PMID" | "PMC" => {
                self.extras.insert(tag.to_owned(), value.to_owned());
            }
            _ => {}
        }
        self.last_tag = Some(tag.to_owned());
    }

    fn append_continuation(&mut self, text: &str) {
        let Some(tag) = self.last_tag.as_deref() else {
            return;
        };
        match tag {
            "TI" => append_to_opt(&mut self.title, text),
            "AB" => append_to_opt(&mut self.abstract_text, text),
            "JT" => append_to_opt(&mut self.journal, text),
            "FAU" => append_to_last_vec(&mut self.fau_authors, text),
            "AU" => append_to_last_vec(&mut self.au_authors, text),
            "LID" | "AID" => append_to_last_vec(&mut self.doi_candidates, text),
            _ => {}
        }
    }

    fn finish(self) -> Result<Record, Error> {
        let title = self.title.ok_or(Error::MissingRequiredField {
            tag: "TI",
            line: self.line_start,
        })?;

        let authors = if self.fau_authors.is_empty() {
            self.au_authors
        } else {
            self.fau_authors
        };

        let doi = extract_doi(&self.doi_candidates);

        Ok(Record {
            title,
            authors,
            date: self.date,
            journal: self.journal,
            doi,
            pages: self.pages,
            volume: self.volume,
            number: self.number,
            abstract_text: self.abstract_text,
            isbn: self.isbn,
            extras: self.extras,
        })
    }
}

fn append_to_opt(field: &mut Option<String>, text: &str) {
    if let Some(existing) = field {
        existing.push(' ');
        existing.push_str(text);
    }
}

fn append_to_last_vec(vec: &mut [String], text: &str) {
    if let Some(last) = vec.last_mut() {
        last.push(' ');
        last.push_str(text);
    }
}
