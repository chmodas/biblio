mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::{select, subsequence};
use std::collections::HashMap;
use std::fmt::Write as _;
use test_strategy::{Arbitrary, proptest};

use biblio::nbib::{parse, serialize};
use biblio::{Error, PublicationDate, Record};

use common::{arb_author, arb_date, arb_pages, arb_raw_value, arb_text, arb_volume};

// -- Core parsing --

#[test]
fn single_record() {
    let input = "\
PMID- 12345678
TI  - A test title
FAU - Doe, Jane
FAU - Smith, John
DP  - 2024 Sep 1
JT  - Nature
LID - 10.1038/s41586-024-07386-0 [doi]
PG  - 123-130
VI  - 620
IP  - 3
AB  - This is the abstract text.
IS  - 1234-5678
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
            month: Some(9),
            day: Some(1),
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
    assert_eq!(r.extras.get("PMID").map(String::as_str), Some("12345678"));
}

#[test]
fn multiple_records() {
    let input = "\
PMID- 38456124
TI  - CRISPR-Cas9 gene editing for sickle cell disease and beta-thalassemia
FAU - Frangoul, Haydar
FAU - Altshuler, David
DP  - 2024 Jan 18
JT  - The New England Journal of Medicine
LID - 10.1056/NEJMoa2031054 [doi]
PG  - 175-186
VI  - 390
IP  - 2
AB  - Sickle cell disease causes severe pain crises.
IS  - 1533-4406

PMID- 39012847
TI  - Large language models in clinical medicine
FAU - Lee, Peter
FAU - Bubeck, Sebastien
FAU - Petro, Joseph
DP  - 2024 Mar
JT  - Nature Medicine
LID - 10.1038/s41591-024-02893-1 [doi]
PG  - 768-779
VI  - 30
IP  - 3
AB  - We review the emerging capabilities of large language models in medicine.
IS  - 1546-170X
";

    let records = parse(input).unwrap();
    assert_eq!(records.len(), 2);

    assert_eq!(
        records[0].title,
        "CRISPR-Cas9 gene editing for sickle cell disease and beta-thalassemia"
    );
    assert_eq!(
        records[0].authors,
        vec!["Frangoul, Haydar", "Altshuler, David"]
    );
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2024,
            month: Some(1),
            day: Some(18),
        })
    );
    assert_eq!(
        records[0].journal.as_deref(),
        Some("The New England Journal of Medicine")
    );
    assert_eq!(records[0].doi.as_deref(), Some("10.1056/NEJMoa2031054"));
    assert_eq!(records[0].pages.as_deref(), Some("175-186"));
    assert_eq!(records[0].volume.as_deref(), Some("390"));
    assert_eq!(records[0].number.as_deref(), Some("2"));
    assert_eq!(records[0].isbn.as_deref(), Some("1533-4406"));

    assert_eq!(
        records[1].title,
        "Large language models in clinical medicine"
    );
    assert_eq!(
        records[1].authors,
        vec!["Lee, Peter", "Bubeck, Sebastien", "Petro, Joseph"]
    );
    assert_eq!(
        records[1].date,
        Some(PublicationDate {
            year: 2024,
            month: Some(3),
            day: None,
        })
    );
    assert_eq!(records[1].journal.as_deref(), Some("Nature Medicine"));
    assert_eq!(
        records[1].doi.as_deref(),
        Some("10.1038/s41591-024-02893-1")
    );
    assert_eq!(records[1].isbn.as_deref(), Some("1546-170X"));
}

