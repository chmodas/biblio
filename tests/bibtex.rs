mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::subsequence;
use std::collections::HashMap;
use std::fmt::Write as _;
use test_strategy::{Arbitrary, proptest};

use biblio::bibtex::{parse, serialize};
use biblio::{Diagnostic, Error, PublicationDate, Record};

use common::{arb_date, first_extra};

// -- Core parsing --

#[test]
fn single_entry() {
    let input = r"
@article{doe2024,
  title = {A test title},
  author = {Doe, Jane and Smith, John},
  year = {2024},
  journal = {Nature},
  doi = {10.1038/s41586-024-07386-0},
  pages = {123--130},
  volume = {620},
  number = {3},
  abstract = {This is the abstract text.},
  isbn = {1234-5678},
}
";

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 1);

    let r = &records[0];
    assert_eq!(r.title.as_deref(), Some("A test title"));
    assert_eq!(r.authors, vec!["Doe, Jane", "Smith, John"]);
    assert_eq!(
        r.date,
        Some(PublicationDate {
            year: 2024,
            month: None,
            day: None,
        })
    );
    assert_eq!(r.journal.as_deref(), Some("Nature"));
    assert_eq!(r.doi.as_deref(), Some("10.1038/s41586-024-07386-0"));
    assert_eq!(r.pages.as_deref(), Some("123-130"));
    assert_eq!(r.volume.as_deref(), Some("620"));
    assert_eq!(r.number.as_deref(), Some("3"));
    assert_eq!(
        r.abstract_text.as_deref(),
        Some("This is the abstract text.")
    );
    assert_eq!(r.isbn.as_deref(), Some("1234-5678"));
    assert_eq!(first_extra(r, "bibtex:@key"), Some("doe2024"));
}

#[test]
fn multiple_entries() {
    let input = r"
@article{frangoul2024,
  title = {CRISPR-Cas9 gene editing},
  author = {Frangoul, Haydar and Altshuler, David},
  year = {2024},
  journal = {The New England Journal of Medicine},
  doi = {10.1056/NEJMoa2031054},
  pages = {175--186},
  volume = {390},
}

@article{lee2024,
  title = {Large language models in clinical medicine},
  author = {Lee, Peter and Bubeck, Sebastien},
  year = {2024},
  journal = {Nature Medicine},
}
";

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].title.as_deref(),
        Some("CRISPR-Cas9 gene editing")
    );
    assert_eq!(
        first_extra(&records[0], "bibtex:@key"),
        Some("frangoul2024")
    );
    assert_eq!(
        records[1].title.as_deref(),
        Some("Large language models in clinical medicine")
    );
}

#[test]
fn misc_entry_type() {
    let input = r"
@misc{website2024,
  title = {A website resource},
  year = {2024},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A website resource"));
    assert_eq!(records[0].authors, Vec::<String>::new());
}

// -- Authors --

#[test]
fn no_authors() {
    let input = r"
@article{anon2024,
  title = {Anonymous work},
  year = {2024},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors, Vec::<String>::new());
}

#[test]
fn author_with_prefix() {
    let input = r"
@article{von2024,
  title = {Test},
  author = {von Neumann, John},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors, vec!["von Neumann, John"]);
}

#[test]
fn author_name_only() {
    let input = r"
@article{cern2024,
  title = {Test},
  author = {{CERN}},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors.len(), 1);
    assert!(!records[0].authors[0].contains(','));
}

#[test]
fn author_with_suffix() {
    let input = r"
@article{king2024,
  title = {Test},
  author = {King, Jr., Martin Luther},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors.len(), 1);
    assert!(records[0].authors[0].contains("Jr."));
}

// -- Dates --

#[test]
fn date_year_only() {
    let input = r"
@article{test,
  title = {Test},
  year = {2024},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2024,
            month: None,
            day: None,
        })
    );
}

#[test]
fn date_full_iso() {
    let input = r"
@article{test,
  title = {Test},
  date = {2024-09-01},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2024,
            month: Some(9),
            day: Some(1),
        })
    );
}

#[test]
fn date_between() {
    let input = r"
@article{test,
  title = {Test},
  date = {2020/2024},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].date.unwrap().year, 2020);
}

#[test]
fn date_after() {
    let input = r"
@article{test,
  title = {Test},
  date = {2020/..},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].date.unwrap().year, 2020);
}

#[test]
fn date_before() {
    let input = r"
@article{test,
  title = {Test},
  date = {../2024},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].date.unwrap().year, 2024);
}

#[test]
fn date_unparseable_chunks() {
    let input = r"
@article{test,
  title = {Test},
  date = {2020 approximate},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2020,
            month: None,
            day: None,
        })
    );
}

/// A `date` that is not a date, either blank or not starting with a year, gives no date.
#[test]
fn date_without_year_ignored() {
    for date in ["", " ", "in press"] {
        let input = format!("@article{{k, title = {{T}}, date = {{{date}}}}}\n");
        let records = parse(&input).unwrap().records;
        assert_eq!(records[0].date, None, "date = {{{date}}}");
    }
}

/// A `year` that is not a number gives no date.
#[test]
fn year_not_a_number_ignored() {
    let records = parse("@article{k, title = {T}, year = {in press}}\n")
        .unwrap()
        .records;
    assert_eq!(records[0].date, None);
}

