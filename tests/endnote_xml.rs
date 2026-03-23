mod common;

use proptest::prelude::*;
use std::collections::HashMap;

use biblio::endnote_xml::{parse, serialize};
use biblio::{Error, PublicationDate, Record};

use common::{arb_author, arb_date, arb_pages, arb_volume};

// -- Core parsing --

#[test]
fn single_record() {
    let input = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml>
<records>
<record>
  <rec-number>42</rec-number>
  <titles><title>A test title</title></titles>
  <contributors><authors>
    <author>Doe, Jane</author>
    <author>Smith, John</author>
  </authors></contributors>
  <dates><year>2024</year></dates>
  <periodical><full-title>Nature</full-title></periodical>
  <electronic-resource-num>10.1038/s41586-024-07386-0</electronic-resource-num>
  <pages>123-130</pages>
  <volume>620</volume>
  <number>3</number>
  <abstract>This is the abstract text.</abstract>
  <isbn>1234-5678</isbn>
  <label>ref001</label>
</record>
</records>
</xml>"#;

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
    assert_eq!(r.extras.get("label").map(String::as_str), Some("ref001"));
    assert_eq!(r.extras.get("rec-number").map(String::as_str), Some("42"));
}

#[test]
fn multiple_records() {
    let input = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml>
<records>
<record>
  <titles><title>CRISPR-Cas9 gene editing</title></titles>
  <contributors><authors>
    <author>Frangoul, Haydar</author>
    <author>Altshuler, David</author>
  </authors></contributors>
  <dates><year>2024</year></dates>
  <periodical><full-title>The New England Journal of Medicine</full-title></periodical>
  <electronic-resource-num>10.1056/NEJMoa2031054</electronic-resource-num>
  <pages>175-186</pages>
  <volume>390</volume>
  <isbn>1533-4406</isbn>
</record>
<record>
  <titles><title>Large language models in clinical medicine</title></titles>
  <contributors><authors>
    <author>Lee, Peter</author>
    <author>Bubeck, Sebastien</author>
  </authors></contributors>
  <dates><year>2024</year></dates>
  <periodical><full-title>Nature Medicine</full-title></periodical>
  <electronic-resource-num>10.1038/s41591-024-02893-1</electronic-resource-num>
  <isbn>1546-170X</isbn>
</record>
</records>
</xml>"#;

    let records = parse(input).unwrap();
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].title, "CRISPR-Cas9 gene editing");
    assert_eq!(
        records[0].authors,
        vec!["Frangoul, Haydar", "Altshuler, David"]
    );
    assert_eq!(records[0].doi.as_deref(), Some("10.1056/NEJMoa2031054"));
    assert_eq!(records[0].pages.as_deref(), Some("175-186"));
    assert_eq!(records[0].isbn.as_deref(), Some("1533-4406"));

    assert_eq!(
        records[1].title,
        "Large language models in clinical medicine"
    );
    assert_eq!(records[1].authors, vec!["Lee, Peter", "Bubeck, Sebastien"]);
    assert_eq!(records[1].isbn.as_deref(), Some("1546-170X"));
}

#[test]
fn bare_records_root() {
    let input = r"<records>
<record>
  <titles><title>Bare root test</title></titles>
  <dates><year>2023</year></dates>
</record>
</records>";

    let records = parse(input).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].title, "Bare root test");
}

// -- Style tag stripping --

#[test]
fn style_tags_stripped() {
    let input = r#"<?xml version="1.0" encoding="UTF-8"?>
<xml>
<records>
<record>
  <titles><title><style face="normal" font="default" size="100%">Styled title</style></title></titles>
  <abstract><style face="normal" font="default" size="100%">Styled abstract</style></abstract>
</record>
</records>
</xml>"#;

    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "Styled title");
    assert_eq!(records[0].abstract_text.as_deref(), Some("Styled abstract"));
}

#[test]
fn unclosed_style_opening_tag() {
    // A <style tag with no closing > anywhere – the strip function breaks
    // out. We feed this through strip_style_tags indirectly via parse, and
    // verify no panic. The result is malformed XML so parse will error.
    let input = "text before <style face=normal -- no closing angle bracket";
    let _ = parse(input);
}

#[test]
fn self_closing_elements() {
    let input = r"<xml><records>
<record>
  <titles><title>Test</title></titles>
  <pages/>
  <volume/>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(records[0].pages, None);
    assert_eq!(records[0].volume, None);
}

#[test]
fn numeric_char_ref_decoded() {
    let input = r"<xml><records>
<record>
  <titles><title>A &#38; B</title></titles>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A & B");
}

#[test]
fn style_prefix_not_stripped() {
    // <stylesheet> should NOT be matched by the <style> stripping logic.
    let input = r"<xml><records>
<record>
  <titles><title>Test</title></titles>
  <stylesheet>should survive</stylesheet>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "Test");
}

