//! Parser and serializer for the RIS bibliographic format.
//!
//! # References
//!
//! - [RIS format (Wikipedia)](https://en.wikipedia.org/wiki/RIS_(file_format))
//! - [RIS specification (gris docs)](https://gris.readthedocs.io/en/latest/specification.html)

use std::collections::HashMap;
use std::fmt::Write as _;

use winnow::Parser;
use winnow::token::{rest, take_while};

use crate::parse_util::{append_to_last, append_to_opt, check_empty, normalize_line_endings};
use crate::{Error, PublicationDate, Record};

/// Parse RIS-formatted text into zero or more [`Record`]s.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input is empty or whitespace-only.
/// Returns [`Error::MissingRequiredField`] if a record lacks a title (`TI` or `T1`).
pub fn parse(input: &str) -> Result<Vec<Record>, Error> {
    check_empty(input)?;
    let input = normalize_line_endings(input);

    let mut records = Vec::new();
    let mut builder: Option<RecordBuilder> = None;

    // Each RIS line is one of three kinds:
    //   1. Tag line:  "XX  - value"  (exactly 2 uppercase chars, two spaces, dash, space)
    //   2. End tag:   "ER  -"        (finalises the current record)
    //   3. Start tag: "TY  - TYPE"   (begins a new record)
    // Lines that don't match a tag pattern while inside a record are continuation lines.
    for (line_idx, line) in input.lines().enumerate() {
        let line_number = line_idx + 1;

        if line.trim().is_empty() {
            continue;
        }

        if let Ok((tag, value)) = tag_line.parse(line) {
            match tag {
                // TY starts a new record.
                "TY" => {
                    builder = Some(RecordBuilder {
                        line_start: line_number,
                        ..RecordBuilder::default()
                    });
                }
                // ER finalises the current record.
                "ER" => {
                    if let Some(b) = builder.take() {
                        records.push(b.finish()?);
                    }
                }
                // Any other tag — dispatch to the current builder.
                _ => {
                    if let Some(b) = builder.as_mut() {
                        b.dispatch(tag, value);
                    }
                }
            }
        } else if let Some(b) = builder.as_mut() {
            // Continuation line — append to the previous field.
            b.append_continuation(line.trim());
        }
    }

    // Handle a record that was never closed with ER.
    if let Some(b) = builder.take() {
        records.push(b.finish()?);
    }

    Ok(records)
}

/// Serialize [`Record`]s to RIS format.
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    let mut buf = String::new();

    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }

        write_tag(&mut buf, "TY", "JOUR");
        write_tag(&mut buf, "TI", &record.title);

        for author in &record.authors {
            write_tag(&mut buf, "AU", author);
        }

        if let Some(date) = record.date {
            write_tag(&mut buf, "PY", &format_date(date));
        }

        if let Some(journal) = &record.journal {
            write_tag(&mut buf, "JO", journal);
        }

        if let Some(doi) = &record.doi {
            write_tag(&mut buf, "DO", doi);
        }

        if let Some(pages) = &record.pages {
            // Split pages on hyphen or en-dash for SP/EP.
            let normalized = pages.replace('\u{2013}', "-");
            if let Some((sp, ep)) = normalized.split_once('-') {
                write_tag(&mut buf, "SP", sp.trim());
                write_tag(&mut buf, "EP", ep.trim());
            } else {
                write_tag(&mut buf, "SP", &normalized);
            }
        }

        if let Some(volume) = &record.volume {
            write_tag(&mut buf, "VL", volume);
        }

        if let Some(number) = &record.number {
            write_tag(&mut buf, "IS", number);
        }

        if let Some(abstract_text) = &record.abstract_text {
            write_tag(&mut buf, "AB", abstract_text);
        }

        if let Some(isbn) = &record.isbn {
            write_tag(&mut buf, "SN", isbn);
        }

        // Extras — sort keys for deterministic output.
        let mut extras_sorted: Vec<_> = record.extras.iter().collect();
        extras_sorted.sort_by_key(|(k, _)| k.as_str());
        for (key, value) in extras_sorted {
            write_tag(&mut buf, key, value);
        }

        write_tag(&mut buf, "ER", "");
    }

    buf
}