// -- Pages --

#[test]
fn pages_typed_range() {
    let input = r"
@article{test,
  title = {Test},
  pages = {123--130},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].pages.as_deref(), Some("123-130"));
}

#[test]
fn pages_chunks_verbatim() {
    let input = r"
@article{test,
  title = {Test},
  pages = {S12--S18},
}
";
    let records = parse(input).unwrap().records;
    // Non-numeric pages fall back to chunks verbatim.
    assert!(records[0].pages.is_some());
}

// -- Volume --

#[test]
fn volume_non_numeric() {
    let input = r"
@article{test,
  title = {Test},
  volume = {S1},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].volume.as_deref(), Some("S1"));
}

// -- Journal --

#[test]
fn journaltitle_preferred() {
    let input = r"
@article{test,
  title = {Test},
  journaltitle = {Nature Medicine},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].journal.as_deref(), Some("Nature Medicine"));
}

#[test]
fn journal_fallback() {
    let input = r"
@article{test,
  title = {Test},
  journal = {Science},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].journal.as_deref(), Some("Science"));
}

// -- Identifiers --

/// Parse a single entry whose fields, after the title, are `field_lines`, and return its
/// identifier.
fn identifier_of(field_lines: &str) -> Option<String> {
    let input = format!("@article{{k,\n  title = {{t}},\n{field_lines}}}\n");
    parse(&input).unwrap().records.remove(0).isbn
}

/// An entry with only an ISSN gets it as its identifier.
#[test]
fn issn_read_when_no_isbn() {
    assert_eq!(
        identifier_of("  issn = {1234-5678},\n").as_deref(),
        Some("1234-5678")
    );
}

/// The ISBN is preferred over the ISSN, whichever comes first.
#[test]
fn isbn_preferred_over_issn() {
    let isbn_first = identifier_of("  isbn = {978-3-16-148410-0},\n  issn = {1234-5678},\n");
    let isbn_last = identifier_of("  issn = {1234-5678},\n  isbn = {978-3-16-148410-0},\n");
    assert_eq!(isbn_first.as_deref(), Some("978-3-16-148410-0"));
    assert_eq!(isbn_last.as_deref(), Some("978-3-16-148410-0"));
}

/// An empty or whitespace-only ISBN does not hide an ISSN that has a value.
#[test]
fn blank_isbn_does_not_hide_issn() {
    let empty = identifier_of("  isbn = {},\n  issn = {1234-5678},\n");
    let spaces = identifier_of("  isbn = { },\n  issn = {1234-5678},\n");
    assert_eq!(empty.as_deref(), Some("1234-5678"));
    assert_eq!(spaces.as_deref(), Some("1234-5678"));
}

/// Field names are case-insensitive, so `ISSN` is read like `issn`.
#[test]
fn issn_field_name_case_insensitive() {
    assert_eq!(
        identifier_of("  ISSN = {1234-5678},\n").as_deref(),
        Some("1234-5678")
    );
}

/// A field listing several ISSNs is kept as one string, not split.
#[test]
fn several_issns_kept_verbatim() {
    let identifier = identifier_of("  issn = {1234-5678, 2345-6789},\n");
    assert_eq!(identifier.as_deref(), Some("1234-5678, 2345-6789"));
}

// -- Lenient parsing --

/// Titles of the parsed records, in order.
fn titles(parsed: &biblio::Parsed) -> Vec<Option<&str>> {
    parsed.records.iter().map(|r| r.title.as_deref()).collect()
}

/// A broken entry between two valid ones is skipped and reported at its line; the others are kept.
#[test]
fn broken_entry_between_valid_ones_skipped() {
    let input = "@article{a, title = {A}}\n@article{b title = {B}}\n@article{c, title = {C}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("C")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 2, .. }]
    ));
}

/// The message of a skipped entry describes the problem without byte offsets into the input.
#[test]
fn skipped_entry_message_has_no_offsets() {
    let parsed = parse("@article{b title = {B}}\n").unwrap();
    let [Diagnostic::SkippedEntry { message, .. }] = parsed.diagnostics.as_slice() else {
        panic!("expected one skipped entry, got {:?}", parsed.diagnostics);
    };
    assert_ne!(message.as_str(), "");
    assert!(
        !message.ends_with(|c: char| c.is_ascii_digit()),
        "{message}"
    );
}

/// An unclosed brace does not swallow the entries after it.
#[test]
fn unclosed_brace_does_not_swallow_next_entry() {
    let input = "@article{a, title = {never closed\n@article{b, title = {B}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// `@string` macros still expand when entries are parsed one by one.
#[test]
fn string_macros_expand_after_a_broken_entry() {
    let input = "\
@string{jn = {Nature}}
@article{a, title = {A}, journal = jn}
@article{b, title = {B}
";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A")]);
    assert_eq!(parsed.records[0].journal.as_deref(), Some("Nature"));
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 3, .. }]
    ));
}

/// Entries with the same citation key are all kept, without a diagnostic.
#[test]
fn duplicate_keys_keep_every_entry() {
    let parsed = parse("@article{a, title = {One}}\n@article{a, title = {Two}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("One"), Some("Two")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A broken `@string` is reported, and the macros that could be read still expand.
#[test]
fn broken_string_skipped_others_expand() {
    let input = "\
@string{good = {Nature}}
@string{bad = {never closed}
@article{a, title = {A}, journal = good}
";
    let parsed = parse(input).unwrap();
    assert_eq!(parsed.records[0].journal.as_deref(), Some("Nature"));
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 2, .. }]
    ));
}

