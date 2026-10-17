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

use biblatex::{
    Bibliography, ChunksExt, Date, DateValue, PermissiveType, Person, RawBibliography, RawEntry,
    Spanned,
};
use winnow::Parser;
use winnow::ascii::space0;
use winnow::token::{one_of, take_while};

use crate::parse_util::{
    check_empty, first_non_blank, parsed, report_missing_title, strip_byte_order_mark,
};
use crate::{Diagnostic, Error, Parsed, PublicationDate, Record};

/// Parse `BibTeX`/`BibLaTeX` text into zero or more [`Record`]s.
///
/// Every entry that can be read is kept, even one without a title. Problems are reported as
/// diagnostics:
///
/// - [`Diagnostic::MissingTitle`]: an entry has no `title`, or only a blank one.
/// - [`Diagnostic::SkippedEntry`]: an entry or `@string` definition could not be parsed and was
///   dropped.
///
/// The whole input is first parsed at once, which resolves `@string` macros and `crossref`
/// inheritance. If that fails, the input is split into entries at lines starting with `@type{` or
/// `@type(` and each entry is parsed on its own, with the `@string` macros of every entry that
/// could be read. In that case `crossref` inheritance is lost for the whole input. The whole-input
/// parse fails on a broken entry, a duplicate citation key (or an `ids` alias that clashes with a
/// key), or an undefined macro; on the per-entry path, entries with duplicate keys are all kept.
///
/// Text outside entries is a `BibTeX` comment and is not reported. Entries written with
/// parentheses, `@type(...)`, are not supported and are skipped. A stray `@word` in text between
/// entries makes the entry before it fail.
///
/// A leading byte-order mark is ignored.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input, after any leading byte-order mark, is empty or
/// whitespace-only.
///
/// # Identifiers
///
/// The identifier is read from `isbn`, falling back to `issn`. A blank `isbn` never hides an `issn`
/// with a value. The value is trimmed but not split or normalised, so a field listing several
/// identifiers stays one string; the underlying parser still collapses whitespace and turns `--`
/// into a dash.
pub fn parse(input: &str) -> Result<Parsed, Error> {
    let input = strip_byte_order_mark(input);
    check_empty(input)?;

    let mut records = Vec::new();
    let mut diagnostics = Vec::new();
    if !parse_whole(input, &mut records, &mut diagnostics) {
        records.clear();
        diagnostics.clear();
        parse_entries(input, &mut records, &mut diagnostics);
    }
    Ok(parsed(records, diagnostics))
}

/// Parse the whole input at once. Returns `false`, leaving partial results behind, if any entry
/// fails to parse or resolve.
fn parse_whole(input: &str, records: &mut Vec<Record>, diagnostics: &mut Vec<Diagnostic>) -> bool {
    let Ok(raw) = RawBibliography::parse(input) else {
        return false;
    };
    let offsets: Vec<usize> = raw.entries.iter().map(|entry| entry.span.start).collect();
    let Ok(bibliography) = Bibliography::from_raw(raw) else {
        return false;
    };
    // `from_raw` keeps the raw entries one-for-one and in order (it rejects a duplicate key rather
    // than replacing an entry), so each entry lines up with its offset.
    let lines = lines_of(input, &offsets);
    for (entry, line) in bibliography.iter().zip(lines) {
        records.push(record_from_entry(entry, line, diagnostics));
    }
    true
}

/// Parse each entry of the input on its own, skipping those that fail.
///
/// Every chunk is first parsed raw, so that the `@string` macros of every chunk that can be read
/// are known; then each chunk's entries are resolved with all of those macros.
fn parse_entries(input: &str, records: &mut Vec<Record>, diagnostics: &mut Vec<Diagnostic>) {
    let mut abbreviations = Vec::new();
    let mut chunks: Vec<(usize, Vec<Spanned<RawEntry<'_>>>)> = Vec::new();
    for (line, chunk) in split_entries(input) {
        match RawBibliography::parse(chunk) {
            Ok(raw) => {
                abbreviations.extend(raw.abbreviations);
                chunks.push((line, raw.entries));
            }
            Err(e) => diagnostics.push(Diagnostic::SkippedEntry {
                line,
                message: e.kind.to_string(),
            }),
        }
    }

    for (line, entries) in chunks {
        if entries.is_empty() {
            continue;
        }
        let raw = RawBibliography {
            preamble: String::new(),
            entries,
            abbreviations: abbreviations.clone(),
        };
        match Bibliography::from_raw(raw) {
            Ok(bibliography) => {
                for entry in bibliography.iter() {
                    records.push(record_from_entry(entry, line, diagnostics));
                }
            }
            Err(e) => diagnostics.push(Diagnostic::SkippedEntry {
                line,
                message: e.kind.to_string(),
            }),
        }
    }
}

/// Split the input into entries, returning each entry's one-indexed starting line and text.
///
/// An entry starts at a line whose first non-blank text is `@`, a type name, and `{` or `(`.
/// Splitting at line starts rather than by brace depth stops an unclosed brace from swallowing the
/// entries after it. Text before the first such line becomes a chunk of its own if it contains an
/// `@`, so an entry hidden there (after other text on its line, or with its brace on the next line)
/// is still parsed or reported; text without an `@` is a comment.
fn split_entries(input: &str) -> Vec<(usize, &str)> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for (index, line) in input.split_inclusive('\n').enumerate() {
        if is_entry_start(line.trim_start()) {
            starts.push((index + 1, offset));
        }
        offset += line.len();
    }
    let first = starts.first().map_or(input.len(), |&(_, start)| start);
    if let Some(at) = input[..first].find('@') {
        starts.insert(0, (1 + input[..at].matches('\n').count(), 0));
    }
    let ends = starts
        .iter()
        .skip(1)
        .map(|&(_, start)| start)
        .chain([input.len()]);
    starts
        .iter()
        .zip(ends)
        .map(|(&(line, start), end)| (line, &input[start..end]))
        .collect()
}

/// Whether `text` begins with `@`, a type name, optional spaces, and `{` or `(`.
fn is_entry_start(mut text: &str) -> bool {
    let start: winnow::ModalResult<_> = (
        '@',
        take_while(1.., |c: char| c.is_ascii_alphabetic()),
        space0,
        one_of(['{', '(']),
    )
        .parse_next(&mut text);
    start.is_ok()
}

/// Convert ascending byte offsets into one-indexed line numbers, in one pass over the input.
fn lines_of(input: &str, offsets: &[usize]) -> Vec<usize> {
    let mut line = 1;
    let mut counted = 0;
    offsets
        .iter()
        .map(|&offset| {
            line += input[counted..offset].matches('\n').count();
            counted = offset;
            line
        })
        .collect()
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

        if let Some(title) = &record.title {
            emit(&mut out, "title", title);
        }

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

/// Map a parsed entry starting at `line` to a [`Record`], reporting a missing title.
fn record_from_entry(
    entry: &biblatex::Entry,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Record {
    let mut extras = HashMap::new();
    extras.insert("key".into(), entry.key.clone());

    let title = entry.get("title").map(ChunksExt::format_verbatim);
    report_missing_title(title.as_deref(), line, diagnostics);

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
    let field = |name| {
        entry
            .get(name)
            .map(|chunks| chunks.format_verbatim().trim().to_owned())
    };
    let isbn = first_non_blank([field("isbn"), field("issn")]);

    Record {
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
    }
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
