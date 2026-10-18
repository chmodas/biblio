//! Parser and serializer for the `BibTeX`/`BibLaTeX` bibliographic format.
//!
//! Delegates parsing to the [`biblatex`](https://docs.rs/biblatex) crate and maps entries to the
//! unified [`Record`] type.
//!
//! # References
//!
//! - [BibTeX format](https://www.bibtex.com/g/bibtex-format/)
//! - [`BibLaTeX` guide](https://www.overleaf.com/learn/latex/Bibliography_management_with_biblatex)

mod guard;

use std::collections::HashMap;
use std::fmt::Write as _;

use biblatex::{
    Bibliography, ChunksExt, Date, DateValue, EntryType, PermissiveType, Person, RawBibliography,
    RawEntry, Spanned,
};
use winnow::Parser;
use winnow::ascii::space0;
use winnow::token::{one_of, take_while};

use crate::parse_util::{
    check_empty, first_non_blank, own_extras, parsed, report_missing_title, strip_byte_order_mark,
};
use crate::{Diagnostic, Error, Parsed, PublicationDate, Record};

/// Parse `BibTeX`/`BibLaTeX` text into zero or more [`Record`]s.
///
/// Every entry that can be read is kept, even one without a title. Problems are reported as
/// diagnostics:
///
/// - [`Diagnostic::MissingTitle`]: an entry has no `title`, or only a blank one.
/// - [`Diagnostic::SkippedEntry`]: an entry or `@string` definition could not be parsed and was
///   dropped. This includes values whose braces nest more than 128 deep, which would overflow the
///   stack: the nesting of every `{` and `}` is counted, plain groups and escaped braces included,
///   in every field, verbatim ones such as `url` too.
///   When a `@string` is dropped, every definition of that name goes with it, and an entry using it
///   is skipped, or falls back to the month for a month name. A `crossref` to a skipped entry
///   inherits nothing.
///
///   `@string` definitions that refer to themselves, directly or through others, are dropped the
///   same way, unused ones included, and so are those whose chain of definitions is longer than
///   64, or whose use would cost more than 4,096 look-ups or expand to more than 1 MiB. Across
///   the input, the uses of `@string`s may cost at most about 4 million look-ups, half a billion
///   comparisons while finding definitions, and about 128 MiB of parsed text; an entry that would
///   go over that is skipped. `@string` names longer than 64 bytes are dropped too. If the whole
///   input cannot be read at once, entries are read again one by one with a fresh budget, so the
///   work is bounded by twice the budget. Real files stay far below these limits, but a very
///   large file that uses thousands of different `@string`s may have its later entries skipped.
/// - [`Diagnostic::ReferenceCycle`]: an entry's `crossref` or `xdata` led back to itself,
///   directly, through other entries or through `ids` aliases. Entries are searched in order, and
///   the field whose reference closes the cycle is removed in full; the entry is kept.
/// - [`Diagnostic::ReferenceTooDeep`]: resolving an entry's `crossref` and `xdata` would follow a
///   chain longer than 64, copy more than 4,096 entries or about 4 MiB, or go over the input's
///   budget of 262,144 copies and about 512 MiB; both fields are removed and the entry is kept.
///   Sizes are upper bounds on the parser's work, counting field names, the pieces text is parsed
///   into, and every `crossref` or `xdata` key read, whether or not it names an entry.
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
/// # Extras
///
/// Every field without a slot is kept in [`Record::extras`] as `"bibtex:{name}"` (names in lower
/// case), with the citation key as `"bibtex:@key"` and the entry type as `"bibtex:@type"`. Values
/// are read as the underlying parser formats them: macros are expanded (`month = jan` gives
/// `"January"`), braces are removed and whitespace is collapsed, so a value may change once when it
/// is first read. A `crossref` child's extras include the fields it inherits, and `crossref` itself
/// is kept; `xdata` is consumed while resolving and is not kept. A field that appears twice keeps
/// its last value. Fields that are read but lose to another are dropped: `issn` when `isbn` is
/// present, `journal` when `journaltitle` is present, `year` when `date` is present, and values of
/// `year` or `author` that cannot be read.
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
    let Ok(mut raw) = RawBibliography::parse(input) else {
        return false;
    };
    let entry_offsets: Vec<usize> = raw.entries.iter().map(|entry| entry.span.start).collect();
    let mut lines = lines_of(input, &entry_offsets);
    let string_offsets: Vec<usize> = raw
        .abbreviations
        .iter()
        .map(|pair| pair.key.span.start)
        .collect();
    let mut string_lines = lines_of(input, &string_offsets);
    let mut strings = guard::strings(&mut raw.abbreviations, &mut string_lines, diagnostics);
    strings.plan(&raw.entries);
    let mut budget = guard::Budget::default();
    guard::entries(
        &mut raw.entries,
        &mut lines,
        &strings,
        &mut budget,
        diagnostics,
    );
    raw.abbreviations = strings.needed(&raw.abbreviations, &raw.entries);
    guard::references(&mut raw, &lines, &strings, &mut budget, diagnostics);
    let Ok(bibliography) = Bibliography::from_raw(raw) else {
        return false;
    };
    // `from_raw` keeps the raw entries one-for-one and in order (it rejects a duplicate key rather
    // than replacing an entry), so each entry lines up with its line.
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
    let mut string_lines = Vec::new();
    let mut chunks: Vec<(usize, Vec<Spanned<RawEntry<'_>>>)> = Vec::new();
    for (line, chunk) in split_entries(input) {
        match RawBibliography::parse(chunk) {
            Ok(raw) => {
                // Spans are relative to the chunk.
                let offsets: Vec<usize> = raw
                    .abbreviations
                    .iter()
                    .map(|pair| pair.key.span.start)
                    .collect();
                string_lines.extend(lines_of(chunk, &offsets).into_iter().map(|l| line + l - 1));
                abbreviations.extend(raw.abbreviations);
                chunks.push((line, raw.entries));
            }
            Err(e) => diagnostics.push(Diagnostic::SkippedEntry {
                line,
                message: e.kind.to_string(),
            }),
        }
    }

    let mut strings = guard::strings(&mut abbreviations, &mut string_lines, diagnostics);
    strings.plan(chunks.iter().flat_map(|(_, entries)| entries));
    // One budget for the whole input, however it is split.
    let mut budget = guard::Budget::default();

    for (line, mut entries) in chunks {
        let mut lines = vec![line; entries.len()];
        guard::entries(&mut entries, &mut lines, &strings, &mut budget, diagnostics);
        if entries.is_empty() {
            continue;
        }
        let needed = strings.needed(&abbreviations, &entries);
        let mut raw = RawBibliography {
            preamble: String::new(),
            entries,
            abbreviations: needed,
        };
        guard::references(&mut raw, &lines, &strings, &mut budget, diagnostics);
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
/// The entry type and citation key come from `extras["bibtex:@type"]` and `extras["bibtex:@key"]`,
/// falling back to `article` and `unknown` when absent or not valid. Other `bibtex:` extras are
/// written as fields after the first-class fields, sorted by name, using only their first value;
/// names that are not valid field names, and fields the parser reads, are skipped. A `crossref`
/// that would close a cycle among the records written (to the entry itself, directly or through an
/// `ids` alias, or through other entries) is skipped too, because such an entry cannot be resolved.
/// Extras of other formats are not written.
///
/// Values are escaped so that braces, backslashes and `$` read back unchanged: `\`, `{` and `}`
/// everywhere, and `$` except in verbatim fields such as `url` and `doi`. The parser still turns
/// `--` into a dash and collapses whitespace.
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    let mut out = String::new();
    let cyclic = cyclic_crossrefs(records);

    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }

        let first_extra = |name: &str| first_extra(record, name);
        let entry_type = first_extra("bibtex:@type")
            .filter(|t| {
                is_identifier(t)
                    && !matches!(
                        t.to_ascii_lowercase().as_str(),
                        "comment" | "string" | "preamble"
                    )
            })
            .unwrap_or("article");
        let key = entry_key(record);

        let _ = writeln!(out, "@{entry_type}{{{key},");

        if let Some(title) = &record.title {
            emit(&mut out, "title", title);
        }

        if !record.authors.is_empty() {
            emit(&mut out, "author", &record.authors.join(" and "));
        }

        let mut wrote_month = false;
        if let Some(date) = record.date {
            emit(&mut out, "year", &date.year.to_string());
            if let Some(m) = date.month {
                emit(&mut out, "month", &m.to_string());
                wrote_month = true;
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

        for (name, values) in own_extras(&record.extras, "bibtex") {
            let Some(value) = values.first() else {
                continue;
            };
            if !is_identifier(name) || name.bytes().any(|b| b.is_ascii_uppercase()) {
                continue;
            }
            // Fields written above, or read back into them.
            if is_read_field(name) || (name == "month" && wrote_month) {
                continue;
            }
            // An entry on a crossref cycle cannot be resolved.
            if name == "crossref" && cyclic[i] {
                continue;
            }
            emit(&mut out, name, value);
        }

        out.push_str("}\n");
    }

    out
}

/// The first value of `record`'s extra `name`, if any.
fn first_extra<'a>(record: &'a Record, name: &str) -> Option<&'a str> {
    record
        .extras
        .get(name)
        .and_then(|values| values.first())
        .map(String::as_str)
}