/// An entry that uses an undefined macro is skipped; the other entries are kept.
#[test]
fn undefined_macro_skips_only_its_entry() {
    let input = "@article{a, title = {A}, journal = nope}\n@article{b, title = {B}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// An entry after other text on the first line is still read when the per-entry path is taken
/// (here forced by a duplicate key). Regression test from code review.
#[test]
fn entry_after_leading_text_kept() {
    let parsed = parse("See @article{a, title = {A}}\n@article{a, title = {B}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("B")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// An entry after a byte-order mark is still read when the per-entry path is taken. Regression
/// test from code review.
#[test]
fn entry_after_byte_order_mark_kept() {
    let parsed = parse("\u{feff}@article{a, title = {A}}\n@article{a, title = {B}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("B")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// An entry whose brace is on the line after its type is still read when the per-entry path is
/// taken. Regression test from code review.
#[test]
fn entry_with_brace_on_next_line_kept() {
    let input = "@article\n{a, title = {A}}\n@article{b, title = {B}\n@article{c, title = {C}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("C")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 3, .. }]
    ));
}

/// Entries written with parentheses are not supported and are skipped.
#[test]
fn parenthesised_entry_skipped() {
    let parsed = parse("@article(a, title = {A})\n@article{b, title = {B}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

// -- Extras --

/// Fields without a slot are kept under `bibtex:`, with the citation key and entry type as
/// `bibtex:@key` and `bibtex:@type`.
#[test]
fn unread_fields_kept_in_extras() {
    let input = "@book{k1, title = {T}, note = {A note}, Publisher = {Pub}, keywords = {a, b}}\n";
    let records = parse(input).unwrap().records;
    let extras = &records[0].extras;
    assert_eq!(extras["bibtex:@key"], vec!["k1"]);
    assert_eq!(extras["bibtex:@type"], vec!["book"]);
    assert_eq!(extras["bibtex:note"], vec!["A note"]);
    // Field names are lower-cased.
    assert_eq!(extras["bibtex:publisher"], vec!["Pub"]);
    assert_eq!(extras["bibtex:keywords"], vec!["a, b"]);
    assert_eq!(extras.len(), 5);
}

/// An entry type the parser does not know keeps its name, and `@unknown` reads as "unknown".
#[test]
fn unknown_entry_types_keep_their_name() {
    let input = "@dataset{a, title = {A}}\n@unknown{b, title = {B}}\n";
    let records = parse(input).unwrap().records;
    assert_eq!(first_extra(&records[0], "bibtex:@type"), Some("dataset"));
    assert_eq!(first_extra(&records[1], "bibtex:@type"), Some("unknown"));
}

/// A month macro is expanded to the month's name.
#[test]
fn month_macro_kept_as_name() {
    let records = parse("@article{k, title = {T}, month = jan}\n")
        .unwrap()
        .records;
    assert_eq!(records[0].extras["bibtex:month"], vec!["January"]);
}

/// Fields a `crossref` child inherits from its parent are kept in the child's extras, along with
/// the `crossref` itself.
#[test]
fn crossref_inheritance_kept_in_extras() {
    let input = "\
@book{parent, title = {P}, publisher = {Pub}}
@incollection{child, title = {C}, crossref = {parent}}
";
    let records = parse(input).unwrap().records;
    let child = &records[1];
    assert_eq!(child.extras["bibtex:crossref"], vec!["parent"]);
    assert_eq!(child.extras["bibtex:publisher"], vec!["Pub"]);
}

#[test]
fn extras_citation_key() {
    let input = r"
@article{mykey2024,
  title = {Test},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(first_extra(&records[0], "bibtex:@key"), Some("mykey2024"));
}

// -- Error cases --

#[test]
fn empty_input() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("   \n\n  "), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff}"), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff} \n\t"), Err(Error::EmptyInput));
}

/// A leading byte-order mark is ignored. This already worked before the mark was stripped
/// explicitly; the test guards it.
#[test]
fn leading_byte_order_mark_ignored() {
    let input = "@article{a, title = {First}}\n";
    let parsed = parse(&format!("\u{feff}{input}")).unwrap();
    assert_eq!(parsed, parse(input).unwrap());
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title.as_deref(), Some("First"));
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A broken entry is skipped and reported at its line instead of failing the whole input.
#[test]
fn malformed_entry_skipped() {
    let parsed = parse("@article{broken, title = {missing close brace}").unwrap();
    assert_eq!(parsed.records, Vec::<Record>::new());
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// An entry without a title is kept, and the missing title is reported at the entry's line.
#[test]
fn missing_title_kept_and_reported() {
    let input = r"
@article{x,
  year = {2024},
}
";
    let parsed = parse(input).unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title, None);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 2 }]
    );
}

// -- Serialization --

/// The entry type, key and extra fields are written back, so they read back unchanged.
#[test]
fn serialize_writes_type_key_and_extras() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:@type", "book")
        .extra("bibtex:@key", "k1")
        .extra("bibtex:note", "A note")
        .build();
    let output = serialize(std::slice::from_ref(&record));
    assert!(output.starts_with("@book{k1,\n"), "{output}");
    assert!(output.contains("  note = {A note},\n"));
    assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Only `bibtex:` extras are written, and only the first value of each.
