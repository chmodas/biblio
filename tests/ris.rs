mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::{select, subsequence};
use std::collections::HashMap;
use std::fmt::Write as _;
use test_strategy::{Arbitrary, proptest};

use biblio::ris::{parse, serialize};
use biblio::{Diagnostic, Error, PublicationDate, Record};

use common::{
    IdentifierValues, arb_author, arb_blank, arb_date, arb_identifier_values, arb_keyword,
    arb_pages, arb_raw_value, arb_text, arb_url, arb_volume, first_extra,
};

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
    assert_eq!(first_extra(r, "ris:ID"), Some("ref001"));
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

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 2);

    assert_eq!(
        records[0].title.as_deref(),
        Some("CRISPR-Cas9 gene editing for sickle cell disease")
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
    assert_eq!(records[1].isbn.as_deref(), Some("1546-170X"));
}

#[test]
fn windows_line_endings() {
    let input = "TY  - JOUR\r\nTI  - A test title\r\nAU  - Doe, Jane\r\nER  -\r\n";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A test title"));
    assert_eq!(records[0].authors, vec!["Doe, Jane"]);
}

/// Tags without a field are kept in `extras` under `ris:`, with repeated tags keeping every value
/// in order, and the record type is kept as `ris:TY`.
#[test]
fn unknown_tags_kept_in_extras() {
    let input = "\
TY  - BOOK
TI  - A test title
N1  - note one
N1  - note two
AD  - an address
ER  -
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A test title"));
    let extras = &records[0].extras;
    assert_eq!(extras["ris:TY"], vec!["BOOK"]);
    assert_eq!(extras["ris:N1"], vec!["note one", "note two"]);
    assert_eq!(extras["ris:AD"], vec!["an address"]);
    assert_eq!(extras.len(), 3);
}

/// A continuation line of a tag without a field is joined to that tag's last value.
#[test]
fn unknown_tag_continuation_joined() {
    let records = parse("TY  - JOUR\nN1  - first\nN1  - a note that\nwraps\nER  -\n")
        .unwrap()
        .records;
    assert_eq!(
        records[0].extras["ris:N1"],
        vec!["first", "a note that wraps"]
    );
}

#[test]
fn record_without_er() {
    let input = "\
TY  - JOUR
TI  - Unclosed record
AU  - Doe, Jane
";
    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].title.as_deref(), Some("Unclosed record"));
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
TY  - JOUR
TI  - A test title
PY  - 2024/09//
ER  -
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
TY  - JOUR
TI  - A test title
PY  - 2024/09/01/
ER  -
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
fn date_y1_fallback() {
    let input = "\
TY  - JOUR
TI  - A test title
Y1  - 2023/06//
ER  -
";
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    parse(&input).unwrap().records.remove(0).journal
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
    let records = parse("TY  - JOUR\nTI  -\nA test title\nER  -\n")
        .unwrap()
        .records;
    assert_eq!(records[0].title.as_deref(), Some("A test title"));
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
    let records = parse("TY  - JOUR\nTI  - A test title\nAU  -\nDoe, Jane\nER  -\n")
        .unwrap()
        .records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("Preferred title"));
}

#[test]
fn t1_fallback() {
    let input = "\
TY  - JOUR
T1  - Fallback title
ER  -
";
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("Fallback title"));
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
    let records = parse(input).unwrap().records;
    assert_eq!(first_extra(&records[0], "ris:ID"), Some("ref001"));
}

// -- Source database --

/// `DB` is read as the database and not kept in extras; the database provider `DP` stays an extra.
#[test]
fn db_read_as_database() {
    let input = "\
TY  - JOUR
TI  - A test title
DB  - Scopus
DP  - Elsevier
ER  -
";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.database.as_deref(), Some("Scopus"));
    assert!(!record.extras.contains_key("ris:DB"));
    assert_eq!(first_extra(&record, "ris:DP"), Some("Elsevier"));
}

