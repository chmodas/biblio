mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::{select, subsequence};
use std::collections::HashMap;
use std::fmt::Write as _;
use test_strategy::{Arbitrary, proptest};

use biblio::ris::{parse, serialize};
use biblio::{Error, PublicationDate, Record};

use common::{arb_author, arb_date, arb_pages, arb_raw_value, arb_text, arb_volume};

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

// -- Journal --

/// The tags that carry a journal name, in order of preference. This is a copy of the parser's
/// list, so that the tests pin the order independently of the implementation.
const JOURNAL_TAGS: [&str; 6] = ["JF", "T2", "JO", "JA", "J2", "J1"];

/// Parse a single record whose body, after the type and title, is `tag_lines`, and return its
/// journal.
fn journal_of(tag_lines: &str) -> Option<String> {
    let input = format!("TY  - JOUR\nTI  - A test title\n{tag_lines}ER  -\n");
    parse(&input).unwrap().remove(0).journal
}

/// The common layout: the full name in `T2` and the abbreviation in `J2`. The full name wins.
#[test]
fn t2_preferred_over_j2() {
    let journal = journal_of("T2  - Journal of Testing\nJ2  - J. Test.\n");
    assert_eq!(journal.as_deref(), Some("Journal of Testing"));
}

/// The preferred tag wins even when a less preferred one comes first.
#[test]
fn preference_ignores_tag_order() {
    let journal = journal_of("J2  - J. Test.\nT2  - Journal of Testing\n");
    assert_eq!(journal.as_deref(), Some("Journal of Testing"));
}

/// `T2` on its own is read as the journal.
#[test]
fn t2_only() {
    let journal = journal_of("T2  - Journal of Testing\n");
    assert_eq!(journal.as_deref(), Some("Journal of Testing"));
}

/// `JF`, the full name, is preferred over `T2`.
#[test]
fn jf_preferred_over_t2() {
    let journal = journal_of("T2  - Secondary Title\nJF  - Journal of Testing\n");
    assert_eq!(journal.as_deref(), Some("Journal of Testing"));
}

/// `JO` is preferred over both abbreviation tags.
#[test]
fn jo_preferred_over_ja_and_j2() {
    let journal = journal_of("JA  - J Test\nJ2  - J. Test.\nJO  - Journal of Testing\n");
    assert_eq!(journal.as_deref(), Some("Journal of Testing"));
}

/// `JA` on its own is read as the journal.
#[test]
fn ja_only() {
    let journal = journal_of("JA  - J Test\n");
    assert_eq!(journal.as_deref(), Some("J Test"));
}

/// `J1`, the user abbreviation, is used only when no other journal tag is present.
#[test]
fn j1_is_last_fallback() {
    assert_eq!(journal_of("J1  - JT\n").as_deref(), Some("JT"));
    assert_eq!(
        journal_of("J1  - JT\nJ2  - J. Test.\n").as_deref(),
        Some("J. Test.")
    );
}

/// `J2` on its own is read as the journal.
#[test]
fn j2_only() {
    let journal = journal_of("J2  - J. Test.\n");
    assert_eq!(journal.as_deref(), Some("J. Test."));
}

/// `JF` is preferred over `JO` whichever comes last.
#[test]
fn jf_preferred_over_jo_in_either_order() {
    let full_name_first = journal_of("JF  - Journal of Testing\nJO  - J Test\n");
    let full_name_last = journal_of("JO  - J Test\nJF  - Journal of Testing\n");
    assert_eq!(full_name_first.as_deref(), Some("Journal of Testing"));
    assert_eq!(full_name_last.as_deref(), Some("Journal of Testing"));
}

/// A journal name wrapped onto a continuation line is joined with a single space.
#[test]
fn journal_continuation_joined() {
    let journal = journal_of("T2  - Journal of\nTesting and Verification\n");
    assert_eq!(
        journal.as_deref(),
        Some("Journal of Testing and Verification")
    );
}