#[test]
fn serialize_writes_only_bibtex_extras_once() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:note", "first")
        .extra("bibtex:note", "second")
        .extra("ris:KW", "cats")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("note = {first}"));
    assert!(!output.contains("second"));
    assert!(!output.contains("cats"));
}

/// Extras that are not valid field names, name a field the parser reads, or cannot be resolved,
/// are not written; an invalid type or key falls back to the default.
#[test]
fn serialize_skips_unwritable_extras() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:@type", "a b")
        .extra("bibtex:@key", "has space")
        .extra("bibtex:Bad Name", "x")
        .extra("bibtex:xdata", "x")
        .extra("bibtex:journal", "would become the journal")
        .build();
    assert_eq!(
        serialize(&[record]),
        "@article{unknown,\n  title = {T},\n}\n"
    );

    let comment = Record::builder()
        .extra("bibtex:@type", "comment")
        .extra("bibtex:@key", "self")
        .extra("bibtex:crossref", "self")
        .build();
    assert_eq!(serialize(&[comment]), "@article{self,\n}\n");
}

/// A month from the date is written instead of a `bibtex:month` extra, so the field appears once.
#[test]
fn date_month_wins_over_month_extra() {
    let record = Record::builder()
        .title("T")
        .date(PublicationDate {
            year: 2024,
            month: Some(3),
            day: None,
        })
        .extra("bibtex:month", "January")
        .build();
    let output = serialize(&[record]);
    assert_eq!(output.matches("month =").count(), 1, "{output}");
    assert!(output.contains("month = {3}"));
}

/// A `crossref` that would close a cycle is not written, whether the cycle runs through another
/// entry or through the entry's own `ids` alias, so the output can always be read back. A
/// `crossref` that only leads into other entries is kept.
#[test]
fn crossref_cycles_not_written() {
    let entry = |key: &str, crossref: &str| {
        Record::builder()
            .title(key)
            .extra("bibtex:@key", key)
            .extra("bibtex:crossref", crossref)
            .build()
    };
    let pair = serialize(&[entry("a", "b"), entry("b", "a")]);
    assert!(!pair.contains("crossref"), "{pair}");
    assert_eq!(parse(&pair).unwrap().records.len(), 2);

    let alias = Record::builder()
        .title("A")
        .extra("bibtex:@key", "a")
        .extra("bibtex:ids", "x, b")
        .extra("bibtex:crossref", "b")
        .build();
    let output = serialize(&[alias]);
    assert!(!output.contains("crossref"), "{output}");
    assert_eq!(parse(&output).unwrap().records.len(), 1);

    let parent = Record::builder()
        .title("P")
        .extra("bibtex:@key", "p")
        .build();
    let chain = serialize(&[entry("c", "p"), parent]);
    assert!(chain.contains("crossref = {p}"), "{chain}");
}

/// An extra with no values writes nothing.
#[test]
fn serialize_skips_extras_without_values() {
    let mut record = Record::builder().title("T").build();
    record.extras.insert("bibtex:note".to_owned(), Vec::new());
    assert_eq!(
        serialize(&[record]),
        "@article{unknown,\n  title = {T},\n}\n"
    );
}

/// An entry whose `crossref` leads into a cycle it is not part of keeps its `crossref`; only the
/// entries on the cycle lose theirs.
#[test]
fn crossref_into_a_cycle_kept() {
    let entry = |key: &str, crossref: &str| {
        Record::builder()
            .title(key)
            .extra("bibtex:@key", key)
            .extra("bibtex:crossref", crossref)
            .build()
    };
    let output = serialize(&[entry("c", "a"), entry("a", "b"), entry("b", "a")]);
    let records = parse(&output).unwrap().records;
    let crossrefs: Vec<_> = records
        .iter()
        .map(|r| first_extra(r, "bibtex:crossref"))
        .collect();
    assert_eq!(crossrefs, [Some("a"), None, None], "{output}");
}

/// A field name with an uppercase letter is not written, even when it is otherwise valid, because
/// the parser lower-cases names and it would read back under a different key.
#[test]
fn serialize_skips_uppercase_field_names() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:Note", "x")
        .build();
    assert_eq!(
        serialize(&[record]),
        "@article{unknown,\n  title = {T},\n}\n"
    );
}

/// A `bibtex:month` extra is written when the date has no month to write instead, or there is no
/// date at all.
#[test]
fn month_extra_written_without_date_month() {
    let year_only = Record::builder()
        .title("T")
        .date(PublicationDate {
            year: 2024,
            month: None,
            day: None,
        })
        .extra("bibtex:month", "March")
        .build();
    assert!(serialize(&[year_only]).contains("month = {March}"));

    let no_date = Record::builder()
        .title("T")
        .extra("bibtex:month", "March")
        .build();
    assert!(serialize(&[no_date]).contains("month = {March}"));
}

/// The special entry types `@comment`, `@string` and `@preamble`, in any case, are never written as
/// the entry type, because the entry would vanish or fail to parse.
#[test]
fn special_entry_types_fall_back_to_article() {
    for entry_type in ["comment", "string", "preamble", "Comment", "PREAMBLE"] {
        let record = Record::builder()
            .title("T")
            .extra("bibtex:@type", entry_type)
            .build();
        let output = serialize(&[record]);
        assert!(output.starts_with("@article{"), "{entry_type}: {output}");
    }
}