/// The citation key written for `record`: `bibtex:@key` if it is a valid key, otherwise `unknown`.
fn entry_key(record: &Record) -> &str {
    first_extra(record, "bibtex:@key")
        .filter(|key| is_key(key))
        .unwrap_or("unknown")
}

/// The names `record` can be referred to by once written: its key and its `ids` aliases.
fn entry_names(record: &Record) -> impl Iterator<Item = &str> {
    let aliases = first_extra(record, "bibtex:ids")
        .into_iter()
        .flat_map(|ids| ids.split(',').map(str::trim));
    std::iter::once(entry_key(record)).chain(aliases)
}

/// For each record, whether the `crossref` it would write lies on a cycle among `records`, so that
/// resolving it would never end. Dropping the `crossref` of every record on a cycle breaks it.
fn cyclic_crossrefs(records: &[Record]) -> Vec<bool> {
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        for name in entry_names(record) {
            by_name.entry(name).or_insert(index);
        }
    }
    let crossref = |index: usize| first_extra(&records[index], "bibtex:crossref");

    (0..records.len())
        .map(|start| {
            let Some(target) = crossref(start) else {
                return false;
            };
            if entry_names(&records[start]).any(|name| name == target) {
                return true;
            }
            // Follow the chain; a cycle through `start` returns to it within `records.len()` steps.
            let mut next = by_name.get(target).copied();
            for _ in 0..records.len() {
                match next {
                    Some(index) if index == start => return true,
                    Some(index) => next = crossref(index).and_then(|t| by_name.get(t).copied()),
                    None => return false,
                }
            }
            false
        })
        .collect()
}

