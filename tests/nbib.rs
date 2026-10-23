mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::{select, subsequence};
use std::collections::HashMap;
use std::fmt::Write as _;
use test_strategy::{Arbitrary, proptest};

use biblio::nbib::{parse, serialize};
use biblio::{Diagnostic, Error, PublicationDate, Record};

use common::{arb_author, arb_date, arb_pages, arb_raw_value, arb_text, arb_volume, first_extra};

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

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 1);

    let r = &records[0];
    assert_eq!(r.title.as_deref(), Some("A test title"));
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
    assert_eq!(first_extra(r, "nbib:PMID"), Some("12345678"));
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

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 2);

    assert_eq!(
        records[0].title.as_deref(),
        Some("CRISPR-Cas9 gene editing for sickle cell disease and beta-thalassemia")
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
        records[1].title.as_deref(),
        Some("Large language models in clinical medicine")
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

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A test title"));
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

/// Tags without a field are kept in `extras` under `nbib:`, with repeated tags keeping every value
/// in order.
#[test]
fn unknown_tags_kept_in_extras() {
    let input = "\
TI  - A test title
OT  - keyword one
OT  - keyword two
MH  - mesh heading
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A test title"));
    let extras = &records[0].extras;
    assert_eq!(extras["nbib:OT"], vec!["keyword one", "keyword two"]);
    assert_eq!(extras["nbib:MH"], vec!["mesh heading"]);
    assert_eq!(extras.len(), 2);
}

/// A continuation line of a tag without a field is joined to that tag's last value.
#[test]
fn unknown_tag_continuation_joined() {
    let records = parse("TI  - t\nAD  - An affiliation that\n      wraps\n")
        .unwrap()
        .records;
    assert_eq!(
        records[0].extras["nbib:AD"],
        vec!["An affiliation that wraps"]
    );
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

    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors, vec!["Doe, Jane", "Smith, John"]);
}

#[test]
fn au_fallback() {
    let input = "\
TI  - A test title
AU  - Doe J
AU  - Smith J
";

    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].title.as_deref(), Some("An ordinary article"));
    assert_eq!(
        records[1].title.as_deref(),
        Some("A very long book title that wraps onto a second line and then a third.")
    );
    assert_eq!(records[1].journal.as_deref(), Some("Reports in Testing"));
    assert_eq!(records[1].authors, vec!["Doe, Jane"]);
}

/// In a chapter record, `TI` holds the chapter title and is preferred over the book title.
#[test]
fn ti_preferred_over_bti() {
    let records = parse("BTI - The Book\nTI  - The Chapter\n")
        .unwrap()
        .records;
    assert_eq!(records[0].title.as_deref(), Some("The Chapter"));
}

/// `JT` is preferred over the collection title.
#[test]
fn jt_preferred_over_cti() {
    let records = parse("TI  - t\nCTI - Reports in Testing\nJT  - Journal of Testing\n")
        .unwrap()
        .records;
    assert_eq!(records[0].journal.as_deref(), Some("Journal of Testing"));
}

/// A blank `TI` does not hide a book title that has a value.
#[test]
fn blank_ti_does_not_hide_bti() {
    let records = parse("TI  - \nBTI - The Book\n").unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("The Book"));
}

/// A blank `JT` does not hide a collection title that has a value.
#[test]
fn blank_jt_does_not_hide_cti() {
    let records = parse("TI  - t\nJT  - \nCTI - Reports in Testing\n")
        .unwrap()
        .records;
    assert_eq!(records[0].journal.as_deref(), Some("Reports in Testing"));
}

/// The collection title is shared by unrelated books, so it is never used as a title.
#[test]
fn collection_title_is_not_a_title() {
    let parsed = parse("CTI - Reports in Testing\nDP  - 2024\n").unwrap();
    assert_eq!(parsed.records[0].title, None);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 1 }]
    );
}

// -- Lenient parsing --

/// `TI  -` without the space after the dash is not a tag line, so it is ignored and reported.
#[test]
fn tag_without_trailing_space_ignored() {
    let parsed = parse("TI  - Kept\nAB  -\n").unwrap();
    assert_eq!(parsed.records[0].title.as_deref(), Some("Kept"));
    assert_eq!(parsed.records[0].abstract_text, None);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::IgnoredLine { line: 2 }]
    );
}

/// A continuation line with no tag before it in its record has nothing to continue.
#[test]
fn continuation_without_tag_ignored() {
    let parsed = parse("TI  - First\n\n      orphan text\nTI  - Second\n").unwrap();
    let titles: Vec<_> = parsed.records.iter().map(|r| r.title.as_deref()).collect();
    assert_eq!(titles, vec![Some("First"), Some("Second")]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::IgnoredLine { line: 3 }]
    );
}