/// A field name starting with a character the parser does not accept at the start of a name is not
/// written; the same character later in the name is fine.
#[test]
fn field_names_with_invalid_first_character_skipped() {
    for first in [':', '<', '-', '>'] {
        let record = Record::builder()
            .title("T")
            .extra(format!("bibtex:{first}x"), "skipped")
            .extra(format!("bibtex:x{first}y"), "kept")
            .build();
        let output = serialize(&[record]);
        assert!(!output.contains("skipped"), "{first}: {output}");
        assert!(
            output.contains(&format!("x{first}y = {{kept}}")),
            "{first}: {output}"
        );
    }
}

/// Values are escaped so that braces, backslashes and dollar signs read back unchanged, except
/// that `$` is left alone in verbatim fields such as `url`.
#[test]
fn serialize_escapes_values() {
    let record = Record::builder()
        .title("A {braced} title")
        .extra("bibtex:@key", "k")
        .extra("bibtex:@type", "article")
        .extra("bibtex:note", "costs $5, ends with \\")
        .extra("bibtex:url", "https://example.org/$id")
        .build();
    let output = serialize(std::slice::from_ref(&record));
    assert!(
        output.contains("url = {https://example.org/$id}"),
        "{output}"
    );
    assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Converting `BibTeX` to `RIS` writes no `BibTeX` key or fields into the `RIS`, so it reads back
/// cleanly. Regression test: the citation key used to be written as a `key  -` line, which read
/// back as part of the previous field.
#[test]
fn bibtex_to_ris_writes_no_foreign_tags() {
    let input = "\
@article{doe2024,
  title  = {An interesting finding},
  author = {Doe, Jane and Smith, John},
  year   = {2024},
  journal = {Nature},
  note   = {A note},
}
";
    let records = parse(input).unwrap().records;
    let ris_output = biblio::ris::serialize(&records);
    assert!(!ris_output.contains("doe2024"), "{ris_output}");
    assert!(!ris_output.contains("A note"));
    let reparsed = biblio::ris::parse(&ris_output).unwrap();
    assert_eq!(reparsed.diagnostics, Vec::<Diagnostic>::new());
    assert_eq!(reparsed.records[0].title, records[0].title);
    assert_eq!(reparsed.records[0].journal, records[0].journal);
}

/// Several entries with distinct keys are written and read back whole, keys included.
#[test]
fn serialize_multiple_entries() {
    let a = Record::builder()
        .title("First")
        .extra("bibtex:@key", "a")
        .build();
    let b = Record::builder()
        .title("Second")
        .extra("bibtex:@key", "b")
        .build();
    let output = serialize(&[a, b]);
    let parsed = parse(&output).unwrap();
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
    let reparsed = parsed.records;
    assert_eq!(first_extra(&reparsed[0], "bibtex:@key"), Some("a"));
    assert_eq!(first_extra(&reparsed[1], "bibtex:@key"), Some("b"));
    assert_eq!(reparsed.len(), 2);
    assert_eq!(reparsed[0].title.as_deref(), Some("First"));
    assert_eq!(reparsed[1].title.as_deref(), Some("Second"));
}

#[test]
fn serialize_with_month() {
    let record = Record::builder()
        .title("Test")
        .date(PublicationDate {
            year: 2024,
            month: Some(3),
            day: None,
        })
        .extra("bibtex:@key", "test")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("year = {2024}"));
    assert!(output.contains("month = {3}"));
}

#[test]
fn serialize_skips_none_fields() {
    let record = Record::builder()
        .title("Minimal")
        .extra("bibtex:@key", "min")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("title = {Minimal}"));
    assert!(!output.contains("author"));
    assert!(!output.contains("journal"));
    assert!(!output.contains("doi"));
}

// -- Display impls --

#[test]
fn error_display() {
    assert_eq!(
        Error::EmptyInput.to_string(),
        "input was empty or only contained whitespace"
    );
    assert_eq!(
        Error::MalformedSyntax {
            line: 5,
            message: "bad".into()
        }
        .to_string(),
        "malformed syntax at line 5: bad"
    );
    assert_eq!(
        Error::Internal("oops".into()).to_string(),
        "internal error: oops"
    );
}

#[test]
fn date_display() {
    let year_only = PublicationDate {
        year: 2024,
        month: None,
        day: None,
    };
    assert_eq!(year_only.to_string(), "2024");

    let year_month = PublicationDate {
        year: 2024,
        month: Some(3),
        day: None,
    };
    assert_eq!(year_month.to_string(), "2024-03");

    let full = PublicationDate {
        year: 2024,
        month: Some(9),
        day: Some(1),
    };
    assert_eq!(full.to_string(), "2024-09-01");
}

// -- Proptest helpers --

/// Fields the generator never uses as extras: the fields the `BibTeX` parser reads (a copy of its
/// list), `xdata` (consumed while resolving), and `crossref`, `ids` and `set`, which refer to other
/// entries.
const BIBTEX_RESERVED_FIELDS: [&str; 17] = [
    "title",
    "author",
    "date",
    "year",
    "journaltitle",
    "journal",
    "doi",
    "pages",
    "volume",
    "number",
    "abstract",
    "isbn",
    "issn",
    "xdata",
    "crossref",
    "ids",
    "set",
];