/// A continuation line extends only the journal tag it follows.
#[test]
fn journal_continuation_appends_to_its_own_tag() {
    let continued_full_name = journal_of("T2  - Journal of\nTesting\nJ2  - J. Test.\n");
    assert_eq!(continued_full_name.as_deref(), Some("Journal of Testing"));

    let continued_abbreviation = journal_of("T2  - Journal of Testing\nJ2  - J.\nTest.\n");
    assert_eq!(
        continued_abbreviation.as_deref(),
        Some("Journal of Testing")
    );
}

/// A blank tag does not hide a less preferred tag that has a value.
#[test]
fn blank_journal_tag_does_not_hide_less_preferred_tag() {
    let journal = journal_of("T2  -\nJO  - Nature\n");
    assert_eq!(journal.as_deref(), Some("Nature"));
}

/// A repeated journal tag replaces its own earlier value, even with a blank one, which then gives
/// way to a less preferred tag.
#[test]
fn repeated_journal_tag_replaces_its_value() {
    let replaced = journal_of("T2  - Old Title\nT2  - New Title\n");
    assert_eq!(replaced.as_deref(), Some("New Title"));

    let blanked = journal_of("T2  - Nature\nT2  -\nJ2  - Nat.\n");
    assert_eq!(blanked.as_deref(), Some("Nat."));
}

/// A blank title continued on the next line becomes the continuation text, without a leading
/// space.
#[test]
fn blank_title_continued_without_leading_space() {
    let records = parse("TY  - JOUR\nTI  -\nA test title\nER  -\n").unwrap();
    assert_eq!(records[0].title, "A test title");
}

/// A blank field continued over two lines joins the lines with a single space.
#[test]
fn blank_field_continued_over_two_lines() {
    let journal = journal_of("T2  -\nJournal of\nTesting\n");
    assert_eq!(journal.as_deref(), Some("Journal of Testing"));
}

/// A blank journal tag continued on the next line becomes the continuation text, without a
/// leading space.
#[test]
fn blank_journal_continued_without_leading_space() {
    assert_eq!(
        journal_of("T2  -\nJournal of Testing\n").as_deref(),
        Some("Journal of Testing")
    );
}

