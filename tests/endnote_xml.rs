mod common;

use proptest::prelude::*;
use std::collections::HashMap;
use std::fmt::Write as _;
use test_strategy::proptest;

use biblio::endnote_xml::{parse, serialize};
use biblio::{Diagnostic, Error, PublicationDate, Record};

use common::{
    arb_author, arb_blank, arb_date, arb_keyword, arb_pages, arb_url, arb_volume, first_extra,
};

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
    assert_eq!(r.label.as_deref(), Some("ref001"));
    assert_eq!(first_extra(r, "endnote:rec-number"), Some("42"));
    // The whitespace inside containers such as <authors> is not a value.
    assert_eq!(r.extras.len(), 1, "{:?}", r.extras);
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

/// Elements without a field are kept under `endnote:` with their path below `<record>`, repeated
/// elements keeping every value in order.
#[test]
fn unread_elements_kept_by_path() {
    let input = r#"<xml><records><record>
  <ref-type name="Journal Article">17</ref-type>
  <titles><title>T</title><secondary-title>Nature</secondary-title></titles>
  <keywords><keyword>cats</keyword><keyword>dogs</keyword></keywords>
  <urls><related-urls><url>https://example.org</url></related-urls></urls>
</record></records></xml>"#;
    let records = parse(input).unwrap().records;
    // Keywords and related links have fields of their own.
    assert_eq!(records[0].keywords, vec!["cats", "dogs"]);
    assert_eq!(records[0].urls, vec!["https://example.org"]);
    let extras = &records[0].extras;
    assert_eq!(extras["endnote:ref-type"], vec!["17"]);
    assert_eq!(extras["endnote:titles/secondary-title"], vec!["Nature"]);
    assert_eq!(extras.len(), 2);
}

// -- Source database --

/// Wrap the elements `body` in a single `<record>` with a title, and parse it.
fn parse_record(body: &str) -> Record {
    let input = format!(
        "<xml><records><record><titles><title>t</title></titles>{body}</record></records></xml>"
    );
    parse(&input).unwrap().records.remove(0)
}

/// `remote-database-name` is read as the database, with the style tags `EndNote` wraps it in
/// removed, and is not kept in extras.
#[test]
fn remote_database_name_read_as_database() {
    let record = parse_record(
        r#"<remote-database-name><style face="normal" font="default" size="100%">Scopus</style></remote-database-name>"#,
    );
    assert_eq!(record.database.as_deref(), Some("Scopus"));
    assert!(!record.extras.contains_key("endnote:remote-database-name"));
}

/// A repeated `remote-database-name` keeps its last value.
#[test]
fn repeated_remote_database_name_keeps_last() {
    let record = parse_record(
        "<remote-database-name>Embase</remote-database-name>\
         <remote-database-name>MEDLINE</remote-database-name>",
    );
    assert_eq!(record.database.as_deref(), Some("MEDLINE"));
}

/// The database provider is not the database, and stays in extras.
#[test]
fn remote_database_provider_kept_in_extras() {
    let record =
        parse_record("<remote-database-provider>Ovid Technologies</remote-database-provider>");
    assert_eq!(record.database, None);
    assert_eq!(
        first_extra(&record, "endnote:remote-database-provider"),
        Some("Ovid Technologies")
    );
}

/// The database is written as `<remote-database-name>` directly under `<record>`, after `<isbn>`.
#[test]
fn serialize_writes_remote_database_name() {
    let record = Record::builder()
        .title("T")
        .isbn("1234-5678")
        .database("PubMed")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains(
        "<isbn>1234-5678</isbn><remote-database-name>PubMed</remote-database-name></record>"
    ));
}

/// A blank database read from `RIS` is written to `EndNote XML` as an empty element, which reads
/// back as no database.
#[test]
fn blank_database_from_ris_reads_back_as_none() {
    let from_ris = biblio::ris::parse("TY  - JOUR\nTI  - t\nDB  -\nER  -\n")
        .unwrap()
        .records;
    assert_eq!(from_ris[0].database.as_deref(), Some(""));
    let reparsed = parse(&serialize(&from_ris)).unwrap().records;
    assert_eq!(reparsed[0].database, None);
}

/// The database read from `EndNote XML` is carried through a conversion to `RIS` and back.
#[test]
fn database_carried_from_endnote_to_ris() {
    let records = vec![parse_record(
        "<remote-database-name>CINAHL</remote-database-name>",
    )];
    let converted = biblio::ris::parse(&biblio::ris::serialize(&records))
        .unwrap()
        .records;
    assert_eq!(converted[0].database.as_deref(), Some("CINAHL"));
}

// -- Label --

