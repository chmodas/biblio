mod common;

use proptest::prelude::*;
use std::collections::HashMap;
use test_strategy::proptest;

use biblio::endnote_xml::{parse, serialize};
use biblio::{Diagnostic, Error, PublicationDate, Record};

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

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 2);

    assert_eq!(
        records[0].title.as_deref(),
        Some("CRISPR-Cas9 gene editing")
    );
    assert_eq!(
        records[0].authors,
        vec!["Frangoul, Haydar", "Altshuler, David"]
    );
    assert_eq!(records[0].doi.as_deref(), Some("10.1056/NEJMoa2031054"));
    assert_eq!(records[0].pages.as_deref(), Some("175-186"));
    assert_eq!(records[0].isbn.as_deref(), Some("1533-4406"));

    assert_eq!(
        records[1].title.as_deref(),
        Some("Large language models in clinical medicine")
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

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].title.as_deref(), Some("Bare root test"));
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

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("Styled title"));
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

    let records = parse(input).unwrap().records;
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

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A & B"));
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

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("Test"));
}

#[test]
fn xml_entities_decoded() {
    let input = r"<xml><records>
<record>
  <titles><title>A &amp; B &lt; C</title></titles>
</record>
</records></xml>";

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A & B < C"));
}

// -- Authors --

#[test]
fn no_contributors() {
    let input = r"<xml><records>
<record>
  <titles><title>No authors</title></titles>
</record>
</records></xml>";

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors, Vec::<String>::new());
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

    let records = parse(input).unwrap().records;
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

    let records = parse(input).unwrap().records;
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

    let records = parse(input).unwrap().records;
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

/// Broken XML before any record is complete fails the whole input, at the line of the error.
#[test]
fn malformed_xml_before_first_record() {
    let result = parse("<xml><records>\n<record><titles><title>A</wrong>");
    assert!(matches!(
        result,
        Err(Error::MalformedSyntax { line: 2, .. })
    ));
}

// -- Lenient parsing --

/// Titles of the parsed records, in order.
fn titles(parsed: &biblio::Parsed) -> Vec<Option<&str>> {
    parsed.records.iter().map(|r| r.title.as_deref()).collect()
}

/// An XML error after complete records keeps them and reports the truncation at the error's line.
#[test]
fn xml_error_after_records_truncates() {
    let input = "\
<xml><records>
<record><titles><title>A</title></titles></record>
<record><titles><title>B</title></titles></record>
<record><titles><title>C</title></wrong>
</records></xml>";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::Truncated { line: 4, .. }]
    ));
}

/// Input ending inside a record keeps the complete records and reports where the unfinished one
/// starts.
#[test]
fn end_inside_record_truncates() {
    let input = "\
<xml><records>
<record><titles><title>A</title></titles></record>
<record><titles><title>B";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A")]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::Truncated {
            line: 3,
            message: "input ended inside a record".into()
        }]
    );
}

/// Input ending inside the first record fails the whole input.
#[test]
fn end_inside_first_record_fails() {
    let result = parse("<xml><records>\n<record><titles><title>A");
    assert!(matches!(
        result,
        Err(Error::MalformedSyntax { line: 2, .. })
    ));
}

/// Input ending right after a record, with `<records>` still open, is not truncated.
#[test]
fn end_after_record_not_truncated() {
    let parsed = parse("<xml><records><record><titles><title>A</title></titles></record>").unwrap();
    assert_eq!(titles(&parsed), vec![Some("A")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// Line numbers count the newlines inside removed `<style>` tags.
#[test]
fn lines_counted_across_style_tags() {
    let input = "\
<xml><records>
<record><titles><title><style face=\"bold\"
 size=\"100%\">A</style></title></titles></record>
<record>
<dates><year>2024</year></dates>
</record>
</records></xml>";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), None]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 4 }]
    );
}

/// A record without a title is kept, and the missing title is reported at the record's line.
#[test]
fn missing_title_kept_and_reported() {
    let input = r"<xml><records>
<record>
  <dates><year>2024</year></dates>
</record>
</records></xml>";

    let parsed = parse(input).unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title, None);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 2 }]
    );
}