/// A blank author continued on the next line becomes the continuation text, without a leading
/// space.
#[test]
fn blank_author_continued_without_leading_space() {
    let records = parse("TY  - JOUR\nTI  - A test title\nAU  -\nDoe, Jane\nER  -\n").unwrap();
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

/// A blank journal tag with nothing better is kept as an empty journal, as other blank tags are.
#[test]
fn blank_only_journal_tag_kept() {
    let journal = journal_of("T2  -\n");
    assert_eq!(journal.as_deref(), Some(""));
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
        proptest::option::of("10\\.[0-9]{4}/[A-Za-z0-9().]{3,30}"), // no hyphens – would break page split
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

/// Parsing arbitrary printable text never panics.
#[proptest]
fn parse_never_panics(#[strategy("\\PC{0,500}")] s: String) {
    let _ = parse(&s);
}

/// A date written in the `RIS` date format parses back to the same date.
#[proptest]
fn date_round_trip(#[strategy(arb_date())] date: PublicationDate) {
    let formatted = format_ris_date(date);
    let reparsed = parse(&format!("TY  - JOUR\nTI  - t\nPY  - {formatted}\nER  -\n")).unwrap()[0]
        .date
        .unwrap();
    prop_assert_eq!(date, reparsed);
}

/// Serialising a record and parsing the output gives back the same record.
#[proptest]
fn serialize_round_trip(#[strategy(arb_ris_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap();
    prop_assert_eq!(vec![record], reparsed);
}

/// Values containing tag-like text survive a round trip, because that text appears mid-line,
/// not at the start of a line.
#[proptest]
fn adversarial_values_round_trip(
    #[strategy(select(&["TI  - nested", "AU  - trap", "ER  -"]))] tag_like: &'static str,
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

/// Parsing arbitrary text wrapped in fragments of `RIS` syntax (record and field openers,
/// closers and line endings) never panics.
#[proptest]
fn adversarial_parse_no_panic(
    #[strategy("(TY  - JOUR\n|TI  - |AU  - |ER  -\n|\\r\\n|\n){0,5}")] prefix: String,
    #[strategy("\\PC{0,200}")] body: String,
    #[strategy("(\n\n|\r\n\r\n|\n){0,3}")] suffix: String,
) {
    let input = format!("{prefix}{body}{suffix}");
    let _ = parse(&input);
}

// -- Journal properties --

/// The journal tags of one record: distinct tags in any order, each with a raw value.
#[derive(Arbitrary, Debug)]
struct JournalTags {
    /// Distinct journal tags, shuffled. Because no tag repeats, no value overwrites another.
    #[strategy(subsequence(JOURNAL_TAGS.to_vec(), 1..=JOURNAL_TAGS.len()).prop_shuffle())]
    tags: Vec<&'static str>,
    /// One raw value for each entry in `tags`, in the same order.
    #[strategy(vec(arb_raw_value(), #tags.len()))]
    values: Vec<String>,
}

/// A value under any journal tag parses to that value, without trailing whitespace.
#[proptest]
fn journal_from_any_tag(
    #[strategy(select(JOURNAL_TAGS.to_vec()))] tag: &'static str,
    #[strategy(arb_raw_value())] value: String,
) {
    let journal = journal_of(&format!("{tag}  - {value}\n"));
    prop_assert_eq!(journal.as_deref(), Some(value.trim_end()));
}

/// The parsed journal matches a direct reference: order the values by tag preference, then take
/// the first non-blank value, or the first value if all are blank.
#[proptest]
fn journal_preference_matches_reference(input: JournalTags) {
    let mut tag_lines = String::new();
    for (tag, value) in input.tags.iter().zip(&input.values) {
        writeln!(tag_lines, "{tag}  - {value}").unwrap();
    }
    let journal = journal_of(&tag_lines);

    let mut by_preference: Vec<(usize, String)> = input
        .tags
        .into_iter()
        .zip(input.values)
        .map(|(tag, value)| {
            let rank = JOURNAL_TAGS.iter().position(|t| *t == tag).unwrap();
            (rank, value.trim_end().to_owned())
        })
        .collect();
    by_preference.sort_by_key(|(rank, _)| *rank);
    let expected = by_preference
        .iter()
        .find(|(_, value)| !value.is_empty())
        .or(by_preference.first())
        .map(|(_, value)| value.clone());

    prop_assert_eq!(journal, expected);
}

// -- Continuation properties --

/// Text for a continuation line: no control characters, does not start with whitespace and has no
/// `-`, so it is never blank and never read as a tag line.
const CONTINUATION_TEXT: &str = "[^\\s\\p{C}-][^\\p{C}-]{0,59}";

/// A blank field of any continuable tag, continued on the next line, becomes exactly the trimmed
/// continuation text.
#[proptest]
fn blank_field_continued_is_continuation_text(
    #[strategy(select(&["TI", "T1", "AB", "N2", "AU", "A1", "JF", "T2", "JO", "JA", "J2", "J1"]))]
    tag: &'static str,
    #[strategy(CONTINUATION_TEXT)] text: String,
) {
    // A title tag must be the only title, so that the blank one is the one read.
    let title = if matches!(tag, "TI" | "T1") {
        ""
    } else {
        "TI  - t\n"
    };
    let input = format!("TY  - JOUR\n{title}{tag}  -\n{text}\nER  -\n");
    let record = parse(&input).unwrap().remove(0);

    let field = match tag {
        "TI" | "T1" => Some(record.title),
        "AB" | "N2" => record.abstract_text,
        "AU" | "A1" => record.authors.last().cloned(),
        _ => record.journal,
    };
    prop_assert_eq!(field.as_deref(), Some(text.trim()));
}