/// A database name wrapped onto the next line is joined to it.
#[test]
fn db_continuation_joined() {
    let input = "\
TY  - JOUR
TI  - t
DB  - Cochrane Central Register
  of Controlled Trials
ER  -
";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(
        record.database.as_deref(),
        Some("Cochrane Central Register of Controlled Trials")
    );
}

/// A repeated `DB` keeps its last value.
#[test]
fn repeated_db_keeps_last() {
    let input = "TY  - JOUR\nTI  - t\nDB  - Embase\nDB  - MEDLINE\nER  -\n";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.database.as_deref(), Some("MEDLINE"));
}

/// A blank `DB` is kept as an empty database.
#[test]
fn blank_db_kept_empty() {
    let record = parse("TY  - JOUR\nTI  - t\nDB  -\nER  -\n")
        .unwrap()
        .records
        .remove(0);
    assert_eq!(record.database.as_deref(), Some(""));
    let reparsed = parse(&serialize(std::slice::from_ref(&record)))
        .unwrap()
        .records;
    assert_eq!(reparsed, vec![record]);
}

/// The database is written as `DB`, after `SN`.
#[test]
fn serialize_writes_db() {
    let record = Record::builder()
        .title("T")
        .isbn("1234-5678")
        .database("Web of Science")
        .build();
    assert_eq!(
        serialize(&[record]),
        "TY  - JOUR\nTI  - T\nSN  - 1234-5678\nDB  - Web of Science\nER  -\n"
    );
}

// -- Label --

/// `LB` is read as the label and not kept in extras.
#[test]
fn lb_read_as_label() {
    let record = parse("TY  - JOUR\nTI  - t\nLB  - OLD\nER  -\n")
        .unwrap()
        .records
        .remove(0);
    assert_eq!(record.label.as_deref(), Some("OLD"));
    assert!(!record.extras.contains_key("ris:LB"));
}

/// A label wrapped onto the next line is joined to it.
#[test]
fn lb_continuation_joined() {
    let input = "\
TY  - JOUR
TI  - t
LB  - Included after
  full-text screening
ER  -
";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(
        record.label.as_deref(),
        Some("Included after full-text screening")
    );
}

/// A repeated `LB` keeps its last value.
#[test]
fn repeated_lb_keeps_last() {
    let input = "TY  - JOUR\nTI  - t\nLB  - OLD\nLB  - NEW\nER  -\n";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.label.as_deref(), Some("NEW"));
}

/// A blank `LB` is kept as an empty label, and survives a round trip.
#[test]
fn blank_lb_kept_empty() {
    let record = parse("TY  - JOUR\nTI  - t\nLB  -\nER  -\n")
        .unwrap()
        .records
        .remove(0);
    assert_eq!(record.label.as_deref(), Some(""));
    let reparsed = parse(&serialize(std::slice::from_ref(&record)))
        .unwrap()
        .records;
    assert_eq!(reparsed, vec![record]);
}

/// The label is written as `LB`, after `DB`.
#[test]
fn serialize_writes_lb() {
    let record = Record::builder()
        .title("T")
        .isbn("1234-5678")
        .database("Web of Science")
        .label("OLD")
        .build();
    assert_eq!(
        serialize(&[record]),
        "TY  - JOUR\nTI  - T\nSN  - 1234-5678\nDB  - Web of Science\nLB  - OLD\nER  -\n"
    );
}

// -- Keywords and links --

/// Each `KW` adds a keyword and each `UR` a link, in order; `KW` and `UR` are not kept in extras.
#[test]
fn kw_and_ur_read_in_order() {
    let input = "\
TY  - JOUR
TI  - t
KW  - cats
UR  - https://example.org/a
KW  - dogs
UR  - https://example.org/b
ER  -
";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.keywords, vec!["cats", "dogs"]);
    assert_eq!(
        record.urls,
        vec!["https://example.org/a", "https://example.org/b"]
    );
    assert!(!record.extras.contains_key("ris:KW"));
    assert!(!record.extras.contains_key("ris:UR"));
}