/// A continuation line after a tag that is not read is still part of the record, not ignored.
#[test]
fn continuation_of_unread_tag_not_reported() {
    let parsed = parse("TI  - t\nAD  - An affiliation that\n      wraps\n").unwrap();
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

// -- DOI extraction --

#[test]
fn doi_from_lid() {
    let input = "\
TI  - A test title
LID - S0140-6736(24)00624-X [pii]
LID - 10.1016/S0140-6736(24)00624-X [doi]
";

    let records = parse(input).unwrap().records;
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

    let records = parse(input).unwrap().records;
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
fn date_year_month() {
    let input = "\
TI  - A test title
DP  - 2024 Sep
";

    let records = parse(input).unwrap().records;
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

// -- Continuation lines --

#[test]
fn continuation_lines() {
    let input = "\
TI  - A test title
AB  - This is a long abstract that
      continues on the next line and
      keeps going further.
";

    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
    assert_eq!(
        records[0].authors,
        vec!["Very Long Author Name That Continues Here"]
    );
}

/// A blank title continued on the next line becomes the continuation text, without a leading
/// space.
#[test]
fn blank_title_continued_without_leading_space() {
    let records = parse("TI  - \n      A test title\n").unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A test title"));
}

/// A blank author continued on the next line becomes the continuation text, without a leading
/// space.
#[test]
fn blank_author_continued_without_leading_space() {
    let records = parse("TI  - A test title\nFAU - \n      Doe, Jane\n")
        .unwrap()
        .records;
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

/// A blank `LID` continued with a DOI on the next line gives the DOI without a leading space.
#[test]
fn blank_lid_continued_gives_doi() {
    let records = parse("TI  - A test title\nLID - \n      10.1000/xyz123 [doi]\n")
        .unwrap()
        .records;
    assert_eq!(records[0].doi.as_deref(), Some("10.1000/xyz123"));
}

#[test]
fn continuation_lines_lid() {
    let input = "\
TI  - A test title
LID - 10.1000/very-long-doi-that
      -keeps-going [doi]
";
    let records = parse(input).unwrap().records;
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

    let records = parse(input).unwrap().records;
    assert_eq!(first_extra(&records[0], "nbib:PMID"), Some("12345678"));
    assert_eq!(first_extra(&records[0], "nbib:PMC"), Some("PMC9876543"));
}

// -- Error cases --

#[test]
fn empty_input() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("   \n\n  "), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff}"), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff} \n\t"), Err(Error::EmptyInput));
}

/// A leading byte-order mark does not hide the first record's first tag.
#[test]
fn leading_byte_order_mark_ignored() {
    let input = "PMID- 1\nTI  - First\n";
    let parsed = parse(&format!("\u{feff}{input}")).unwrap();
    assert_eq!(parsed, parse(input).unwrap());
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(first_extra(&parsed.records[0], "nbib:PMID"), Some("1"));
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A record without a title is kept, and the missing title is reported at the record's line.
#[test]
fn missing_title_kept_and_reported() {
    let input = "\
FAU - Doe, Jane
DP  - 2024
";
    let parsed = parse(input).unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title, None);
    assert_eq!(parsed.records[0].authors, vec!["Doe, Jane"]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 1 }]
    );
}

// -- Serialization --

/// Only `nbib:` extras are written, `PMID` first; keys of other formats are not written.
#[test]
fn serialize_writes_only_nbib_extras() {
    let record = Record::builder()
        .title("T")
        .extra("nbib:PMC", "PMC1")
        .extra("nbib:PMID", "1")
        .extra("ris:KW", "cats")
        .build();
    let output = serialize(&[record]);
    assert!(output.starts_with("PMID- 1\nPMC - PMC1\n"));
    assert!(!output.contains("cats"));
}

