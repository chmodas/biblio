mod common;

use proptest::prelude::*;
use std::collections::HashMap;

use biblio::nbib::{parse, serialize};
use biblio::{Error, PublicationDate, Record};

use common::{arb_author, arb_date, arb_pages, arb_text, arb_volume};

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

proptest! {
    #[test]
    fn parse_never_panics(s in "\\PC{0,500}") {
        let _ = parse(&s);
    }

    #[test]
    fn date_round_trip(date in arb_date()) {
        let formatted = format_nbib_date(date);
        let reparsed = parse(&format!("TI  - t\nDP  - {formatted}\n"))
            .unwrap()[0]
            .date
            .unwrap();
        prop_assert_eq!(date, reparsed);
    }

    #[test]
    fn serialize_round_trip(record in arb_nbib_record()) {
        let serialized = serialize(std::slice::from_ref(&record));
        let reparsed = parse(&serialized).unwrap();
        prop_assert_eq!(vec![record], reparsed);
    }

    /// Values containing tag-like patterns (e.g. "TI  - nested") must
    /// survive round-trip because they appear mid-line, not at line start.
    #[test]
    fn adversarial_values_round_trip(
        tag_like in prop::sample::select(&["TI  - nested", "AB  - trap", "PMID- 999"]),
        padding in "[A-Za-z ]{1,20}",
    ) {
        let title = format!("{padding} {tag_like} {padding}").trim_end().to_owned();
        let record = Record {
            title,
            ..Default::default()
        };
        let serialized = serialize(std::slice::from_ref(&record));
        let reparsed = parse(&serialized).unwrap();
        prop_assert_eq!(vec![record], reparsed);
    }

    /// Parsing adversarial input must never panic, even with embedded
    /// tag separators, fake continuation lines, and mixed line endings.
    #[test]
    fn adversarial_parse_no_panic(
        prefix in "(TI  - |FAU - |AB  - |PMID- |      |\\r\\n|\n){0,5}",
        body in "\\PC{0,200}",
        suffix in "(\n\n|\r\n\r\n|\n){0,3}",
    ) {
        let input = format!("{prefix}{body}{suffix}");
        let _ = parse(&input);
    }
}