#[test]
fn windows_line_endings() {
    let input = "TI  - A test title\r\nFAU - Doe, Jane\r\nDP  - 2024\r\n";

    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A test title");
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

#[test]
fn unknown_tags_ignored() {
    let input = "\
TI  - A test title
OT  - keyword one
MH  - mesh heading
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A test title");
    assert!(records[0].extras.is_empty());
}

// -- Authors --

#[test]
fn fau_over_au() {
    let input = "\
TI  - A test title
FAU - Doe, Jane
AU  - Doe J
FAU - Smith, John
AU  - Smith J
";

    let records = parse(input).unwrap();
    assert_eq!(records[0].authors, vec!["Doe, Jane", "Smith, John"]);
}

#[test]
fn au_fallback() {
    let input = "\
TI  - A test title
AU  - Doe J
AU  - Smith J
";

    let records = parse(input).unwrap();
    assert_eq!(records[0].authors, vec!["Doe J", "Smith J"]);
}

// -- Books --

/// A book record has no `TI` or `JT`: its title is the book title (`BTI`), here wrapped over two
/// continuation lines, and its journal is the collection title (`CTI`). It parses alongside an
/// ordinary article.
#[test]
fn book_record_title_and_series() {
    let input = "\
PMID- 10000001
TI  - An ordinary article
JT  - Journal of Testing

PMID- 10000002
PT  - Book
CTI - Reports in Testing
DP  - 2021 Nov
BTI - A very long book title that
      wraps onto a second line and
      then a third.
FAU - Doe, Jane
";
    let records = parse(input).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].title, "An ordinary article");
    assert_eq!(
        records[1].title,
        "A very long book title that wraps onto a second line and then a third."
    );
    assert_eq!(records[1].journal.as_deref(), Some("Reports in Testing"));
    assert_eq!(records[1].authors, vec!["Doe, Jane"]);
}

/// In a chapter record, `TI` holds the chapter title and is preferred over the book title.
#[test]
fn ti_preferred_over_bti() {
    let records = parse("BTI - The Book\nTI  - The Chapter\n").unwrap();
    assert_eq!(records[0].title, "The Chapter");
}

/// `JT` is preferred over the collection title.
#[test]
fn jt_preferred_over_cti() {
    let records = parse("TI  - t\nCTI - Reports in Testing\nJT  - Journal of Testing\n").unwrap();
    assert_eq!(records[0].journal.as_deref(), Some("Journal of Testing"));
}

/// A blank `TI` does not hide a book title that has a value.
#[test]
fn blank_ti_does_not_hide_bti() {
    let records = parse("TI  - \nBTI - The Book\n").unwrap();
    assert_eq!(records[0].title, "The Book");
}

/// A blank `JT` does not hide a collection title that has a value.
#[test]
fn blank_jt_does_not_hide_cti() {
    let records = parse("TI  - t\nJT  - \nCTI - Reports in Testing\n").unwrap();
    assert_eq!(records[0].journal.as_deref(), Some("Reports in Testing"));
}

/// The collection title is shared by unrelated books, so it is never used as a title.
#[test]
fn collection_title_is_not_a_title() {
    let err = parse("CTI - Reports in Testing\nDP  - 2024\n").unwrap_err();
    assert!(matches!(err, Error::MissingRequiredField { tag: "TI", .. }));
}

// -- DOI extraction --

#[test]
fn doi_from_lid() {
    let input = "\
TI  - A test title
LID - S0140-6736(24)00624-X [pii]
LID - 10.1016/S0140-6736(24)00624-X [doi]
";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].doi.as_deref(),
        Some("10.1016/S0140-6736(24)00624-X")
    );
}

#[test]
fn doi_from_aid() {
    let input = "\
TI  - A test title
AID - 10.1016/j.cell.2024.01.001 [doi]
";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].doi.as_deref(),
        Some("10.1016/j.cell.2024.01.001")
    );
}

// -- Date parsing --

#[test]
fn date_year_only() {
    let input = "\
TI  - A test title
DP  - 2024
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
fn date_year_month() {
    let input = "\
TI  - A test title
DP  - 2024 Sep
";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2024,
            month: Some(9),
            day: None,
        })
    );
}

#[test]
fn date_full() {
    let input = "\
TI  - A test title
DP  - 2024 Sep 1
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

// -- Continuation lines --

#[test]
fn continuation_lines() {
    let input = "\
TI  - A test title
AB  - This is a long abstract that
      continues on the next line and
      keeps going further.
";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].abstract_text.as_deref(),
        Some("This is a long abstract that continues on the next line and keeps going further.")
    );
}

#[test]
fn continuation_lines_au() {
    let input = "\
TI  - A test title
AU  - Very Long Author Name That
      Continues Here
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].authors,
        vec!["Very Long Author Name That Continues Here"]
    );
}

/// A blank title continued on the next line becomes the continuation text, without a leading
/// space.
#[test]
fn blank_title_continued_without_leading_space() {
    let records = parse("TI  - \n      A test title\n").unwrap();
    assert_eq!(records[0].title, "A test title");
}

