mod common;

use proptest::prelude::*;
use std::collections::HashMap;

use biblio::ris::{parse, serialize};
use biblio::{Error, PublicationDate, Record};

use common::{arb_author, arb_date, arb_pages, arb_text, arb_volume};

// -- Core parsing --

#[test]
fn single_record() {
    let input = "\
TY  - JOUR
TI  - A test title
AU  - Doe, Jane
AU  - Smith, John
PY  - 2024/09/01/
JO  - Nature
DO  - 10.1038/s41586-024-07386-0
SP  - 123
EP  - 130
VL  - 620
IS  - 3
AB  - This is the abstract text.
SN  - 1234-5678
ID  - ref001
ER  -
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
    assert_eq!(r.extras.get("ID").map(String::as_str), Some("ref001"));
}

#[test]
fn multiple_records() {
    let input = "\
TY  - JOUR
TI  - CRISPR-Cas9 gene editing for sickle cell disease
AU  - Frangoul, Haydar
AU  - Altshuler, David
PY  - 2024/01/18/
JO  - The New England Journal of Medicine
DO  - 10.1056/NEJMoa2031054
SP  - 175
EP  - 186
VL  - 390
IS  - 2
AB  - Sickle cell disease causes severe pain crises.
SN  - 1533-4406
ER  -

TY  - JOUR
TI  - Large language models in clinical medicine
AU  - Lee, Peter
AU  - Bubeck, Sebastien
AU  - Petro, Joseph
PY  - 2024/03//
JO  - Nature Medicine
DO  - 10.1038/s41591-024-02893-1
SP  - 768
EP  - 779
VL  - 30
IS  - 3
SN  - 1546-170X
ER  -
";

    let records = parse(input).unwrap();
    assert_eq!(records.len(), 2);

    assert_eq!(
        records[0].title,
        "CRISPR-Cas9 gene editing for sickle cell disease"
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
    assert_eq!(records[0].doi.as_deref(), Some("10.1056/NEJMoa2031054"));
    assert_eq!(records[0].pages.as_deref(), Some("175-186"));
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
    assert_eq!(records[1].isbn.as_deref(), Some("1546-170X"));
}

#[test]
fn windows_line_endings() {
    let input = "TY  - JOUR\r\nTI  - A test title\r\nAU  - Doe, Jane\r\nER  -\r\n";
    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A test title");
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

#[test]
fn unknown_tags_ignored() {
    let input = "\
TY  - JOUR
TI  - A test title
KW  - keyword one
N1  - some note
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A test title");
    assert!(records[0].extras.is_empty());
}

#[test]
fn record_without_er() {
    let input = "\
TY  - JOUR
TI  - Unclosed record
AU  - Doe, Jane
";
    let records = parse(input).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].title, "Unclosed record");
}

// -- Authors --

#[test]
fn au_and_a1() {
    let input = "\
TY  - JOUR
TI  - A test title
AU  - Doe, Jane
A1  - Smith, John
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].authors, vec!["Doe, Jane", "Smith, John"]);
}

#[test]
fn a1_only() {
    let input = "\
TY  - JOUR
TI  - A test title
A1  - Doe, Jane
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

// -- DOI --

#[test]
fn doi_verbatim() {
    let input = "\
TY  - JOUR
TI  - A test title
DO  - 10.1016/S0140-6736(24)00624-X
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].doi.as_deref(),
        Some("10.1016/S0140-6736(24)00624-X")
    );
}

// -- Date parsing --

#[test]
fn date_year_only() {
    let input = "\
TY  - JOUR
TI  - A test title
PY  - 2024///
ER  -
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
TY  - JOUR
TI  - A test title
PY  - 2024/09//
ER  -
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
TY  - JOUR
TI  - A test title
PY  - 2024/09/01/
ER  -
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
fn date_y1_fallback() {
    let input = "\
TY  - JOUR
TI  - A test title
Y1  - 2023/06//
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2023,
            month: Some(6),
            day: None,
        })
    );
}

// -- Continuation lines --

#[test]
fn continuation_line_resembling_tag() {
    let input = "\
TY  - JOUR
TI  - A test title
AB  - This abstract mentions that
is  - not actually a tag
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].abstract_text.as_deref(),
        Some("This abstract mentions that is  - not actually a tag")
    );
}

#[test]
fn continuation_abstract() {
    let input = "\
TY  - JOUR
TI  - A test title
AB  - This is a long abstract that
continues on the next line and
keeps going further.
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].abstract_text.as_deref(),
        Some("This is a long abstract that continues on the next line and keeps going further.")
    );
}