/// Extras that are not valid tags, or that name a tag the parser reads, are not written.
#[test]
fn serialize_skips_unwritable_extras() {
    let record = Record::builder()
        .title("T")
        .extra("nbib:TOOLONG", "x")
        .extra("nbib:AU", "would become an author")
        .build();
    assert_eq!(serialize(&[record]), "TI  - T\n\n");
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

/// A record with no fields has nothing to write in `NBIB`, so it serialises to an empty record
/// separator and does not come back when parsed. Regression test found by
/// `garbage_between_files_keeps_records`.
#[test]
fn empty_record_serialises_to_nothing() {
    assert_eq!(serialize(&[Record::default()]), "\n");
}

#[test]
fn serialize_skips_none_fields() {
    let record = Record::builder().title("Minimal").build();

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

/// The tags the `NBIB` parser reads into fields. A copy of the parser's list, so that the tests pin
/// it.
const NBIB_READ_TAGS: [&str; 14] = [
    "TI", "BTI", "CTI", "FAU", "AU", "DP", "JT", "LID", "AID", "PG", "VI", "IP", "AB", "IS",
];

/// Extras as `NBIB` parsing produces them: up to four tags the parser does not read (such as
/// `PMID`), each with one to three values in order.
fn arb_nbib_extras() -> impl Strategy<Value = HashMap<String, Vec<String>>> {
    let unread_tag = "[A-Z]{2,4}".prop_filter("tag must not be read", |tag| {
        !NBIB_READ_TAGS.contains(&tag.as_str())
    });
    let pmid = proptest::option::of("[0-9]{5,8}");
    (
        pmid,
        proptest::collection::hash_map(unread_tag, vec(arb_text(40), 1..=3), 0..=3),
    )
        .prop_map(|(pmid, unread)| {
            let mut extras: HashMap<String, Vec<String>> = unread
                .into_iter()
                .map(|(tag, values)| (format!("nbib:{tag}"), values))
                .collect();
            if let Some(pmid) = pmid {
                extras.insert("nbib:PMID".to_owned(), vec![pmid]);
            }
            extras
        })
}

fn arb_nbib_record() -> impl Strategy<Value = Record> {
    (
        proptest::option::of(arb_text(80)),
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
            arb_nbib_extras(),
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
                (isbn, extras),
            )| {
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
        // A record with no fields has no lines to write, so it cannot survive a round trip (see
        // `empty_record_serialises_to_nothing`).
        .prop_filter("an NBIB record needs at least one field", |record| {
            *record != Record::default()
        })
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
    let reparsed = parse(&format!("TI  - t\nDP  - {formatted}\n"))
        .unwrap()
        .records[0]
        .date
        .unwrap();
    prop_assert_eq!(date, reparsed);
}

/// Serialising a record and parsing the output gives back the same record.
#[proptest]
fn serialize_round_trip(#[strategy(arb_nbib_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
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
    let record = Record::builder().title(title).build();
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
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
    let record = parse(&input).unwrap().records.remove(0);

    let field = match tag {
        "TI" | "BTI" => record.title,
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
    let record = parse(&text).unwrap().records.remove(0);
    let parsed = if is_title {
        record.title
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

// -- Lenient parsing properties --

/// Whether `line` would be read as an `NBIB` tag line: 2–4 uppercase letters or digits, optional
/// spaces, then `- `.
fn looks_like_tag(line: &str) -> bool {
    let tag_len = line
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .count();
    (2..=4).contains(&tag_len) && line[tag_len..].trim_start_matches(' ').starts_with("- ")
}

/// A line that belongs to no record: printable text that is not a tag line, either at the start of
/// the line or indented like a continuation.
fn arb_garbage_line() -> impl Strategy<Value = String> {
    prop_oneof![
        "[^\\s\\p{C}][^\\p{C}]{0,40}",
        "[^\\s\\p{C}][^\\p{C}]{0,40}".prop_map(|text| format!("      {text}")),
    ]
    .prop_filter("must not be a tag line", |line| !looks_like_tag(line))
}

/// A group of garbage lines between two valid files keeps every record, and each garbage line is
/// reported at its line.
#[proptest]
fn garbage_between_files_keeps_records(
    #[strategy(vec(arb_nbib_record(), 0..=3))] before: Vec<Record>,
    #[strategy(vec(arb_garbage_line(), 1..=4))] garbage: Vec<String>,
    #[strategy(vec(arb_nbib_record(), 0..=3))] after: Vec<Record>,
) {
    let head = serialize(&before);
    let input = format!("{head}{}\n\n{}", garbage.join("\n"), serialize(&after));
    let parsed = parse(&input).unwrap();

    let expected: Vec<Record> = before.into_iter().chain(after).collect();
    prop_assert_eq!(parsed.records, expected);

    let first = head.lines().count() + 1;
    let ignored: Vec<usize> = parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::IgnoredLine { .. }))
        .map(Diagnostic::line)
        .collect();
    let expected_lines: Vec<usize> = (first..first + garbage.len()).collect();
    prop_assert_eq!(ignored, expected_lines);
}

/// The tag lines used to build structured inputs.
const TAG_LINES: [&str; 4] = ["TI  - A title", "TI  - ", "FAU - Doe, Jane", "PMID- 1"];

/// Lines drawn from the pieces of an `NBIB` file, in any order, including blank and stray lines.
fn arb_nbib_line() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => select(TAG_LINES.to_vec()).prop_map(str::to_owned),
        2 => Just("      continued".to_owned()),
        2 => Just(String::new()),
        2 => arb_garbage_line(),
    ]
}

/// However the pieces are arranged, every blank-separated group with a tag line gives exactly one
/// record, diagnostics come in line order, and each record without a usable title is reported once.
#[proptest]
fn structure_invariants(#[strategy(vec(arb_nbib_line(), 1..40))] lines: Vec<String>) {
    let input = lines.join("\n");
    let Ok(parsed) = parse(&input) else {
        // Only blank input is rejected.
        prop_assert!(input.trim().is_empty());
        return Ok(());
    };

    let groups_with_tags = lines
        .split(|line| line.trim().is_empty())
        .filter(|group| group.iter().any(|line| TAG_LINES.contains(&line.as_str())))
        .count();
    prop_assert_eq!(parsed.records.len(), groups_with_tags);

    let diagnostic_lines: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    prop_assert!(diagnostic_lines.is_sorted());

    let untitled = parsed
        .records
        .iter()
        .filter(|r| r.title.as_deref().is_none_or(|t| t.trim().is_empty()))
        .count();
    let missing = parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::MissingTitle { .. }))
        .count();
    prop_assert_eq!(missing, untitled);
}