/// A blank author continued on the next line becomes the continuation text, without a leading
/// space.
#[test]
fn blank_author_continued_without_leading_space() {
    let records = parse("TI  - A test title\nFAU - \n      Doe, Jane\n").unwrap();
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

/// A blank `LID` continued with a DOI on the next line gives the DOI without a leading space.
#[test]
fn blank_lid_continued_gives_doi() {
    let records = parse("TI  - A test title\nLID - \n      10.1000/xyz123 [doi]\n").unwrap();
    assert_eq!(records[0].doi.as_deref(), Some("10.1000/xyz123"));
}

#[test]
fn continuation_lines_lid() {
    let input = "\
TI  - A test title
LID - 10.1000/very-long-doi-that
      -keeps-going [doi]
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].doi.as_deref(),
        Some("10.1000/very-long-doi-that -keeps-going")
    );
}

// -- Extras --

#[test]
fn extras() {
    let input = "\
PMID- 12345678
PMC - PMC9876543
TI  - A test title
";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].extras.get("PMID").map(String::as_str),
        Some("12345678")
    );
    assert_eq!(
        records[0].extras.get("PMC").map(String::as_str),
        Some("PMC9876543")
    );
}

// -- Error cases --

#[test]
fn empty_input() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("   \n\n  "), Err(Error::EmptyInput));
}

#[test]
fn missing_title() {
    let input = "\
FAU - Doe, Jane
DP  - 2024
";

    let err = parse(input).unwrap_err();
    assert!(matches!(err, Error::MissingRequiredField { tag: "TI", .. }));
}

// -- Serialization --

#[test]
fn serialize_multiple_records() {
    let a = Record {
        title: "First".into(),
        ..Default::default()
    };
    let b = Record {
        title: "Second".into(),
        ..Default::default()
    };
    let output = serialize(&[a, b]);
    let reparsed = parse(&output).unwrap();
    assert_eq!(reparsed.len(), 2);
    assert_eq!(reparsed[0].title, "First");
    assert_eq!(reparsed[1].title, "Second");
}

#[test]
fn serialize_skips_none_fields() {
    let record = Record {
        title: "Minimal".into(),
        ..Default::default()
    };

    let output = serialize(&[record]);
    assert!(output.contains("TI  - Minimal"));
    assert!(!output.contains("FAU"));
    assert!(!output.contains("DP"));
    assert!(!output.contains("JT"));
}

// -- NBIB-specific proptest helpers --

fn format_nbib_date(date: PublicationDate) -> String {
    let months = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    match (date.month, date.day) {
        (Some(m), Some(d)) => format!("{} {} {d}", date.year, months[m as usize]),
        (Some(m), None) => format!("{} {}", date.year, months[m as usize]),
        _ => format!("{}", date.year),
    }
}