/// Fields whose value `biblatex` reads verbatim, without `$` math. Mirrors the crate's private
/// list, so `$` is not escaped in them.
const VERBATIM_FIELDS: [&str; 10] = [
    "file", "doi", "uri", "eprint", "verba", "verbb", "verbc", "pdf", "url", "urlraw",
];

/// Write one field, escaping the value so that it reads back unchanged.
fn emit(out: &mut String, field: &str, value: &str) {
    let verbatim = VERBATIM_FIELDS.contains(&field);
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '{' | '}') || (c == '$' && !verbatim) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    let _ = writeln!(out, "  {field} = {{{escaped}}},");
}

/// Whether the parser reads field `name` into a first-class field, or `biblatex` consumes it while
/// resolving (`xdata`), rather than keeping it in `extras`. The serialiser never writes such a
/// field from `extras`.
fn is_read_field(name: &str) -> bool {
    matches!(
        name,
        "title"
            | "author"
            | "date"
            | "year"
            | "journaltitle"
            | "journal"
            | "doi"
            | "pages"
            | "volume"
            | "number"
            | "abstract"
            | "isbn"
            | "issn"
            | "xdata"
    )
}

/// Whether `name` is an identifier `biblatex` accepts as an entry type or field name: not empty,
/// not starting with `:`, `<`, `-` or `>`, and free of whitespace, control characters and
/// `@{}"#'(),=%\~`.
fn is_identifier(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| !matches!(first, ':' | '<' | '-' | '>'))
        && !name.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    '@' | '{' | '}' | '"' | '#' | '\'' | '(' | ')' | ',' | '=' | '%' | '\\' | '~'
                )
        })
}

/// Whether `key` is a citation key `biblatex` accepts: free of `,`, `}`, whitespace and control
/// characters. The empty key is allowed.
fn is_key(key: &str) -> bool {
    !key.chars()
        .any(|c| c == ',' || c == '}' || c.is_whitespace() || c.is_control())
}

/// Map a parsed entry starting at `line` to a [`Record`], reporting a missing title.
fn record_from_entry(
    entry: &biblatex::Entry,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Record {
    let mut extras: HashMap<String, Vec<String>> = entry
        .fields
        .iter()
        .filter(|(name, _)| !is_read_field(name))
        .map(|(name, chunks)| (format!("bibtex:{name}"), vec![chunks.format_verbatim()]))
        .collect();
    extras.insert("bibtex:@key".into(), vec![entry.key.clone()]);
    let entry_type = match &entry.entry_type {
        // `Display` prints every unknown type as "unknown", losing its name.
        EntryType::Unknown(name) if !name.is_empty() => name.clone(),
        known => known.to_string(),
    };
    extras.insert("bibtex:@type".into(), vec![entry_type]);

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
