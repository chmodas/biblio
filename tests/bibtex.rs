mod common;

use proptest::prelude::*;
use std::collections::HashMap;

use biblio::bibtex::{parse, serialize};
use biblio::{Error, PublicationDate, Record};

use common::arb_date;

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

    let records = parse(input).unwrap();
    assert_eq!(records.len(), 1);

    let r = &records[0];
    assert_eq!(r.title, "A test title");
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
    assert_eq!(r.extras.get("key").map(String::as_str), Some("doe2024"));
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

    let records = parse(input).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].title, "CRISPR-Cas9 gene editing");
    assert_eq!(
        records[0].extras.get("key").map(String::as_str),
        Some("frangoul2024")
    );
    assert_eq!(
        records[1].title,
        "Large language models in clinical medicine"
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
    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A website resource");
    assert!(records[0].authors.is_empty());
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
    let records = parse(input).unwrap();
    assert!(records[0].authors.is_empty());
}

#[test]
fn author_with_prefix() {
    let input = r"
@article{von2024,
  title = {Test},
  author = {von Neumann, John},
}
";
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2020,
            month: None,
            day: None,
        })
    );
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
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
    let records = parse(input).unwrap();
    assert_eq!(records[0].journal.as_deref(), Some("Science"));
}

// -- Extras --

#[test]
fn extras_citation_key() {
    let input = r"
@article{mykey2024,
  title = {Test},
}
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].extras.get("key").map(String::as_str),
        Some("mykey2024")
    );
}

// -- Error cases --

#[test]
fn empty_input() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("   \n\n  "), Err(Error::EmptyInput));
}

#[test]
fn malformed_bibtex() {
    let result = parse("@article{broken, title = {missing close brace}");
    assert!(result.is_err());
}

#[test]
fn missing_title() {
    let input = r"
@article{x,
  year = {2024},
}
";
    let result = parse(input);
    assert!(matches!(
        result,
        Err(Error::MissingRequiredField { tag: "title", .. })
    ));
}

// -- Serialization --

#[test]
fn serialize_multiple_entries() {
    let a = Record {
        title: "First".into(),
        extras: [("key".into(), "a".into())].into_iter().collect(),
        ..Default::default()
    };
    let b = Record {
        title: "Second".into(),
        extras: [("key".into(), "b".into())].into_iter().collect(),
        ..Default::default()
    };
    let output = serialize(&[a, b]);
    let reparsed = parse(&output).unwrap();
    assert_eq!(reparsed.len(), 2);
    assert_eq!(reparsed[0].title, "First");
    assert_eq!(reparsed[1].title, "Second");
}

#[test]
fn serialize_with_month() {
    let record = Record {
        title: "Test".into(),
        date: Some(PublicationDate {
            year: 2024,
            month: Some(3),
            day: None,
        }),
        extras: [("key".into(), "test".into())].into_iter().collect(),
        ..Default::default()
    };
    let output = serialize(&[record]);
    assert!(output.contains("year = {2024}"));
    assert!(output.contains("month = {3}"));
}

#[test]
fn serialize_skips_none_fields() {
    let record = Record {
        title: "Minimal".into(),
        extras: [("key".into(), "min".into())].into_iter().collect(),
        ..Default::default()
    };
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
        Error::MissingRequiredField {
            tag: "title",
            line: 1
        }
        .to_string(),
        "missing required field 'title' at line 1"
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

fn arb_bibtex_record() -> impl Strategy<Value = Record> {
    // BibTeX-safe text: no braces which would break the format.
    // No braces, no hyphens (biblatex converts -- to en-dash), no backslash,
    // no tilde (biblatex converts ~ to non-breaking space).
    let bib_safe = "[A-Za-z0-9 .,;:!?'()\\[\\]/<>@#%^*+=_]";
    let arb_bib_text = move |max_len: usize| {
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
    };

    (
        arb_bib_text(80),
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
                (isbn, key),
            )| {
                let mut extras = HashMap::new();
                extras.insert("key".into(), key);
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
            },
        )
}

// -- Property-based tests --

proptest! {
    #[test]
    fn parse_never_panics(s in "\\PC{0,500}") {
        let _ = parse(&s);
    }

    #[test]
    fn serialize_round_trip(record in arb_bibtex_record()) {
        let serialized = serialize(std::slice::from_ref(&record));
        let reparsed = parse(&serialized).unwrap();
        prop_assert_eq!(vec![record], reparsed);
    }

    #[test]
    fn adversarial_parse_no_panic(
        prefix in "(@article\\{|@misc\\{|title = \\{|author = \\{|\\}|\n){0,5}",
        body in "\\PC{0,200}",
        suffix in "(\\}|\n){0,3}",
    ) {
        let input = format!("{prefix}{body}{suffix}");
        let _ = parse(&input);
    }
}