/// A continuation line of `KW` is joined to the keyword, as for every other tag, even when an
/// export lists one keyword per line under a single `KW`.
#[test]
fn kw_continuation_joined() {
    let input = "\
TY  - JOUR
TI  - t
KW  - Access
Cancer
ER  -
";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.keywords, vec!["Access Cancer"]);
}

/// A blank `KW` continued on the next line gives that line as the keyword, not an addition to the
/// keyword before it.
#[test]
fn blank_kw_continued_gives_that_keyword() {
    let input = "TY  - JOUR\nTI  - t\nKW  - cats\nKW  -\n  dogs\nER  -\n";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.keywords, vec!["cats", "dogs"]);
}

/// Blank `KW` and `UR` tags add nothing.
#[test]
fn blank_kw_and_ur_dropped() {
    let input = "TY  - JOUR\nTI  - t\nKW  -\nUR  -   \nKW  - cats\nER  -\n";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.keywords, vec!["cats"]);
    assert_eq!(record.urls, Vec::<String>::new());
}

/// Links other than `UR`, such as the file link `L1`, stay in extras.
#[test]
fn l1_kept_in_extras() {
    let input = "TY  - JOUR\nTI  - t\nL1  - internal-pdf://paper.pdf\nER  -\n";
    let record = parse(input).unwrap().records.remove(0);
    assert_eq!(record.urls, Vec::<String>::new());
    assert_eq!(
        first_extra(&record, "ris:L1"),
        Some("internal-pdf://paper.pdf")
    );
}

/// Keywords are written as `KW` and links as `UR`, after `LB`, skipping blank values.
#[test]
fn serialize_writes_kw_and_ur_after_lb() {
    let record = Record::builder()
        .title("T")
        .label("OLD")
        .keywords(["cats", " ", "dogs"])
        .urls(["", "https://example.org"])
        .build();
    assert_eq!(
        serialize(&[record]),
        "TY  - JOUR\nTI  - T\nLB  - OLD\nKW  - cats\nKW  - dogs\nUR  - https://example.org\nER  -\n"
    );
}

// -- Error cases --

#[test]
fn empty_input() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("   \n\n  "), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff}"), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff} \n\t"), Err(Error::EmptyInput));
}

/// A leading byte-order mark does not hide the first record's `TY` tag.
#[test]
fn leading_byte_order_mark_ignored() {
    let input = "TY  - JOUR\nTI  - First\nER  -\n";
    let parsed = parse(&format!("\u{feff}{input}")).unwrap();
    assert_eq!(parsed, parse(input).unwrap());
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title.as_deref(), Some("First"));
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A record without a title is kept, and the missing title is reported at the record's line.
#[test]
fn missing_title_kept_and_reported() {
    let input = "\
TY  - JOUR
AU  - Doe, Jane
ER  -
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

// -- Lenient parsing --

/// A `TY` before the open record's `ER` keeps both records and reports the unterminated one.
#[test]
fn ty_before_er_keeps_both_records() {
    let parsed = parse("TY  - JOUR\nTI  - First\nTY  - JOUR\nTI  - Second\nER  -\n").unwrap();
    let titles: Vec<_> = parsed.records.iter().map(|r| r.title.as_deref()).collect();
    assert_eq!(titles, vec![Some("First"), Some("Second")]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::UnterminatedRecord { line: 1 }]
    );
}

/// A record still open at the end of the input is kept and reported.
#[test]
fn unterminated_record_at_end_reported() {
    let parsed = parse("TY  - JOUR\nTI  - Open\n").unwrap();
    assert_eq!(parsed.records[0].title.as_deref(), Some("Open"));
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::UnterminatedRecord { line: 1 }]
    );
}

/// Tags before the first `TY` and a stray `ER` are ignored and reported at their lines.
#[test]
fn lines_outside_records_ignored() {
    let parsed = parse("TI  - Stray\nTY  - JOUR\nTI  - Kept\nER  -\n\nER  -\nnot a tag\n").unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(
        parsed.diagnostics,
        vec![
            Diagnostic::IgnoredLine { line: 1 },
            Diagnostic::IgnoredLine { line: 6 },
            Diagnostic::IgnoredLine { line: 7 },
        ]
    );
}