// -- Serialization --

#[test]
fn serialize_multiple_records() {
    let a = Record::builder().title("First").build();
    let b = Record::builder().title("Second").build();
    let output = serialize(&[a, b]);
    let reparsed = parse(&output).unwrap().records;
    assert_eq!(reparsed.len(), 2);
    assert_eq!(reparsed[0].title.as_deref(), Some("First"));
    assert_eq!(reparsed[1].title.as_deref(), Some("Second"));
}

#[test]
fn serialize_skips_none_fields() {
    let record = Record::builder().title("Minimal").build();
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
        proptest::option::of(arb_xml_text(80)),        // title
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
fn serialize_round_trip(#[strategy(arb_endnote_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
    prop_assert_eq!(vec![record], reparsed);
}

/// Parsing arbitrary text wrapped in fragments of `EndNote XML` syntax (element openers and
/// closers) never panics.
#[proptest]
fn adversarial_parse_no_panic(
    #[strategy("(<xml>|<records>|<record>|</){0,5}")] prefix: String,
    #[strategy("\\PC{0,200}")] body: String,
    #[strategy("(</record>|</records>|</xml>){0,3}")] suffix: String,
) {
    let input = format!("{prefix}{body}{suffix}");
    let _ = parse(&input);
}

// -- Lenient parsing properties --

/// Cutting a serialised file anywhere keeps exactly the records completed before the cut. A cut
/// inside a record or tag is reported as truncation, or fails the input if no record is complete.
#[proptest]
fn truncation_keeps_complete_records(
    #[strategy(proptest::collection::vec(arb_endnote_record(), 1..4))] records: Vec<Record>,
    cut: proptest::sample::Index,
) {
    let serialized = serialize(&records);
    let mut end = 1 + cut.index(serialized.len());
    while !serialized.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &serialized[..end];

    // The serialiser escapes `<`, `>` and `&` in text and writes nothing between elements, so the
    // prefix ends inside markup or text exactly when it does not end with `>` or when a record has
    // been opened but not closed.
    let complete = prefix.matches("</record>").count();
    let opened = prefix.matches("<record>").count();
    let broken_off = !prefix.ends_with('>') || opened > complete;

    match parse(prefix) {
        Ok(parsed) => {
            prop_assert!(complete > 0 || !broken_off);
            prop_assert_eq!(&parsed.records[..], &records[..complete]);
            let truncations: Vec<usize> = parsed
                .diagnostics
                .iter()
                .enumerate()
                .filter(|(_, d)| matches!(d, Diagnostic::Truncated { .. }))
                .map(|(index, _)| index)
                .collect();
            if broken_off {
                prop_assert_eq!(truncations, vec![parsed.diagnostics.len() - 1]);
            } else {
                prop_assert!(truncations.is_empty());
            }
        }
        Err(error) => {
            prop_assert!(broken_off && complete == 0, "unexpected error {error}");
            let malformed = matches!(error, Error::MalformedSyntax { .. });
            prop_assert!(malformed);
        }
    }
}

/// Each record without a title is reported once, and diagnostics come in line order.
#[proptest]
fn missing_titles_reported_in_order(
    #[strategy(proptest::collection::vec(proptest::bool::ANY, 1..8))] titled: Vec<bool>,
) {
    let mut input = String::from("<xml><records>\n");
    for has_title in &titled {
        if *has_title {
            input.push_str("<record><titles><title>T</title></titles></record>\n");
        } else {
            input.push_str("<record><dates><year>2024</year></dates></record>\n");
        }
    }
    input.push_str("</records></xml>");
    let parsed = parse(&input).unwrap();

    let untitled: Vec<usize> = titled
        .iter()
        .enumerate()
        .filter(|(_, has_title)| !**has_title)
        .map(|(index, _)| index + 2)
        .collect();
    let missing: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    prop_assert_eq!(missing, untitled);
}