#[test]
fn n2_as_abstract() {
    let input = "\
TY  - JOUR
TI  - A test title
N2  - Abstract via N2 tag.
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].abstract_text.as_deref(),
        Some("Abstract via N2 tag.")
    );
}

#[test]
fn jf_as_journal() {
    let input = "\
TY  - JOUR
TI  - A test title
JF  - Science
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].journal.as_deref(), Some("Science"));
}

// -- Pages --

#[test]
fn ep_only_discarded() {
    let input = "\
TY  - JOUR
TI  - A test title
EP  - 130
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].pages, None);
}

#[test]
fn sp_ep_combined() {
    let input = "\
TY  - JOUR
TI  - A test title
SP  - 123
EP  - 130
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].pages.as_deref(), Some("123-130"));
}

#[test]
fn sp_only() {
    let input = "\
TY  - JOUR
TI  - A test title
SP  - e12345
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].pages.as_deref(), Some("e12345"));
}

// -- Title preference --

#[test]
fn ti_over_t1() {
    let input = "\
TY  - JOUR
TI  - Preferred title
T1  - Fallback title
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "Preferred title");
}

#[test]
fn t1_fallback() {
    let input = "\
TY  - JOUR
T1  - Fallback title
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "Fallback title");
}

// -- Extras --

#[test]
fn extras_id() {
    let input = "\
TY  - JOUR
TI  - A test title
ID  - ref001
ER  -
";
    let records = parse(input).unwrap();
    assert_eq!(
        records[0].extras.get("ID").map(String::as_str),
        Some("ref001")
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
TY  - JOUR
AU  - Doe, Jane
ER  -
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
    assert!(!output.contains("AU  -"));
    assert!(!output.contains("PY  -"));
    assert!(!output.contains("JO  -"));
}

#[test]
fn serialize_pages_split() {
    let record = Record {
        title: "Test".into(),
        pages: Some("123-456".into()),
        ..Default::default()
    };
    let output = serialize(&[record]);
    assert!(output.contains("SP  - 123"));
    assert!(output.contains("EP  - 456"));
}

#[test]
fn serialize_en_dash_pages() {
    let record = Record {
        title: "Test".into(),
        pages: Some("123\u{2013}456".into()),
        ..Default::default()
    };
    let output = serialize(&[record]);
    assert!(output.contains("SP  - 123"));
    assert!(output.contains("EP  - 456"));
}

// -- RIS-specific proptest helpers --

fn format_ris_date(date: PublicationDate) -> String {
    let month = date.month.map_or(String::new(), |m| format!("{m:02}"));
    let day = date.day.map_or(String::new(), |d| format!("{d:02}"));
    format!("{}/{month}/{day}/", date.year)
}

fn arb_ris_record() -> impl Strategy<Value = Record> {
    (
        arb_text(80),
        proptest::collection::vec(arb_author(), 0..5),
        proptest::option::of(arb_date()),
        proptest::option::of(arb_text(60)),
        proptest::option::of("10\\.[0-9]{4}/[A-Za-z0-9().]{3,30}"), // no hyphens — would break page split
        proptest::option::of(arb_pages()),
        proptest::option::of(arb_volume()),
        proptest::option::of("[0-9]{1,4}"),
        proptest::option::of(arb_text(200)),
        (
            proptest::option::of("[0-9]{4}-[0-9]{3}[0-9X]"),
            proptest::option::of("[A-Za-z0-9]{1,10}"),
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
                (isbn, id),
            )| {
                let mut extras = HashMap::new();
                if let Some(id) = id {
                    extras.insert("ID".into(), id);
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
        let formatted = format_ris_date(date);
        let reparsed = parse(&format!("TY  - JOUR\nTI  - t\nPY  - {formatted}\nER  -\n"))
            .unwrap()[0]
            .date
            .unwrap();
        prop_assert_eq!(date, reparsed);
    }

    #[test]
    fn serialize_round_trip(record in arb_ris_record()) {
        let serialized = serialize(std::slice::from_ref(&record));
        let reparsed = parse(&serialized).unwrap();
        prop_assert_eq!(vec![record], reparsed);
    }

    #[test]
    fn adversarial_values_round_trip(
        tag_like in prop::sample::select(&["TI  - nested", "AU  - trap", "ER  -"]),
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

    #[test]
    fn adversarial_parse_no_panic(
        prefix in "(TY  - JOUR\n|TI  - |AU  - |ER  -\n|\\r\\n|\n){0,5}",
        body in "\\PC{0,200}",
        suffix in "(\n\n|\r\n\r\n|\n){0,3}",
    ) {
        let input = format!("{prefix}{body}{suffix}");
        let _ = parse(&input);
    }
}