/// A non-tag line directly after `TY` has no field to continue, so it is ignored.
#[test]
fn line_after_ty_ignored() {
    let parsed = parse("TY  - JOUR\nloose text\nTI  - Kept\nER  -\n").unwrap();
    assert_eq!(parsed.records[0].title.as_deref(), Some("Kept"));
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::IgnoredLine { line: 2 }]
    );
}

/// A continuation line after a tag that is not read is still part of the record, not ignored.
#[test]
fn continuation_of_unread_tag_not_reported() {
    let parsed = parse("TY  - JOUR\nTI  - t\nN1  - a note\nthat wraps\nER  -\n").unwrap();
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

// -- Serialization --

/// Only `ris:` extras are written; keys of other formats are kept on the record but not written.
#[test]
fn serialize_writes_only_ris_extras() {
    let record = Record::builder()
        .title("T")
        .extra("ris:N1", "cats")
        .extra("bibtex:note", "a note")
        .extra("nbib:OT", "other term")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("N1  - cats\n"));
    assert!(!output.contains("a note"));
    assert!(!output.contains("other term"));
}

/// The record type comes from `ris:TY`, falling back to `JOUR`.
#[test]
fn serialize_record_type() {
    let book = Record::builder().extra("ris:TY", "BOOK").build();
    assert!(serialize(&[book]).starts_with("TY  - BOOK\n"));
    assert!(serialize(&[Record::default()]).starts_with("TY  - JOUR\n"));
}

/// Extras that are not valid tags, or that name a tag the parser reads, are not written: they would
/// break the output or be read back into a field.
#[test]
fn serialize_skips_unwritable_extras() {
    let record = Record::builder()
        .title("T")
        .extra("ris:foo", "not a tag")
        .extra("ris:T2", "would become the journal")
        .extra("ris:ER", "would end the record")
        .build();
    let output = serialize(&[record]);
    assert_eq!(output, "TY  - JOUR\nTI  - T\nER  -\n");
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
    assert!(output.contains("TI  - Minimal"));
    assert!(!output.contains("AU  -"));
    assert!(!output.contains("PY  -"));
    assert!(!output.contains("JO  -"));
}

#[test]
fn serialize_pages_split() {
    let record = Record::builder().title("Test").pages("123-456").build();
    let output = serialize(&[record]);
    assert!(output.contains("SP  - 123"));
    assert!(output.contains("EP  - 456"));
}