/// Entry types that do not read back as themselves: aliases that the parser renames, `unknown`,
/// and the special entries `@comment`, `@string`, `@preamble`, `@xdata` and `@set`.
const UNSTABLE_ENTRY_TYPES: [&str; 9] = [
    "conference",
    "electronic",
    "www",
    "unknown",
    "comment",
    "string",
    "preamble",
    "xdata",
    "set",
];

/// An entry type that reads back as itself: a common one, or any lower-case name that is not
/// special.
fn arb_entry_type() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::sample::select(vec!["article", "book", "inproceedings", "misc", "dataset"])
            .prop_map(str::to_owned),
        "[a-z]{3,10}".prop_filter("entry type must read back unchanged", |name| {
            !UNSTABLE_ENTRY_TYPES.contains(&name.as_str())
        }),
    ]
}

/// Text that reads back unchanged from a `BibTeX` field: no braces (which would break the format),
/// hyphens (`--` becomes an en dash), backslashes or tildes (`~` becomes a no-break space), with
/// single spaces only and no surrounding whitespace, because the parser collapses whitespace.
fn arb_bib_text(max_len: usize) -> impl Strategy<Value = String> {
    let bib_safe = "[A-Za-z0-9 .,;:!?'()\\[\\]/<>@#%^*+=_]";
    proptest::string::string_regex(&format!("{bib_safe}{{1,{max_len}}}"))
        .unwrap()
        .prop_map(|s| {
            // Collapse multiple spaces – biblatex normalises whitespace.
            let mut prev_space = false;
            let collapsed: String = s
                .chars()
                .filter(|&c| {
                    if c == ' ' {
                        if prev_space {
                            return false;
                        }
                        prev_space = true;
                    } else {
                        prev_space = false;
                    }
                    true
                })
                .collect();
            collapsed.trim().to_owned()
        })
        .prop_filter("must not be empty after trim", |s| !s.is_empty())
}

/// Up to four fields the parser does not read, each with one value (a field appears once per
/// entry).
fn arb_bibtex_unread_fields() -> impl Strategy<Value = HashMap<String, String>> {
    proptest::collection::hash_map(
        "[a-z]{2,10}".prop_filter("field must not be read or special", |name| {
            // `month` would give way to a date's month when written.
            !BIBTEX_RESERVED_FIELDS.contains(&name.as_str()) && name != "month"
        }),
        arb_bib_text(40),
        0..=4,
    )
}

fn arb_bibtex_record() -> impl Strategy<Value = Record> {
    (
        proptest::option::of(arb_bib_text(80)),
        // BibTeX-specific author: no hyphens (biblatex converts -- to en-dash),
        // no commas (BibTeX name-part separator). Use "Last, First" format.
        proptest::collection::vec(
            (
                "[A-Z][a-z]{1,15}( [A-Z][a-z]{1,10}){0,2}",
                "[A-Z][a-z]{1,15}( [A-Z]\\.?){0,2}",
            )
                .prop_map(|(last, first)| format!("{last}, {first}")),
            0..5,
        ),
        proptest::option::of(arb_date().prop_map(|d| {
            // BibTeX year field only stores year
            PublicationDate {
                year: d.year,
                month: None,
                day: None,
            }
        })),
        proptest::option::of(arb_bib_text(60)),
        proptest::option::of("10\\.[0-9]{4}/[A-Za-z0-9().]{3,20}"),
        // Pages: biblatex parses numeric ranges lossily (strips leading zeros,
        // collapses same start/end to single page). Use ranges where start < end
        // and non-numeric formats.
        proptest::option::of(prop_oneof![
            (1_u32..9000, 1_u32..9000)
                .prop_filter("start must differ from end", |(s, e)| s != e)
                .prop_map(|(s, e)| {
                    let (lo, hi) = if s < e { (s, e) } else { (e, s) };
                    format!("{lo}-{hi}")
                }),
            "e[0-9]{3,6}".prop_map(|s| s),
            (1_u32..9000).prop_map(|n| n.to_string()),
        ]),
        // Volume: no leading zeros (biblatex parses as i64, loses them).
        proptest::option::of(prop_oneof![
            "[1-9][0-9]{0,2}",
            "[1-9][0-9]{0,2}[A-Z]",
            "S[1-9][0-9]{0,1}",
            "Suppl [1-9]",
        ]),
        proptest::option::of("[0-9]{1,4}"),
        proptest::option::of(arb_bib_text(200)),
        (
            proptest::option::of("[0-9]{4}-[0-9]{3}[0-9X]"),
            "[a-z]{1,10}[0-9]{4}", // citation key
            arb_entry_type(),
            arb_bibtex_unread_fields(),
        ),
    )
        .prop_map(
            |(
                title,
                authors,
                date,
                journal,
                doi,
                pages,
                volume,
                number,
                abstract_text,
                (isbn, key, entry_type, unread),
            )| {
                let mut extras: HashMap<String, Vec<String>> = unread
                    .into_iter()
                    .map(|(name, value)| (format!("bibtex:{name}"), vec![value]))
                    .collect();
                extras.insert("bibtex:@key".to_owned(), vec![key]);
                extras.insert("bibtex:@type".to_owned(), vec![entry_type]);
                {
                    let mut record = Record::default();
                    record.title = title;
                    record.authors = authors;
                    record.date = date;
                    record.journal = journal;
                    record.doi = doi;
                    record.pages = pages;
                    record.volume = volume;
                    record.number = number;
                    record.abstract_text = abstract_text;
                    record.isbn = isbn;
                    record.extras = extras;
                    record
                }
            },
        )
}