// -- Byte-order mark --

/// Any input, as serialised records or as arbitrary text. It never starts with a byte-order mark:
/// U+FEFF is a format character, which `\PC` excludes, and the serialiser never writes it.
fn arb_input_without_byte_order_mark() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::collection::vec(arb_nbib_record(), 0..4).prop_map(|records| serialize(&records)),
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

/// Keys of the other formats, which `NBIB` serialisation must never write.
const FOREIGN_KEYS: [&str; 3] = ["ris:KW", "bibtex:note", "endnote:keywords/keyword"];

/// A tag lands in `extras` exactly when the parser does not read it.
#[proptest]
fn extras_hold_exactly_the_unread_tags(#[strategy("[A-Z0-9]{2,4}")] tag: String) {
    let records = parse(&format!("TI  - t\n{tag:<4}- value\n"))
        .unwrap()
        .records;
    let kept = records[0].extras.contains_key(&format!("nbib:{tag}"));
    prop_assert_eq!(kept, !NBIB_READ_TAGS.contains(&tag.as_str()));
}

/// An extra naming a tag the parser reads is never written, so it cannot leak into a field.
#[proptest]
fn read_tags_never_written_from_extras(
    #[strategy(select(NBIB_READ_TAGS.to_vec()))] tag: &'static str,
    #[strategy(arb_nbib_record())] record: Record,
) {
    let mut with_read_tag = record.clone();
    with_read_tag
        .extras
        .insert(format!("nbib:{tag}"), vec!["leaked value".to_owned()]);
    prop_assert_eq!(serialize(&[with_read_tag]), serialize(&[record]));
}

/// `NBIB` has no tag for the source database, so it is kept on the record but never written.
#[proptest]
fn database_not_written(
    #[strategy(arb_nbib_record())] record: Record,
    #[strategy(arb_text(30))] database: String,
) {
    let mut with_database = record.clone();
    with_database.database = Some(database);
    prop_assert_eq!(serialize(&[with_database]), serialize(&[record]));
}

/// Extras of other formats are kept on the record but never written as `NBIB` tags.
#[proptest]
fn foreign_extras_not_written(
    #[strategy(arb_nbib_record())] record: Record,
    #[strategy(arb_text(30))] value: String,
) {
    let mut with_foreign = record.clone();
    for key in FOREIGN_KEYS {
        with_foreign
            .extras
            .insert(key.to_owned(), vec![value.clone()]);
    }
    prop_assert_eq!(serialize(&[with_foreign]), serialize(&[record]));
}

/// Records read from source text with arbitrary unread tags and values come back unchanged when
/// written and read again.
#[proptest]
fn records_stable_through_serialize(
    #[strategy(proptest::collection::vec(("[A-Z]{2,4}", arb_raw_value()), 0..6))] lines: Vec<(
        String,
        String,
    )>,
) {
    let mut input = String::from("TI  - A title\n");
    for (tag, value) in &lines {
        if !NBIB_READ_TAGS.contains(&tag.as_str()) {
            writeln!(input, "{tag:<4}- {value}").unwrap();
        }
    }
    let first = parse(&input).unwrap().records;
    let second = parse(&serialize(&first)).unwrap().records;
    prop_assert_eq!(second, first);
}