#[test]
fn serialize_en_dash_pages() {
    let record = Record::builder()
        .title("Test")
        .pages("123\u{2013}456")
        .build();
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

/// The tags the `RIS` parser reads into fields or record structure. A copy of the parser's list, so
/// that the tests pin it.
const RIS_READ_TAGS: [&str; 26] = [
    "TY", "ER", "TI", "T1", "AU", "A1", "PY", "Y1", "DO", "SP", "EP", "VL", "IS", "AB", "N2", "SN",
    "DB", "LB", "JF", "T2", "JO", "JA", "J2", "J1", "KW", "UR",
];

/// Extras as `RIS` parsing produces them: a record type, and up to four tags the parser does not
/// read, each with one to three values in order.
fn arb_ris_extras() -> impl Strategy<Value = HashMap<String, Vec<String>>> {
    let unread_tag = "[A-Z][A-Z0-9]".prop_filter("tag must not be read", |tag| {
        !RIS_READ_TAGS.contains(&tag.as_str())
    });
    (
        select(vec!["JOUR", "BOOK", "CHAP"]),
        proptest::collection::hash_map(unread_tag, vec(arb_text(40), 1..=3), 0..=4),
    )
        .prop_map(|(record_type, unread)| {
            let mut extras: HashMap<String, Vec<String>> = unread
                .into_iter()
                .map(|(tag, values)| (format!("ris:{tag}"), values))
                .collect();
            extras.insert("ris:TY".to_owned(), vec![record_type.to_owned()]);
            extras
        })
}

fn arb_ris_record() -> impl Strategy<Value = Record> {
    (
        proptest::option::of(arb_text(80)),
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
            proptest::option::of(arb_text(60)),
            proptest::option::of(arb_text(60)),
            vec(arb_keyword(), 0..4),
            vec(arb_url(), 0..3),
            arb_ris_extras(),
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
                (isbn, database, label, keywords, urls, extras),
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

/// A date written in the `RIS` date format parses back to the same date.
#[proptest]
fn date_round_trip(#[strategy(arb_date())] date: PublicationDate) {
    let formatted = format_ris_date(date);
    let reparsed = parse(&format!("TY  - JOUR\nTI  - t\nPY  - {formatted}\nER  -\n"))
        .unwrap()
        .records[0]
        .date
        .unwrap();
    prop_assert_eq!(date, reparsed);
}

/// Serialising a record and parsing the output gives back the same record.
#[proptest]
fn serialize_round_trip(#[strategy(arb_ris_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
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
    // Parsing records the type, so the expected record carries the default one.
    let record = Record::builder()
        .title(title)
        .extra("ris:TY", "JOUR")
        .build();
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
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
    #[strategy(select(&[
        "TI", "T1", "AB", "N2", "AU", "A1", "DB", "LB", "KW", "UR", "JF", "T2", "JO", "JA", "J2",
        "J1",
    ]))]
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
    let record = parse(&input).unwrap().records.remove(0);

    let field = match tag {
        "TI" | "T1" => record.title,
        "AB" | "N2" => record.abstract_text,
        "AU" | "A1" => record.authors.last().cloned(),
        "DB" => record.database,
        "LB" => record.label,
        "KW" => record.keywords.last().cloned(),
        "UR" => record.urls.last().cloned(),
        _ => record.journal,
    };
    prop_assert_eq!(field.as_deref(), Some(text.trim()));
}

// -- Lenient parsing properties --

/// A line that is not blank and not a tag line: any printable text not starting with whitespace,
/// minus the rare values that happen to look like `XX  -`.
fn arb_garbage_line() -> impl Strategy<Value = String> {
    "[^\\s\\p{C}][^\\p{C}]{0,40}".prop_filter("must not be a tag line", |line| {
        let bytes = line.as_bytes();
        !(bytes.len() >= 5
            && bytes[..2]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            && &bytes[2..5] == b"  -")
    })
}