// -- Property-based tests --

/// Parsing arbitrary printable text never panics.
#[proptest]
fn parse_never_panics(#[strategy("\\PC{0,500}")] s: String) {
    let _ = parse(&s);
}

/// Serialising a record and parsing the output gives back the same record.
#[proptest]
fn serialize_round_trip(#[strategy(arb_bibtex_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
    prop_assert_eq!(vec![record], reparsed);
}

/// Parsing arbitrary text wrapped in fragments of `BibTeX` syntax (record and field openers,
/// closers and line endings) never panics.
#[proptest]
fn adversarial_parse_no_panic(
    #[strategy("(@article\\{|@misc\\{|title = \\{|author = \\{|\\}|\n){0,5}")] prefix: String,
    #[strategy("\\PC{0,200}")] body: String,
    #[strategy("(\\}|\n){0,3}")] suffix: String,
) {
    let input = format!("{prefix}{body}{suffix}");
    let _ = parse(&input);
}

// -- Identifier properties --

/// Identifier values as they appear in exports: single ISSNs and ISBN-13s, pairs of them, empty,
/// whitespace only, and padded. They contain no braces, `--`, `~`, `\` or repeated inner spaces,
/// which the parser rewrites, so a parsed value is the generated value trimmed.
fn identifier_value() -> impl Strategy<Value = String> {
    let serial_number = "[0-9]{4}-[0-9]{3}[0-9X]";
    let book_number = "97[89]-[0-9]{1,5}-[0-9]{1,7}-[0-9]{1,6}-[0-9X]";
    let single = prop_oneof![serial_number, book_number];
    let pair = (
        single.clone(),
        prop_oneof![Just(", "), Just(" ")],
        single.clone(),
    )
        .prop_map(|(a, separator, b)| format!("{a}{separator}{b}"));
    let padded = ("[ ]{1,2}", single.clone(), "[ ]{1,2}")
        .prop_map(|(before, value, after)| format!("{before}{value}{after}"));
    prop_oneof![
        3 => single,
        1 => pair,
        1 => Just(String::new()),
        1 => "[ ]{1,3}",
        1 => padded,
    ]
}

/// The identifier fields of one entry: `isbn`, `issn` or both, in any order, each with a value.
#[derive(Arbitrary, Debug)]
struct IdentifierFields {
    /// One or both identifier fields, shuffled. No field repeats.
    #[strategy(subsequence(vec!["isbn", "issn"], 1..=2).prop_shuffle())]
    fields: Vec<&'static str>,
    /// One value for each entry in `fields`, in the same order.
    #[strategy(vec(identifier_value(), #fields.len()))]
    values: Vec<String>,
}

/// The parsed identifier matches a direct reference: trim the values, put `isbn` before `issn`,
/// then take the first non-blank value, or the first value if all are blank.
#[proptest]
fn identifier_fallback_matches_reference(input: IdentifierFields) {
    let mut field_lines = String::new();
    for (field, value) in input.fields.iter().zip(&input.values) {
        writeln!(field_lines, "  {field} = {{{value}}},").unwrap();
    }
    let identifier = identifier_of(&field_lines);

    let mut by_preference: Vec<(&str, String)> = input
        .fields
        .into_iter()
        .zip(input.values)
        .map(|(field, value)| (field, value.trim().to_owned()))
        .collect();
    by_preference.sort_by_key(|(field, _)| *field != "isbn");
    let expected = by_preference
        .iter()
        .find(|(_, value)| !value.is_empty())
        .or(by_preference.first())
        .map(|(_, value)| value.clone());

    prop_assert_eq!(identifier, expected);
}

// -- Lenient parsing properties --

/// One entry of a generated input: a valid entry with a title, or a broken one.
#[derive(Debug, Clone)]
enum GeneratedEntry {
    /// A valid entry with this key and title.
    Valid {
        /// The citation key, drawn from a small set so keys repeat.
        key: String,
        /// The title.
        title: String,
    },
    /// An entry whose title brace is never closed.
    Broken,
}

/// A valid entry (keys drawn from two letters, so they often repeat) or a broken one. Titles are
/// words separated by single spaces, because the parser collapses runs of spaces.
fn arb_generated_entry() -> impl Strategy<Value = GeneratedEntry> {
    prop_oneof![
        3 => ("[ab]", "[A-Za-z]{1,8}( [A-Za-z]{1,8}){0,3}")
            .prop_map(|(key, title)| GeneratedEntry::Valid { key, title }),
        1 => Just(GeneratedEntry::Broken),
    ]
}