fn write_tag(buf: &mut String, tag: &str, value: &str) {
    if value.is_empty() {
        let _ = writeln!(buf, "{tag}  -");
    } else {
        let _ = writeln!(buf, "{tag}  - {value}");
    }
}

fn format_date(date: PublicationDate) -> String {
    let month = date.month.map_or(String::new(), |m| format!("{m:02}"));
    let day = date.day.map_or(String::new(), |d| format!("{d:02}"));
    format!("{}/{month}/{day}/", date.year)
}

fn parse_date(value: &str) -> Option<PublicationDate> {
    let mut parts = value.split('/');
    let year = parts.next()?.trim().parse::<i32>().ok()?;
    let month = parts
        .next()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .filter(|m| (1..=12).contains(m));
    let day = parts
        .next()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .filter(|d| (1..=31).contains(d));
    Some(PublicationDate { year, month, day })
}

/// Parse a single RIS tag line into its (tag, value) pair.
///
/// The RIS line format is exactly: 2 uppercase characters, two spaces, a dash,
/// a space, then the value. For example:
///
/// ```text
/// TI  - Article title here
/// AU  - Doe, Jane
/// ER  -
/// ```
///
/// The `winnow` parser consumes `input` left to right through a chain of
/// `parse_next` calls. Each call advances the input past what it matched or
/// returns `Err` to signal the line isn't a valid tag line.
fn tag_line<'a>(input: &mut &'a str) -> winnow::ModalResult<(&'a str, &'a str)> {
    // Consume exactly 2 uppercase ASCII characters for the tag (e.g. "TI", "AU", "ER").
    let tag = take_while(2..=2, |c: char| {
        c.is_ascii_uppercase() || c.is_ascii_digit()
    })
    .parse_next(input)?;
    // Match the literal separator "  - " (two spaces, dash, space).
    // For end-of-record "ER  -", the trailing space may be absent.
    "  -".parse_next(input)?;
    let value = rest.parse_next(input)?;
    let value = value.strip_prefix(' ').unwrap_or(value);
    Ok((tag, value.trim_end()))
}

#[derive(Default)]
struct RecordBuilder {
    line_start: usize,
    ti_title: Option<String>,
    t1_title: Option<String>,
    authors: Vec<String>,
    date: Option<PublicationDate>,
    journal: Option<String>,
    doi: Option<String>,
    start_page: Option<String>,
    end_page: Option<String>,
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
            "TI" => self.ti_title = Some(value.to_owned()),
            "T1" => self.t1_title = Some(value.to_owned()),
            "AU" | "A1" => self.authors.push(value.to_owned()),
            "PY" | "Y1" => self.date = parse_date(value),
            "JO" | "JF" => self.journal = Some(value.to_owned()),
            "DO" => self.doi = Some(value.to_owned()),
            "SP" => self.start_page = Some(value.to_owned()),
            "EP" => self.end_page = Some(value.to_owned()),
            "VL" => self.volume = Some(value.to_owned()),
            "IS" => self.number = Some(value.to_owned()),
            "AB" | "N2" => self.abstract_text = Some(value.to_owned()),
            "SN" => self.isbn = Some(value.to_owned()),
            "ID" => {
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
            "TI" => append_to_opt(&mut self.ti_title, text),
            "T1" => append_to_opt(&mut self.t1_title, text),
            "AB" | "N2" => append_to_opt(&mut self.abstract_text, text),
            "AU" | "A1" => append_to_last(&mut self.authors, text),
            _ => {}
        }
    }

    fn finish(self) -> Result<Record, Error> {
        // Prefer TI over T1.
        let title = self
            .ti_title
            .or(self.t1_title)
            .ok_or(Error::MissingRequiredField {
                tag: "TI",
                line: self.line_start,
            })?;

        // Combine SP + EP into a single pages string.
        let pages = match (self.start_page, self.end_page) {
            (Some(sp), Some(ep)) => Some(format!("{sp}-{ep}")),
            (Some(sp), None) => Some(sp),
            _ => None,
        };

        Ok(Record {
            title,
            authors: self.authors,
            date: self.date,
            journal: self.journal,
            doi: self.doi,
            pages,
            volume: self.volume,
            number: self.number,
            abstract_text: self.abstract_text,
            isbn: self.isbn,
            extras: self.extras,
        })
    }
}