/// `label` is read as the label and not kept in extras; `rec-number` stays an extra.
#[test]
fn label_read_as_label() {
    let record = parse_record("<rec-number>99</rec-number><label>myref</label>");
    assert_eq!(record.label.as_deref(), Some("myref"));
    assert!(!record.extras.contains_key("endnote:label"));
    assert_eq!(first_extra(&record, "endnote:rec-number"), Some("99"));
}

/// The style tags `EndNote` wraps a label in are removed.
#[test]
fn label_wrapped_in_style_tags() {
    let record = parse_record(
        r#"<label><style face="normal" font="default" size="100%">OLD</style></label>"#,
    );
    assert_eq!(record.label.as_deref(), Some("OLD"));
}

/// A repeated `label` keeps its last value.
#[test]
fn repeated_label_keeps_last() {
    let record = parse_record("<label>OLD</label><label>NEW</label>");
    assert_eq!(record.label.as_deref(), Some("NEW"));
}

/// The label is written as `<label>` directly under `<record>`, after `<remote-database-name>`.
#[test]
fn serialize_writes_label() {
    let record = Record::builder()
        .title("T")
        .database("PubMed")
        .label("OLD")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("</remote-database-name><label>OLD</label></record>"));
}

/// A blank label read from `RIS` is written to `EndNote XML` as an empty element, which reads back
/// as no label.
#[test]
fn blank_label_from_ris_reads_back_as_none() {
    let from_ris = biblio::ris::parse("TY  - JOUR\nTI  - t\nLB  -\nER  -\n")
        .unwrap()
        .records;
    assert_eq!(from_ris[0].label.as_deref(), Some(""));
    let reparsed = parse(&serialize(&from_ris)).unwrap().records;
    assert_eq!(reparsed[0].label, None);
}

/// The label read from `EndNote XML` is carried through a conversion to `RIS` and back.
#[test]
fn label_carried_from_endnote_to_ris() {
    let records = vec![parse_record("<label>Drummond2017</label>")];
    let converted = biblio::ris::parse(&biblio::ris::serialize(&records))
        .unwrap()
        .records;
    assert_eq!(converted[0].label.as_deref(), Some("Drummond2017"));
}

// -- Keywords and links --

/// Each `keywords/keyword` element adds a keyword, with the style tags `EndNote` wraps it in
/// removed, and keywords are not kept in extras.
#[test]
fn keywords_read_with_style_tags() {
    let record = parse_record(
        r#"<keywords><keyword><style face="normal" font="default" size="100%">Animals</style></keyword><keyword>anatomy &amp; histology</keyword></keywords>"#,
    );
    assert_eq!(record.keywords, vec!["Animals", "anatomy & histology"]);
    assert!(!record.extras.contains_key("endnote:keywords/keyword"));
}

/// A keyword of only whitespace is dropped.
#[test]
fn whitespace_only_keyword_dropped() {
    let record =
        parse_record("<keywords><keyword>  </keyword><keyword> cats </keyword></keywords>");
    assert_eq!(record.keywords, vec!["cats"]);
}

/// Each `urls/related-urls/url` element adds a link, and links are not kept in extras.
#[test]
fn related_urls_read() {
    let record = parse_record(
        "<urls><related-urls><url>https://example.org/a</url><url>https://example.org/b</url></related-urls></urls>",
    );
    assert_eq!(
        record.urls,
        vec!["https://example.org/a", "https://example.org/b"]
    );
    assert!(!record.extras.contains_key("endnote:urls/related-urls/url"));
}

/// Links to files are not related links, and stay in extras.
#[test]
fn pdf_urls_kept_in_extras() {
    let record = parse_record("<urls><pdf-urls><url>internal-pdf://a.pdf</url></pdf-urls></urls>");
    assert_eq!(record.urls, Vec::<String>::new());
    assert_eq!(
        first_extra(&record, "endnote:urls/pdf-urls/url"),
        Some("internal-pdf://a.pdf")
    );
}

/// Keywords and links are written after `<label>`, each list in one container, skipping blank
/// values.
#[test]
fn serialize_writes_keywords_and_urls_after_label() {
    let record = Record::builder()
        .title("T")
        .label("OLD")
        .keywords(["cats", " ", "dogs"])
        .url("https://example.org")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains(
        "<label>OLD</label><keywords><keyword>cats</keyword><keyword>dogs</keyword></keywords>\
         <urls><related-urls><url>https://example.org</url></related-urls></urls></record>"
    ));
}

/// A list with no non-blank values writes no container.
#[test]
fn empty_lists_not_written() {
    let record = Record::builder().title("T").keyword("").url("  ").build();
    let output = serialize(&[record]);
    assert!(!output.contains("<keywords>"));
    assert!(!output.contains("<urls>"));
}