/// Garbage lines between two valid files keep every record and are each reported at their line.
#[proptest]
fn garbage_between_files_keeps_records(
    #[strategy(vec(arb_ris_record(), 0..=3))] before: Vec<Record>,
    #[strategy(vec(arb_garbage_line(), 1..=4))] garbage: Vec<String>,
    #[strategy(vec(arb_ris_record(), 0..=3))] after: Vec<Record>,
) {
    let head = serialize(&before);
    let input = format!("{head}{}\n{}", garbage.join("\n"), serialize(&after));
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

/// Lines drawn from the pieces of a `RIS` file, in any order, including blank and stray lines.
fn arb_ris_line() -> impl Strategy<Value = String> {
    prop_oneof![
        2 => Just("TY  - JOUR".to_owned()),
        2 => Just("ER  -".to_owned()),
        2 => Just("TI  - A title".to_owned()),
        1 => Just("TI  -".to_owned()),
        1 => Just("T1  - Another title".to_owned()),
        2 => Just("AU  - Doe, Jane".to_owned()),
        1 => Just(String::new()),
        2 => arb_garbage_line(),
    ]
}

/// However the pieces are arranged, every `TY` gives exactly one record, diagnostics come in line
/// order, and each record without a usable title is reported once.
#[proptest]
fn structure_invariants(#[strategy(vec(arb_ris_line(), 1..40))] lines: Vec<String>) {
    let input = lines.join("\n");
    let Ok(parsed) = parse(&input) else {
        // Only blank input is rejected.
        prop_assert!(input.trim().is_empty());
        return Ok(());
    };

    let ty_lines = lines.iter().filter(|l| l.as_str() == "TY  - JOUR").count();
    prop_assert_eq!(parsed.records.len(), ty_lines);

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
        proptest::collection::vec(arb_ris_record(), 0..4).prop_map(|records| serialize(&records)),
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

/// Keys of the other formats, which `RIS` serialisation must never write.
const FOREIGN_KEYS: [&str; 3] = ["nbib:OT", "bibtex:note", "endnote:keywords/keyword"];

/// A tag lands in `extras` exactly when the parser does not read it.
#[proptest]
fn extras_hold_exactly_the_unread_tags(
    #[strategy("[A-Z0-9]{2}")]
    #[filter(!matches!(#tag.as_str(), "TY" | "ER"))]
    tag: String,
) {
    let records = parse(&format!("TY  - JOUR\n{tag}  - value\nER  -\n"))
        .unwrap()
        .records;
    let kept = records[0].extras.contains_key(&format!("ris:{tag}"));
    prop_assert_eq!(kept, !RIS_READ_TAGS.contains(&tag.as_str()));
}

/// An extra naming a tag the parser reads is never written, so it cannot leak into a field. `TY`
/// and `SN` are left out: they are written from `ris:TY` and `ris:SN` by design.
#[proptest]
fn read_tags_never_written_from_extras(
    #[strategy(select(RIS_READ_TAGS[1..].iter().copied().filter(|tag| *tag != "SN").collect::<Vec<_>>()))]
    tag: &'static str,
    #[strategy(arb_ris_record())] record: Record,
) {
    let mut with_read_tag = record.clone();
    with_read_tag
        .extras
        .insert(format!("ris:{tag}"), vec!["leaked value".to_owned()]);
    prop_assert_eq!(serialize(&[with_read_tag]), serialize(&[record]));
}

/// Extras of other formats are kept on the record but never written as `RIS` tags.
#[proptest]
fn foreign_extras_not_written(
    #[strategy(arb_ris_record())] record: Record,
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

/// `KW` and `UR` lines with raw values, in any mix, give the trimmed non-blank values of each tag
/// in order, and the record comes back unchanged when written and read again.
#[proptest]
fn keywords_and_urls_match_reference(
    #[strategy(vec((select(&["KW", "UR"]), arb_raw_value()), 0..8))] lines: Vec<(
        &'static str,
        String,
    )>,
) {
    let mut input = String::from("TY  - JOUR\nTI  - A title\n");
    for (tag, value) in &lines {
        writeln!(input, "{tag}  - {value}").unwrap();
    }
    input.push_str("ER  -\n");
    let first = parse(&input).unwrap().records;

    let reference = |wanted: &str| -> Vec<String> {
        lines
            .iter()
            .filter(|(tag, _)| *tag == wanted)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    };
    prop_assert_eq!(&first[0].keywords, &reference("KW"));
    prop_assert_eq!(&first[0].urls, &reference("UR"));
    prop_assert_eq!(parse(&serialize(&first)).unwrap().records, first);
}

/// Blank keywords and links are not written, wherever they are in the list.
#[proptest]
fn blank_list_values_not_written(
    #[strategy(arb_ris_record())] record: Record,
    #[strategy(arb_blank())] blank: String,
) {
    let mut with_blanks = record.clone();
    for list in [&mut with_blanks.keywords, &mut with_blanks.urls] {
        list.insert(0, blank.clone());
        list.push(blank.clone());
    }
    prop_assert_eq!(serialize(&[with_blanks]), serialize(&[record]));
}

/// Records read from source text with arbitrary unread tags and values come back unchanged when
/// written and read again.
#[proptest]
fn records_stable_through_serialize(
    #[strategy(proptest::collection::vec(("[A-Z][A-Z0-9]", arb_raw_value()), 0..6))] lines: Vec<(
        String,
        String,
    )>,
) {
    let mut input = String::from("TY  - JOUR\nTI  - A title\n");
    for (tag, value) in &lines {
        if !RIS_READ_TAGS.contains(&tag.as_str()) {
            writeln!(input, "{tag}  - {value}").unwrap();
        }
    }
    input.push_str("ER  -\n");
    let first = parse(&input).unwrap().records;
    let second = parse(&serialize(&first)).unwrap().records;
    prop_assert_eq!(second, first);
}

// -- Identifiers --

/// The record read from `tag_lines` between `TY` and `ER`.
fn record_with(tag_lines: &str) -> Record {
    parse(&format!("TY  - JOUR\n{tag_lines}ER  -\n"))
        .unwrap()
        .records
        .remove(0)
}

/// Several `SN` lines give the lowest identifier, without its label, and the others are kept in
/// `ris:SN` as written, in order.
#[test]
fn several_sn_lines_give_the_lowest() {
    let record =
        record_with("SN  - 1939-1676 (Electronic)\nSN  - 0891-6640 (Print)\nSN  - 1234-5678\n");
    assert_eq!(record.isbn.as_deref(), Some("0891-6640"));
    assert_eq!(
        record.extras["ris:SN"],
        ["1939-1676 (Electronic)", "1234-5678"]
    );
}

/// Several identifiers in one `SN`, separated by `;`, are split.
#[test]
fn identifiers_in_one_sn_are_split() {
    let record = record_with("SN  - 978-131750096-4 (ISBN); 978-113888701-5 (ISBN)\n");
    assert_eq!(record.isbn.as_deref(), Some("978-113888701-5"));
    assert_eq!(record.extras["ris:SN"], ["978-131750096-4 (ISBN)"]);
}

/// A label glued to the next identifier, and a literal backslash and `r` standing for a lost line
/// break, are split from the identifiers around them.
#[test]
fn glued_identifiers_are_split() {
    let glued = record_with("SN  - 0964-6906 (Print)0964-6906\n");
    assert_eq!(glued.isbn.as_deref(), Some("0964-6906"));
    assert_eq!(glued.extras["ris:SN"], ["0964-6906 (Print)"]);
    let escaped = record_with("SN  - 1757-4676 (Print)\\r1757-4676\n");
    assert_eq!(escaped.isbn.as_deref(), Some("1757-4676"));
    assert_eq!(escaped.extras["ris:SN"], ["\\r1757-4676"]);
}

/// A continuation line of `SN` is joined to it, so its identifier is read too. Regression test:
/// it used to be dropped.
#[test]
fn sn_continuation_read() {
    let record = record_with("SN  - 1939-1676\n  0891-6640\n");
    assert_eq!(record.isbn.as_deref(), Some("0891-6640"));
    assert_eq!(record.extras["ris:SN"], ["1939-1676"]);
}

/// The chosen identifier is written first, then one `SN` per other identifier, and reading the
/// output gives the same record.
#[test]
fn identifiers_written_back() {
    let record = record_with("SN  - 1939-1676 (Electronic)\nSN  - 0891-6640 (Print)\n");
    let output = serialize(std::slice::from_ref(&record));
    assert!(output.contains("SN  - 0891-6640\nSN  - 1939-1676 (Electronic)\n"));
    assert_eq!(parse(&output).unwrap().records, [record]);
}

/// Identifiers listed in any number of `SN` lines give the lowest, and writing and reading the
/// record again gives the same record.
#[proptest]
fn identifiers_round_trip(#[strategy(arb_identifier_values())] listed: IdentifierValues) {
    let mut tag_lines = String::new();
    for value in &listed.values {
        writeln!(tag_lines, "SN  - {value}").unwrap();
    }
    let record = record_with(&tag_lines);
    prop_assert_eq!(record.isbn.as_deref(), Some(listed.lowest()));
    let output = serialize(std::slice::from_ref(&record));
    prop_assert_eq!(parse(&output).unwrap().records, vec![record]);
}
