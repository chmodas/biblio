//! Parser and serializer for the `BibTeX`/`BibLaTeX` bibliographic format.
//!
//! Delegates parsing to the [`biblatex`](https://docs.rs/biblatex) crate and maps entries to the
//! unified [`Record`] type.
//!
//! # References
//!
//! - [BibTeX format](https://www.bibtex.com/g/bibtex-format/)
//! - [`BibLaTeX` guide](https://www.overleaf.com/learn/latex/Bibliography_management_with_biblatex)

use std::collections::HashMap;
use std::fmt::Write as _;

use biblatex::{Bibliography, ChunksExt, Date, DateValue, PermissiveType, Person};

use crate::parse_util::check_empty;
use crate::{Error, PublicationDate, Record};

/// Parse `BibTeX`/`BibLaTeX` text into zero or more [`Record`]s.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input is empty or whitespace-only.
/// Returns [`Error::MalformedSyntax`] if the input cannot be parsed.
/// Returns [`Error::MissingRequiredField`] if an entry has no title.
pub fn parse(input: &str) -> Result<Vec<Record>, Error> {
    check_empty(input)?;

    let bib = Bibliography::parse(input).map_err(|e| Error::MalformedSyntax {
        line: 0,
        message: e.to_string(),
    })?;

    bib.iter().map(record_from_entry).collect()
}

/// Serialize [`Record`]s to `BibTeX` format.
///
/// Each record is emitted as an `@article` entry. The citation key is taken from `extras["key"]`,
/// falling back to `"unknown"`.
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    let mut out = String::new();

    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }

        let key = record.extras.get("key").map_or("unknown", String::as_str);

        let _ = writeln!(out, "@article{{{key},");

        emit(&mut out, "title", &record.title);

        if !record.authors.is_empty() {
            emit(&mut out, "author", &record.authors.join(" and "));
        }

        if let Some(date) = record.date {
            emit(&mut out, "year", &date.year.to_string());
            if let Some(m) = date.month {
                emit(&mut out, "month", &m.to_string());
            }
        }

        if let Some(journal) = &record.journal {
            emit(&mut out, "journal", journal);
        }

        if let Some(doi) = &record.doi {
            emit(&mut out, "doi", doi);
        }

        if let Some(pages) = &record.pages {
            emit(&mut out, "pages", pages);
        }

        if let Some(volume) = &record.volume {
            emit(&mut out, "volume", volume);
        }

        if let Some(number) = &record.number {
            emit(&mut out, "number", number);
        }

        if let Some(abstract_text) = &record.abstract_text {
            emit(&mut out, "abstract", abstract_text);
        }

        if let Some(isbn) = &record.isbn {
            emit(&mut out, "isbn", isbn);
        }

        out.push_str("}\n");
    }

    out
}

fn emit(out: &mut String, field: &str, value: &str) {
    let _ = writeln!(out, "  {field} = {{{value}}},");
}

fn record_from_entry(entry: &biblatex::Entry) -> Result<Record, Error> {
    let mut extras = HashMap::new();
    extras.insert("key".into(), entry.key.clone());

    let title = entry
        .get("title")
        .map(ChunksExt::format_verbatim)
        .unwrap_or_default();

    if title.is_empty() {
        return Err(Error::MissingRequiredField {
            tag: "title",
            line: 0,
        });
    }

    let authors = entry
        .get_as::<Vec<Person>>("author")
        .ok()
        .map(|people| people.iter().map(format_person).collect())
        .unwrap_or_default();

    let date = parse_date(entry);

    let journal = entry
        .get("journaltitle")
        .or(entry.get("journal"))
        .map(ChunksExt::format_verbatim);

    let doi = entry.get_as::<String>("doi").ok();

    let pages = entry
        .get_as::<PermissiveType<Vec<std::ops::Range<u32>>>>("pages")
        .ok()
        .map(|p| match p {
            PermissiveType::Typed(ranges) => ranges
                .iter()
                .map(|r| {
                    if r.start == r.end {
                        r.start.to_string()
                    } else {
                        format!("{}-{}", r.start, r.end)
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            PermissiveType::Chunks(c) => c.format_verbatim(),
        });

    let volume = entry
        .get_as::<PermissiveType<i64>>("volume")
        .ok()
        .map(|v| match v {
            PermissiveType::Typed(n) => n.to_string(),
            PermissiveType::Chunks(c) => c.format_verbatim(),
        });

    let number = entry.get("number").map(ChunksExt::format_verbatim);
    let abstract_text = entry.get("abstract").map(ChunksExt::format_verbatim);
    let isbn = entry.get("isbn").map(ChunksExt::format_verbatim);

    Ok(Record {
        title,
        authors,
        date,
        journal,
        doi,
        pages,
        volume,
        number,
        abstract_text,
        isbn,
        extras,
    })
}

/// Format a [`Person`] as `"Last, First"` (the convention used by [`Record::authors`]).
fn format_person(person: &Person) -> String {
    let mut parts = String::new();
    if !person.prefix.is_empty() {
        parts.push_str(&person.prefix);
        parts.push(' ');
    }
    parts.push_str(&person.name);
    if !person.given_name.is_empty() {
        parts.push_str(", ");
        parts.push_str(&person.given_name);
    }
    if !person.suffix.is_empty() {
        parts.push_str(", ");
        parts.push_str(&person.suffix);
    }
    parts
}

/// Extract a [`PublicationDate`] from a biblatex entry.
///
/// Tries the `date` field first (ISO 8601 or free-text). Falls back to the `year` field. The
/// biblatex crate uses 0-indexed month and day internally, so both are incremented by 1 before
/// storing.
fn parse_date(entry: &biblatex::Entry) -> Option<PublicationDate> {
    if let Ok(date) = entry.get_as::<PermissiveType<Date>>("date") {
        return match date {
            PermissiveType::Typed(d) => {
                let dt = match d.value {
                    DateValue::At(dt)
                    | DateValue::After(dt)
                    | DateValue::Before(dt)
                    | DateValue::Between(dt, _) => dt,
                };
                // biblatex uses 0-indexed month and day.
                Some(PublicationDate {
                    year: dt.year,
                    month: dt.month.map(|m| m + 1),
                    day: dt.day.map(|d| d + 1),
                })
            }
            PermissiveType::Chunks(c) => {
                let s = c.format_verbatim();
                let year = s.split_whitespace().next()?.parse().ok()?;
                Some(PublicationDate {
                    year,
                    month: None,
                    day: None,
                })
            }
        };
    }

    let year_str = entry.get("year")?.format_verbatim();
    let year = year_str.trim().parse().ok()?;
    Some(PublicationDate {
        year,
        month: None,
        day: None,
    })
}