/// Keywords and links read from `EndNote XML` are carried through a conversion to `RIS` and back.
#[test]
fn keywords_and_urls_carried_to_ris() {
    let records = vec![parse_record(
        "<keywords><keyword>cats</keyword></keywords><urls><related-urls><url>https://example.org</url></related-urls></urls>",
    )];
    let converted = biblio::ris::parse(&biblio::ris::serialize(&records))
        .unwrap()
        .records;
    assert_eq!(converted[0].keywords, vec!["cats"]);
    assert_eq!(converted[0].urls, vec!["https://example.org"]);
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
/// explicitly (the XML reader skips it too); the test guards it.
#[test]
fn leading_byte_order_mark_ignored() {
    let input =
        "<xml><records><record><titles><title>First</title></titles></record></records></xml>";
    let parsed = parse(&format!("\u{feff}{input}")).unwrap();
    assert_eq!(parsed, parse(input).unwrap());
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title.as_deref(), Some("First"));
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
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

/// A container whose only children are self-closing holds no value, even with indentation inside
/// it. Regression test from code review.
#[test]
fn container_of_empty_elements_has_no_value() {
    let input = "\
<xml><records>
<record>
  <urls>
    <related-urls/>
  </urls>
</record>
</records></xml>";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].urls, Vec::<String>::new());
    assert!(
        !records[0].extras.contains_key("endnote:urls"),
        "{:?}",
        records[0].extras
    );
}

/// Extras of a `<record>` nested inside another are keyed by their path below the inner record.
#[test]
fn nested_record_extras_relative_to_inner_record() {
    let input = "<xml><records><record><record><notes>N</notes></record></record></records></xml>";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].extras["endnote:notes"], vec!["N"]);
}

/// Extras are written as nested elements, one per value, and read back unchanged; extras of other
/// formats are not written.
#[test]
fn serialize_writes_extras_as_nested_elements() {
    let record = Record::builder()
        .title("T")
        .extra("endnote:urls/pdf-urls/url", "a.pdf")
        .extra("endnote:urls/pdf-urls/url", "b.pdf")
        .extra("endnote:ref-type", "17")
        .extra("ris:KW", "not written")
        .build();
    let output = serialize(std::slice::from_ref(&record));
    assert!(output.contains("<urls><pdf-urls><url>a.pdf</url><url>b.pdf</url></pdf-urls></urls>"));
    assert!(!output.contains("not written"));
    let mut expected = record;
    expected.extras.remove("ris:KW");
    assert_eq!(parse(&output).unwrap().records, vec![expected]);
}

/// Extras whose path cannot be written, or that the parser would read into a field, are skipped,
/// and so are extras with only empty values.
#[test]
fn serialize_skips_unwritable_extras() {
    let record = Record::builder()
        .title("T")
        .extra("endnote:a//b", "x")
        .extra("endnote:1x", "x")
        .extra("endnote:x/record", "x")
        .extra("endnote:style", "x")
        .extra("endnote:foo/pages", "would become the pages")
        .extra("endnote:note", "")
        .build();
    let output = serialize(&[record]);
    assert!(
        output.ends_with("<record><titles><title>T</title></titles></record></records></xml>"),
        "{output}"
    );
}

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
    assert!(!output.contains("<label>"));
}

// -- Proptest helpers --

/// The leaf element names that the `EndNote XML` parser reads into fields. Some are read only under
/// a particular container (`title` under `titles`, for instance), but the generator avoids them
/// everywhere, which is simpler and only slightly over-cautious.
const ENDNOTE_READ_LEAVES: [&str; 14] = [
    "title",
    "author",
    "year",
    "full-title",
    "electronic-resource-num",
    "pages",
    "volume",
    "number",
    "abstract",
    "isbn",
    "remote-database-name",
    "label",
    "keyword",
    "url",
];