/// Broken entries among valid ones are skipped and reported at their lines; every valid entry is
/// kept, in order.
#[proptest]
fn broken_entries_skipped(
    #[strategy(vec(arb_generated_entry(), 1..10))] entries: Vec<GeneratedEntry>,
) {
    let mut input = String::new();
    let mut expected_titles = Vec::new();
    let mut broken_lines = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            GeneratedEntry::Valid { key, title } => {
                writeln!(input, "@article{{{key}, title = {{{title}}}}}").unwrap();
                expected_titles.push(Some(title.as_str()));
            }
            GeneratedEntry::Broken => {
                writeln!(input, "@article{{broken, title = {{never closed").unwrap();
                broken_lines.push(index + 1);
            }
        }
    }

    let parsed = parse(&input).unwrap();
    prop_assert_eq!(titles(&parsed), expected_titles);
    let skipped: Vec<usize> = parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::SkippedEntry { .. }))
        .map(Diagnostic::line)
        .collect();
    prop_assert_eq!(skipped, broken_lines);
}

/// Each entry without a usable title is reported once, and diagnostics come in line order.
#[proptest]
fn missing_titles_reported_in_order(
    #[strategy(vec(proptest::option::of("[A-Za-z ]{0,10}"), 1..10))] titles: Vec<Option<String>>,
) {
    let mut input = String::new();
    for (index, title) in titles.iter().enumerate() {
        match title {
            Some(title) => writeln!(input, "@article{{k{index}, title = {{{title}}}}}").unwrap(),
            None => writeln!(input, "@article{{k{index}, year = {{2024}}}}").unwrap(),
        }
    }
    let parsed = parse(&input).unwrap();

    let untitled: Vec<usize> = titles
        .iter()
        .enumerate()
        .filter(|(_, title)| title.as_deref().is_none_or(|t| t.trim().is_empty()))
        .map(|(index, _)| index + 1)
        .collect();
    let missing: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    prop_assert_eq!(missing, untitled);
}

// -- Byte-order mark --

/// Any input, as serialised records or as arbitrary text. It never starts with a byte-order mark:
/// U+FEFF is a format character, which `\PC` excludes, and the serialiser never writes it.
fn arb_input_without_byte_order_mark() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::collection::vec(arb_bibtex_record(), 0..4)
            .prop_map(|records| serialize(&records)),
        "\\PC{0,200}",
    ]
}

/// A leading byte-order mark changes nothing: records, diagnostics and errors are the same.
#[proptest]
fn byte_order_mark_does_not_change_result(
    #[strategy(arb_input_without_byte_order_mark())] input: String,
) {
    prop_assert_eq!(parse(&format!("\u{feff}{input}")), parse(&input));
}

// -- Extras properties --

/// Keys of the other formats, which `BibTeX` serialisation must never write.
const FOREIGN_KEYS: [&str; 3] = ["ris:KW", "nbib:OT", "endnote:keywords/keyword"];

/// An extra naming a field the parser reads (or `xdata`, which is consumed when resolving) is never
/// written, so it cannot leak into a field.
#[proptest]
fn read_fields_never_written_from_extras(
    #[strategy(proptest::sample::select(BIBTEX_RESERVED_FIELDS[..14].to_vec()))] name: &'static str,
    #[strategy(arb_bibtex_record())] record: Record,
) {
    let mut with_read_field = record.clone();
    with_read_field
        .extras
        .insert(format!("bibtex:{name}"), vec!["leaked value".to_owned()]);
    prop_assert_eq!(serialize(&[with_read_field]), serialize(&[record]));
}

/// Extras of other formats are kept on the record but never written as `BibTeX` fields.
#[proptest]
fn foreign_extras_not_written(
    #[strategy(arb_bibtex_record())] record: Record,
    #[strategy("[A-Za-z0-9 ]{1,30}")] value: String,
) {
    let mut with_foreign = record.clone();
    for key in FOREIGN_KEYS {
        with_foreign
            .extras
            .insert(key.to_owned(), vec![value.clone()]);
    }
    prop_assert_eq!(serialize(&[with_foreign]), serialize(&[record]));
}

/// Pieces of a field value in `BibTeX` source: words, escaped special characters, groups, inline
/// maths and dashes, which the parser rewrites.
fn arb_value_token() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[A-Za-z0-9]{1,8}",
        1 => proptest::sample::select(vec!["\\{", "\\}", "\\$", "\\%", "\\&", "--"])
            .prop_map(str::to_owned),
        1 => "[A-Za-z]{1,6}".prop_map(|word| format!("{{{word}}}")),
        1 => "[A-Za-z]{1,6}".prop_map(|word| format!("${word}$")),
    ]
}

/// Entries read from source with arbitrary extra fields, whose values the parser rewrites, come
/// back unchanged when written and read again.
#[proptest]
fn records_stable_through_serialize(
    #[strategy(proptest::collection::hash_map(
        "[a-z]{2,10}".prop_filter("field must not be read or special", |name| {
            !BIBTEX_RESERVED_FIELDS.contains(&name.as_str()) && name != "month"
        }),
        proptest::collection::vec(arb_value_token(), 1..6).prop_map(|tokens| tokens.join(" ")),
        0..5,
    ))]
    fields: HashMap<String, String>,
) {
    let mut input = String::from("@misc{k,\n  title = {A title},\n");
    for (name, value) in &fields {
        writeln!(input, "  {name} = {{{value}}},").unwrap();
    }
    input.push_str("}\n");
    let first = parse(&input).unwrap();
    prop_assert_eq!(&first.diagnostics, &Vec::<Diagnostic>::new());
    let second = parse(&serialize(&first.records)).unwrap().records;
    prop_assert_eq!(second, first.records);
}