fn arb_nbib_record() -> impl Strategy<Value = Record> {
    (
        arb_text(80),
        proptest::collection::vec(arb_author(), 0..5),
        proptest::option::of(arb_date()),
        proptest::option::of(arb_text(60)),
        proptest::option::of("10\\.[0-9]{4}/[A-Za-z0-9().\\-]{3,30}"),
        proptest::option::of(arb_pages()),
        proptest::option::of(arb_volume()),
        proptest::option::of("[0-9]{1,4}"),
        proptest::option::of(arb_text(300)),
        (
            proptest::option::of("[0-9]{4}-[0-9]{3}[0-9X]"),
            proptest::option::of("[0-9]{5,8}"),
            proptest::option::of("PMC[0-9]{5,8}"),
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
                (isbn, pmid, pmc),
            )| {
                let mut extras = HashMap::new();
                if let Some(pmid) = pmid {
                    extras.insert("PMID".into(), pmid);
                }
                if let Some(pmc) = pmc {
                    extras.insert("PMC".into(), pmc);
                }
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

/// Parsing arbitrary printable text never panics.
#[proptest]
fn parse_never_panics(#[strategy("\\PC{0,500}")] s: String) {
    let _ = parse(&s);
}

/// A date written in the `NBIB` date format parses back to the same date.
#[proptest]
fn date_round_trip(#[strategy(arb_date())] date: PublicationDate) {
    let formatted = format_nbib_date(date);
    let reparsed = parse(&format!("TI  - t\nDP  - {formatted}\n")).unwrap()[0]
        .date
        .unwrap();
    prop_assert_eq!(date, reparsed);
}

/// Serialising a record and parsing the output gives back the same record.
#[proptest]
fn serialize_round_trip(#[strategy(arb_nbib_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap();
    prop_assert_eq!(vec![record], reparsed);
}

/// Values containing tag-like text survive a round trip, because that text appears mid-line,
/// not at the start of a line.
#[proptest]
fn adversarial_values_round_trip(
    #[strategy(select(&["TI  - nested", "AB  - trap", "PMID- 999"]))] tag_like: &'static str,
    #[strategy("[A-Za-z ]{1,20}")] padding: String,
) {
    let title = format!("{padding} {tag_like} {padding}")
        .trim_end()
        .to_owned();
    let record = Record {
        title,
        ..Default::default()
    };
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap();
    prop_assert_eq!(vec![record], reparsed);
}

/// Parsing arbitrary text wrapped in fragments of `NBIB` syntax (tag openers, continuation
/// indents, blank-line record separators and mixed line endings) never panics.
#[proptest]
fn adversarial_parse_no_panic(
    #[strategy("(TI  - |FAU - |AB  - |PMID- |      |\\r\\n|\n){0,5}")] prefix: String,
    #[strategy("\\PC{0,200}")] body: String,
    #[strategy("(\n\n|\r\n\r\n|\n){0,3}")] suffix: String,
) {
    let input = format!("{prefix}{body}{suffix}");
    let _ = parse(&input);
}

// -- Continuation properties --

/// A blank field of any continuable tag, continued on an indented line, becomes exactly the
/// trimmed continuation text.
#[proptest]
fn blank_field_continued_is_continuation_text(
    #[strategy(select(&["TI", "BTI", "CTI", "AB", "JT", "FAU", "AU"]))] tag: &'static str,
    #[strategy("[^\\s\\p{C}]\\PC{0,59}")] text: String,
) {
    // The leading title makes the record valid for every tag, and a blank `TI` replaces it. A book
    // title gets no leading title, because a non-blank `TI` would be preferred over it.
    let title = if tag == "BTI" { "" } else { "TI  - t\n" };
    let input = format!("{title}{tag:<4}- \n      {text}\n");
    let record = parse(&input).unwrap().remove(0);

    let field = match tag {
        "TI" | "BTI" => Some(record.title),
        "AB" => record.abstract_text,
        "JT" | "CTI" => record.journal,
        _ => record.authors.last().cloned(),
    };
    prop_assert_eq!(field.as_deref(), Some(text.trim()));
}

// -- Fallback properties --

/// A preferred tag and its fallback, in any order and possibly with only one of them, each with a
/// raw value.
#[derive(Arbitrary, Debug)]
struct FallbackTags {
    /// The preferred tag followed by its fallback: the title (`TI`, `BTI`) or the journal (`JT`,
    /// `CTI`).
    #[strategy(select(&[["TI", "BTI"], ["JT", "CTI"]]))]
    pair: [&'static str; 2],
    /// One or both tags of `pair`, shuffled. No tag repeats, so no value overwrites another.
    #[strategy(subsequence(#pair.to_vec(), 1..=2).prop_shuffle())]
    tags: Vec<&'static str>,
    /// One raw value for each entry in `tags`, in the same order.
    #[strategy(vec(arb_raw_value(), #tags.len()))]
    values: Vec<String>,
}

/// The parsed title or journal matches a direct reference: order the values by preference, then
/// take the first non-blank value, or the first value if all are blank.
#[proptest]
fn fallback_matches_reference(input: FallbackTags) {
    let is_title = input.pair[0] == "TI";
    // A journal record needs a title to parse.
    let mut text = String::from(if is_title { "" } else { "TI  - t\n" });
    for (tag, value) in input.tags.iter().zip(&input.values) {
        writeln!(text, "{tag:<4}- {value}").unwrap();
    }
    let record = parse(&text).unwrap().remove(0);
    let parsed = if is_title {
        Some(record.title)
    } else {
        record.journal
    };

    let mut by_preference: Vec<(usize, String)> = input
        .tags
        .into_iter()
        .zip(input.values)
        .map(|(tag, value)| {
            let rank = input.pair.iter().position(|t| *t == tag).unwrap();
            (rank, value.trim_end().to_owned())
        })
        .collect();
    by_preference.sort_by_key(|(rank, _)| *rank);
    let expected = by_preference
        .iter()
        .find(|(_, value)| !value.is_empty())
        .or(by_preference.first())
        .map(|(_, value)| value.clone());

    prop_assert_eq!(parsed, expected);
}