/// An element path below `<record>` that the parser does not read: one to three names, none of them
/// `record` or `style`, and ending in a name that is not a read leaf.
fn arb_unread_path() -> impl Strategy<Value = String> {
    proptest::collection::vec("[a-z][a-z-]{1,8}", 1..=3).prop_filter_map(
        "path must not be read",
        |segments| {
            let usable = segments.iter().all(|s| s != "record" && s != "style")
                && !ENDNOTE_READ_LEAVES.contains(&segments.last()?.as_str());
            usable.then(|| segments.join("/"))
        },
    )
}

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
            proptest::option::of(arb_xml_text(60)),          // database
            proptest::option::of(arb_xml_text(60)),          // label
            proptest::collection::vec(arb_keyword(), 0..4),
            proptest::collection::vec(arb_url(), 0..3),
            proptest::collection::hash_map(
                arb_unread_path(),
                proptest::collection::vec(arb_xml_text(40), 1..=3),
                0..=4,
            ),
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
                (isbn, database, label, keywords, urls, unread),
            )| {
                let extras: HashMap<String, Vec<String>> = unread
                    .into_iter()
                    .map(|(path, values)| (format!("endnote:{path}"), values))
                    .collect();
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
                    record.database = database;
                    record.label = label;
                    record.keywords = keywords;
                    record.urls = urls;
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

// -- Byte-order mark --

/// Any input, as serialised records or as arbitrary text. It never starts with a byte-order mark:
/// U+FEFF is a format character, which `\PC` excludes, and the serialiser never writes it.
fn arb_input_without_byte_order_mark() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::collection::vec(arb_endnote_record(), 0..4)
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

/// Keys of the other formats, which `EndNote XML` serialisation must never write.
const FOREIGN_KEYS: [&str; 3] = ["ris:KW", "nbib:OT", "bibtex:note"];

/// The element paths the parser reads into fields, in the form it reads them.
const ENDNOTE_READ_PATHS: [&str; 14] = [
    "titles/title",
    "contributors/authors/author",
    "dates/year",
    "periodical/full-title",
    "electronic-resource-num",
    "pages",
    "volume",
    "number",
    "abstract",
    "isbn",
    "remote-database-name",
    "label",
    "keywords/keyword",
    "urls/related-urls/url",
];

/// An extra whose path the parser reads into a field is never written, so it cannot leak into the
/// field.
#[proptest]
fn read_paths_never_written_from_extras(
    #[strategy(proptest::sample::select(ENDNOTE_READ_PATHS.to_vec()))] path: &'static str,
    #[strategy(arb_endnote_record())] record: Record,
) {
    let mut with_read_path = record.clone();
    with_read_path
        .extras
        .insert(format!("endnote:{path}"), vec!["leaked value".to_owned()]);
    prop_assert_eq!(serialize(&[with_read_path]), serialize(&[record]));
}

/// Extras of other formats are kept on the record but never written as elements.
#[proptest]
fn foreign_extras_not_written(
    #[strategy(arb_endnote_record())] record: Record,
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

/// Records read from indented XML with arbitrary unread elements come back unchanged when written
/// and read again; the indentation inside containers never becomes a value.
#[proptest]
fn records_stable_through_serialize(
    #[strategy(proptest::collection::vec(
        (arb_unread_path(), "[A-Za-z0-9][A-Za-z0-9 .,]{0,20}"),
        0..6
    ))]
    elements: Vec<(String, String)>,
) {
    let mut input = String::from(
        "<xml>\n  <records>\n    <record>\n      <titles>\n        <title>A title</title>\n",
    );
    input.push_str("      </titles>\n");
    for (path, value) in &elements {
        let segments: Vec<&str> = path.split('/').collect();
        let (leaf, containers) = segments.split_last().unwrap();
        for (depth, container) in containers.iter().enumerate() {
            writeln!(input, "{}<{container}>", "  ".repeat(depth + 3)).unwrap();
        }
        writeln!(
            input,
            "{}<{leaf}>{value}</{leaf}>",
            "  ".repeat(containers.len() + 3)
        )
        .unwrap();
        for (depth, container) in containers.iter().enumerate().rev() {
            writeln!(input, "{}</{container}>", "  ".repeat(depth + 3)).unwrap();
        }
    }
    input.push_str("    </record>\n  </records>\n</xml>\n");
    let first = parse(&input).unwrap().records;
    let extra_values: usize = first[0].extras.values().map(Vec::len).sum();
    prop_assert_eq!(extra_values, elements.len());
    let second = parse(&serialize(&first)).unwrap().records;
    prop_assert_eq!(second, first);
}

/// Blank keywords and links are not written, wherever they are in the list.
#[proptest]
fn blank_list_values_not_written(
    #[strategy(arb_endnote_record())] record: Record,
    #[strategy(arb_blank())] blank: String,
) {
    let mut with_blanks = record.clone();
    for list in [&mut with_blanks.keywords, &mut with_blanks.urls] {
        list.insert(0, blank.clone());
        list.push(blank.clone());
    }
    prop_assert_eq!(serialize(&[with_blanks]), serialize(&[record]));
}

/// Keyword elements with raw values give their trimmed non-blank values in order.
#[proptest]
fn keywords_match_reference(
    #[strategy(proptest::collection::vec("[ \\t\\u{a0}\\u{3000}]{0,2}[A-Za-z0-9 ]{0,12}", 0..6))]
    values: Vec<String>,
) {
    let mut elements = String::new();
    for value in &values {
        write!(elements, "<keyword>{value}</keyword>").unwrap();
    }
    let record = parse_record(&format!("<keywords>{elements}</keywords>"));
    let reference: Vec<&str> = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect();
    prop_assert_eq!(record.keywords, reference);
}