#[test]
fn xml_entities_decoded() {
    let input = r"<xml><records>
<record>
  <titles><title>A &amp; B &lt; C</title></titles>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(records[0].title, "A & B < C");
}

// -- Authors --

#[test]
fn no_contributors() {
    let input = r"<xml><records>
<record>
  <titles><title>No authors</title></titles>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert!(records[0].authors.is_empty());
}

// -- Dates --

#[test]
fn date_non_numeric_ignored() {
    let input = r"<xml><records>
<record>
  <titles><title>Test</title></titles>
  <dates><year>forthcoming</year></dates>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(records[0].date, None);
}

#[test]
fn date_year_only() {
    let input = r"<xml><records>
<record>
  <titles><title>Test</title></titles>
  <dates><year>2024</year></dates>
</record>
</records></xml>";

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

// -- Extras --

#[test]
fn extras_label_and_rec_number() {
    let input = r"<xml><records>
<record>
  <rec-number>99</rec-number>
  <titles><title>Test</title></titles>
  <label>myref</label>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].extras.get("label").map(String::as_str),
        Some("myref")
    );
    assert_eq!(
        records[0].extras.get("rec-number").map(String::as_str),
        Some("99")
    );
}

#[test]
fn extras_label_only() {
    let input = r"<xml><records>
<record>
  <titles><title>Test</title></titles>
  <label>onlylabel</label>
</record>
</records></xml>";

    let records = parse(input).unwrap();
    assert_eq!(
        records[0].extras.get("label").map(String::as_str),
        Some("onlylabel")
    );
    assert_eq!(records[0].extras.get("rec-number"), None);
}

// -- Error cases --

#[test]
fn empty_input() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("   \n\n  "), Err(Error::EmptyInput));
}

#[test]
fn malformed_xml() {
    let result = parse("<xml><records><record><not closed");
    assert!(result.is_err());
}

#[test]
fn missing_title() {
    let input = r"<xml><records>
<record>
  <dates><year>2024</year></dates>
</record>
</records></xml>";

    let err = parse(input).unwrap_err();
    assert!(matches!(
        err,
        Error::MissingRequiredField { tag: "title", .. }
    ));
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
    assert!(output.contains("<title>Minimal</title>"));
    assert!(!output.contains("<volume>"));
    assert!(!output.contains("<pages>"));
    assert!(!output.contains("<isbn>"));
}

// -- Proptest helpers --

fn arb_endnote_record() -> impl Strategy<Value = Record> {
    // XML-safe text: no < > & which would break XML structure
    let xml_safe = "[A-Za-z0-9 .,;:!?'()\\[\\]/=#%^*+_~-]";
    let arb_xml_text = move |max_len: usize| {
        proptest::string::string_regex(&format!("{xml_safe}{{1,{max_len}}}"))
            .unwrap()
            .prop_map(|s| s.trim().to_owned())
            .prop_filter("must not be empty after trim", |s| !s.is_empty())
    };

    (
        arb_xml_text(80),                              // title
        proptest::collection::vec(arb_author(), 0..5), // authors
        proptest::option::of(arb_date().prop_map(|d| {
            // EndNote XML only stores year
            PublicationDate {
                year: d.year,
                month: None,
                day: None,
            }
        })),
        proptest::option::of(arb_xml_text(60)), // journal
        proptest::option::of("10\\.[0-9]{4}/[A-Za-z0-9().]{3,20}"), // doi
        proptest::option::of(arb_pages()),
        proptest::option::of(arb_volume()),
        proptest::option::of("[0-9]{1,4}"),      // number
        proptest::option::of(arb_xml_text(200)), // abstract
        (
            proptest::option::of("[0-9]{4}-[0-9]{3}[0-9X]"), // ISSN
            proptest::option::of("[A-Za-z0-9]{1,10}"),       // label
            proptest::option::of("[0-9]{1,6}"),              // rec-number
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
                (isbn, label, rec_number),
            )| {
                let mut extras = HashMap::new();
                if let Some(label) = label {
                    extras.insert("label".into(), label);
                }
                if let Some(rn) = rec_number {
                    extras.insert("rec-number".into(), rn);
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
    fn serialize_round_trip(record in arb_endnote_record()) {
        let serialized = serialize(std::slice::from_ref(&record));
        let reparsed = parse(&serialized).unwrap();
        prop_assert_eq!(vec![record], reparsed);
    }

    #[test]
    fn adversarial_parse_no_panic(
        prefix in "(<xml>|<records>|<record>|</){0,5}",
        body in "\\PC{0,200}",
        suffix in "(</record>|</records>|</xml>){0,3}",
    ) {
        let input = format!("{prefix}{body}{suffix}");
        let _ = parse(&input);
    }
}
