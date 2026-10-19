mod common;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::subsequence;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use test_strategy::{Arbitrary, proptest};

use biblio::bibtex::{parse, serialize};
use biblio::{Diagnostic, Error, PublicationDate, Record};

use common::{arb_date, first_extra};

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
    assert_eq!(first_extra(r, "bibtex:@key"), Some("doe2024"));
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

    let records = parse(input).unwrap().records;
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].title.as_deref(),
        Some("CRISPR-Cas9 gene editing")
    );
    assert_eq!(
        first_extra(&records[0], "bibtex:@key"),
        Some("frangoul2024")
    );
    assert_eq!(
        records[1].title.as_deref(),
        Some("Large language models in clinical medicine")
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
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].title.as_deref(), Some("A website resource"));
    assert_eq!(records[0].authors, Vec::<String>::new());
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
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].authors, Vec::<String>::new());
}

#[test]
fn author_with_prefix() {
    let input = r"
@article{von2024,
  title = {Test},
  author = {von Neumann, John},
}
";
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
fn date_full_iso() {
    let input = r"
@article{test,
  title = {Test},
  date = {2024-09-01},
}
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
fn date_between() {
    let input = r"
@article{test,
  title = {Test},
  date = {2020/2024},
}
";
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2020,
            month: None,
            day: None,
        })
    );
}

/// A `date` that is not a date, either blank or not starting with a year, gives no date.
#[test]
fn date_without_year_ignored() {
    for date in ["", " ", "in press"] {
        let input = format!("@article{{k, title = {{T}}, date = {{{date}}}}}\n");
        let records = parse(&input).unwrap().records;
        assert_eq!(records[0].date, None, "date = {{{date}}}");
    }
}

/// A `date` that cannot be read falls back to the `year`, including one that starts with digits
/// but is no date and does not start with a year on its own; neither field is kept in the extras.
#[test]
fn unreadable_date_falls_back_to_year() {
    let dates = [
        "",
        " ",
        "in press",
        "2020-02-30",
        "2020-13-45",
        "2020-05 approx",
        "2019a",
        "[2019]",
        "99999999999",
    ];
    for date in dates {
        let input = format!("@article{{k, title = {{T}}, date = {{{date}}}, year = {{2020}}}}\n");
        let records = parse(&input).unwrap().records;
        assert_eq!(
            records[0].date,
            Some(PublicationDate {
                year: 2020,
                month: None,
                day: None,
            }),
            "date = {{{date}}}"
        );
        assert_eq!(first_extra(&records[0], "bibtex:year"), None);
        assert_eq!(first_extra(&records[0], "bibtex:date"), None);
    }
}

/// When neither `date` nor `year` can be read, there is no date.
#[test]
fn unreadable_date_and_year_give_no_date() {
    let records = parse("@article{k, title = {T}, date = {in press}, year = {n.d.}}\n")
        .unwrap()
        .records;
    assert_eq!(records[0].date, None);
}

/// A `date` that can be read wins over a different `year`, including free text that starts with
/// a negative year and ranges open at either end.
#[test]
fn readable_date_wins_over_year() {
    let records = parse(
        "@article{a, title = {T}, date = {2024-09-01}, year = {1999}}\n\
         @article{b, title = {T}, date = {2021 approximate}, year = {1999}}\n\
         @article{c, title = {T}, date = {-44 BC}, year = {1999}}\n\
         @article{d, title = {T}, date = {/2021}, year = {1999}}\n\
         @article{e, title = {T}, date = {2021/}, year = {1999}}\n",
    )
    .unwrap()
    .records;
    assert_eq!(
        records[0].date,
        Some(PublicationDate {
            year: 2024,
            month: Some(9),
            day: Some(1),
        })
    );
    assert_eq!(records[1].date.map(|date| date.year), Some(2021));
    assert_eq!(records[2].date.map(|date| date.year), Some(-44));
    assert_eq!(records[3].date.map(|date| date.year), Some(2021));
    assert_eq!(records[4].date.map(|date| date.year), Some(2021));
}

/// A `date` whose text starts with something other than a year gives way to the `year`, whatever
/// follows. The text never starts with a digit or a sign, which can begin a year.
#[proptest]
fn date_not_starting_with_a_year_gives_the_year(
    #[strategy("[ \t]{0,2}[a-zA-Zé?.:][a-zA-Z0-9é ?.,:/-]{0,24}")] date: String,
    #[strategy(1000_i32..3000)] year: i32,
) {
    let input = format!("@article{{k, title = {{T}}, date = {{{date}}}, year = {{{year}}}}}\n");
    let records = parse(&input).unwrap().records;
    prop_assert_eq!(records[0].date.map(|d| d.year), Some(year));
}

/// A `crossref` child of a type that inherits its parent's date takes the parent's date over its
/// own `year`, which the underlying parser drops while resolving, even when the parent's `date`
/// cannot be read. A child of a type that inherits no date keeps its own `year`.
#[test]
fn crossref_child_takes_the_parents_date() {
    let year_of_child =
        |input: &str| -> Option<i32> { parse(input).unwrap().records[1].date.map(|d| d.year) };
    assert_eq!(
        year_of_child(
            "@proceedings{p, title = {P}, date = {2019}}\n\
             @inproceedings{c, title = {C}, year = {2020}, crossref = {p}}\n"
        ),
        Some(2019)
    );
    assert_eq!(
        year_of_child(
            "@proceedings{p, title = {P}, date = {in press}}\n\
             @inproceedings{c, title = {C}, year = {2020}, crossref = {p}}\n"
        ),
        None
    );
    assert_eq!(
        year_of_child(
            "@book{p, title = {P}, date = {2019}}\n\
             @misc{c, title = {C}, year = {2020}, crossref = {p}}\n"
        ),
        Some(2020)
    );
}

/// A `year` that is not a number gives no date.
#[test]
fn year_not_a_number_ignored() {
    let records = parse("@article{k, title = {T}, year = {in press}}\n")
        .unwrap()
        .records;
    assert_eq!(records[0].date, None);
}

/// The date `date` gives in an entry that also has a different `year`.
fn date_of(date: &str) -> Option<PublicationDate> {
    let input = format!("@article{{k, title = {{T}}, date = {{{date}}}, year = {{1999}}}}\n");
    parse(&input).unwrap().records[0].date
}

/// A year written with a sign and two digits is read as written, with the month and day given,
/// at either end of a range; four digits and one digit already were.
#[test]
fn signed_two_digit_year_read_at_either_end_of_range() {
    let cases = [
        ("-44", -44, None, None),
        ("+77", 77, None, None),
        ("-77", -77, None, None),
        ("- 44", -44, None, None),
        ("-44-03-15", -44, Some(3), Some(15)),
        ("-44-02-29", -44, Some(2), Some(29)),
        ("-44/2020", -44, None, None),
        ("/-44", -44, None, None),
        ("../-44", -44, None, None),
        ("-44~", -44, None, None),
        ("-0044", -44, None, None),
        ("-4", -4, None, None),
        ("2020/-44", 2020, None, None),
    ];
    for (date, year, month, day) in cases {
        assert_eq!(
            date_of(date),
            Some(PublicationDate { year, month, day }),
            "date = {{{date}}}"
        );
    }
}

/// A day that does not exist in a signed two-digit year is not a date, so the `year` is read.
#[test]
fn impossible_day_in_signed_two_digit_year_falls_back() {
    assert_eq!(date_of("-45-02-29").map(|date| date.year), Some(1999));
}

/// A two-digit year without a sign expands to a year from 1950 to 2049.
#[test]
fn unsigned_two_digit_year_expanded() {
    assert_eq!(date_of("44").map(|date| date.year), Some(2044));
    assert_eq!(date_of("99").map(|date| date.year), Some(1999));
}

/// An entry whose date is filled in from another entry gets the underlying parser's reading of
/// a signed two-digit year, through `crossref` and through `xdata` alike.
#[test]
fn inherited_signed_two_digit_year_expanded() {
    let inputs = [
        "@proceedings{p, title = {P}, date = {-44}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n",
        "@xdata{p, date = {-44}}\n@inproceedings{c, title = {C}, xdata = {p}}\n",
    ];
    for input in inputs {
        let parsed = parse(input).unwrap();
        assert_eq!(years(&parsed), vec![Some(-44), Some(1956)], "{input}");
    }
}

/// A signed two-digit year with a valid month and day.
#[derive(Debug, Arbitrary)]
struct SignedShortDate {
    /// Whether the year is before the year 0.
    negative: bool,
    /// The year's two digits.
    #[strategy(0_i32..100)]
    digits: i32,
    /// Whitespace between the sign and the digits.
    #[strategy("[ \t\u{a0}]{0,2}")]
    space: String,
    /// The month, if any.
    #[strategy(proptest::option::of(1_u8..=12))]
    month: Option<u8>,
    /// The day, if the month is given, valid in that month of that year.
    #[strategy(proptest::option::of(1_u8..=days_in_month(#negative, #digits, #month)))]
    day: Option<u8>,
    /// A trailing mark of an uncertain or approximate date.
    #[strategy("[?~%]?")]
    mark: String,
}

/// The days in `month` of the year a signed two-digit year stands for; 31 without a month.
fn days_in_month(negative: bool, digits: i32, month: Option<u8>) -> u8 {
    let year = if negative { -digits } else { digits };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        Some(2) if leap => 29,
        Some(2) => 28,
        Some(4 | 6 | 9 | 11) => 30,
        _ => 31,
    }
}

/// Every signed two-digit year is read as written, with its month and day.
#[proptest]
fn signed_two_digit_years_read_as_written(input: SignedShortDate) {
    let sign = if input.negative { '-' } else { '+' };
    let mut date = format!("{sign}{}{:02}", input.space, input.digits);
    let day = input.month.and(input.day);
    if let Some(month) = input.month {
        write!(date, "-{month:02}").unwrap();
        if let Some(day) = day {
            write!(date, "-{day:02}").unwrap();
        }
    }
    date.push_str(&input.mark);
    let year = if input.negative {
        -input.digits
    } else {
        input.digits
    };
    prop_assert_eq!(
        date_of(&date),
        Some(PublicationDate {
            year,
            month: input.month,
            day,
        }),
        "{}",
        date
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
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
    let records = parse(input).unwrap().records;
    assert_eq!(records[0].journal.as_deref(), Some("Science"));
}

// -- Identifiers --

/// Parse a single entry whose fields, after the title, are `field_lines`, and return its
/// identifier.
fn identifier_of(field_lines: &str) -> Option<String> {
    let input = format!("@article{{k,\n  title = {{t}},\n{field_lines}}}\n");
    parse(&input).unwrap().records.remove(0).isbn
}

/// An entry with only an ISSN gets it as its identifier.
#[test]
fn issn_read_when_no_isbn() {
    assert_eq!(
        identifier_of("  issn = {1234-5678},\n").as_deref(),
        Some("1234-5678")
    );
}

/// The ISBN is preferred over the ISSN, whichever comes first.
#[test]
fn isbn_preferred_over_issn() {
    let isbn_first = identifier_of("  isbn = {978-3-16-148410-0},\n  issn = {1234-5678},\n");
    let isbn_last = identifier_of("  issn = {1234-5678},\n  isbn = {978-3-16-148410-0},\n");
    assert_eq!(isbn_first.as_deref(), Some("978-3-16-148410-0"));
    assert_eq!(isbn_last.as_deref(), Some("978-3-16-148410-0"));
}

/// An empty or whitespace-only ISBN does not hide an ISSN that has a value.
#[test]
fn blank_isbn_does_not_hide_issn() {
    let empty = identifier_of("  isbn = {},\n  issn = {1234-5678},\n");
    let spaces = identifier_of("  isbn = { },\n  issn = {1234-5678},\n");
    assert_eq!(empty.as_deref(), Some("1234-5678"));
    assert_eq!(spaces.as_deref(), Some("1234-5678"));
}

/// Field names are case-insensitive, so `ISSN` is read like `issn`.
#[test]
fn issn_field_name_case_insensitive() {
    assert_eq!(
        identifier_of("  ISSN = {1234-5678},\n").as_deref(),
        Some("1234-5678")
    );
}

/// A field listing several ISSNs is kept as one string, not split.
#[test]
fn several_issns_kept_verbatim() {
    let identifier = identifier_of("  issn = {1234-5678, 2345-6789},\n");
    assert_eq!(identifier.as_deref(), Some("1234-5678, 2345-6789"));
}

// -- Lenient parsing --

/// Titles of the parsed records, in order.
fn titles(parsed: &biblio::Parsed) -> Vec<Option<&str>> {
    parsed.records.iter().map(|r| r.title.as_deref()).collect()
}

/// A broken entry between two valid ones is skipped and reported at its line; the others are kept.
#[test]
fn broken_entry_between_valid_ones_skipped() {
    let input = "@article{a, title = {A}}\n@article{b title = {B}}\n@article{c, title = {C}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("C")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 2, .. }]
    ));
}

/// The message of a skipped entry describes the problem without byte offsets into the input.
#[test]
fn skipped_entry_message_has_no_offsets() {
    let parsed = parse("@article{b title = {B}}\n").unwrap();
    let [Diagnostic::SkippedEntry { message, .. }] = parsed.diagnostics.as_slice() else {
        panic!("expected one skipped entry, got {:?}", parsed.diagnostics);
    };
    assert_ne!(message.as_str(), "");
    assert!(
        !message.ends_with(|c: char| c.is_ascii_digit()),
        "{message}"
    );
}

/// An unclosed brace does not swallow the entries after it.
#[test]
fn unclosed_brace_does_not_swallow_next_entry() {
    let input = "@article{a, title = {never closed\n@article{b, title = {B}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// `@string` macros still expand when entries are parsed one by one.
#[test]
fn string_macros_expand_after_a_broken_entry() {
    let input = "\
@string{jn = {Nature}}
@article{a, title = {A}, journal = jn}
@article{b, title = {B}
";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A")]);
    assert_eq!(parsed.records[0].journal.as_deref(), Some("Nature"));
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 3, .. }]
    ));
}

/// Entries with the same citation key are all kept, without a diagnostic.
#[test]
fn duplicate_keys_keep_every_entry() {
    let parsed = parse("@article{a, title = {One}}\n@article{a, title = {Two}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("One"), Some("Two")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A broken `@string` is reported, and the macros that could be read still expand.
#[test]
fn broken_string_skipped_others_expand() {
    let input = "\
@string{good = {Nature}}
@string{bad = {never closed}
@article{a, title = {A}, journal = good}
";
    let parsed = parse(input).unwrap();
    assert_eq!(parsed.records[0].journal.as_deref(), Some("Nature"));
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 2, .. }]
    ));
}

/// An entry that uses an undefined macro is skipped; the other entries are kept.
#[test]
fn undefined_macro_skips_only_its_entry() {
    let input = "@article{a, title = {A}, journal = nope}\n@article{b, title = {B}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// An entry after other text on the first line is still read when the per-entry path is taken
/// (here forced by a duplicate key). Regression test from code review.
#[test]
fn entry_after_leading_text_kept() {
    let parsed = parse("See @article{a, title = {A}}\n@article{a, title = {B}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("B")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// An entry after a byte-order mark is still read when the per-entry path is taken. Regression
/// test from code review.
#[test]
fn entry_after_byte_order_mark_kept() {
    let parsed = parse("\u{feff}@article{a, title = {A}}\n@article{a, title = {B}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("B")]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// An entry whose brace is on the line after its type is still read when the per-entry path is
/// taken. Regression test from code review.
#[test]
fn entry_with_brace_on_next_line_kept() {
    let input = "@article\n{a, title = {A}}\n@article{b, title = {B}\n@article{c, title = {C}}\n";
    let parsed = parse(input).unwrap();
    assert_eq!(titles(&parsed), vec![Some("A"), Some("C")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 3, .. }]
    ));
}

/// Entries written with parentheses are not supported and are skipped.
#[test]
fn parenthesised_entry_skipped() {
    let parsed = parse("@article(a, title = {A})\n@article{b, title = {B}}\n").unwrap();
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

// -- Extras --

/// Fields without a slot are kept under `bibtex:`, with the citation key and entry type as
/// `bibtex:@key` and `bibtex:@type`.
#[test]
fn unread_fields_kept_in_extras() {
    let input = "@book{k1, title = {T}, note = {A note}, Publisher = {Pub}, keywords = {a, b}}\n";
    let records = parse(input).unwrap().records;
    let extras = &records[0].extras;
    assert_eq!(extras["bibtex:@key"], vec!["k1"]);
    assert_eq!(extras["bibtex:@type"], vec!["book"]);
    assert_eq!(extras["bibtex:note"], vec!["A note"]);
    // Field names are lower-cased.
    assert_eq!(extras["bibtex:publisher"], vec!["Pub"]);
    assert_eq!(extras["bibtex:keywords"], vec!["a, b"]);
    assert_eq!(extras.len(), 5);
}

/// An entry type the parser does not know keeps its name, and `@unknown` reads as "unknown".
#[test]
fn unknown_entry_types_keep_their_name() {
    let input = "@dataset{a, title = {A}}\n@unknown{b, title = {B}}\n";
    let records = parse(input).unwrap().records;
    assert_eq!(first_extra(&records[0], "bibtex:@type"), Some("dataset"));
    assert_eq!(first_extra(&records[1], "bibtex:@type"), Some("unknown"));
}

/// A month macro is expanded to the month's name.
#[test]
fn month_macro_kept_as_name() {
    let records = parse("@article{k, title = {T}, month = jan}\n")
        .unwrap()
        .records;
    assert_eq!(records[0].extras["bibtex:month"], vec!["January"]);
}

/// Fields a `crossref` child inherits from its parent are kept in the child's extras, along with
/// the `crossref` itself.
#[test]
fn crossref_inheritance_kept_in_extras() {
    let input = "\
@book{parent, title = {P}, publisher = {Pub}}
@incollection{child, title = {C}, crossref = {parent}}
";
    let records = parse(input).unwrap().records;
    let child = &records[1];
    assert_eq!(child.extras["bibtex:crossref"], vec!["parent"]);
    assert_eq!(child.extras["bibtex:publisher"], vec!["Pub"]);
}

#[test]
fn extras_citation_key() {
    let input = r"
@article{mykey2024,
  title = {Test},
}
";
    let records = parse(input).unwrap().records;
    assert_eq!(first_extra(&records[0], "bibtex:@key"), Some("mykey2024"));
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
/// explicitly; the test guards it.
#[test]
fn leading_byte_order_mark_ignored() {
    let input = "@article{a, title = {First}}\n";
    let parsed = parse(&format!("\u{feff}{input}")).unwrap();
    assert_eq!(parsed, parse(input).unwrap());
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title.as_deref(), Some("First"));
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A broken entry is skipped and reported at its line instead of failing the whole input.
#[test]
fn malformed_entry_skipped() {
    let parsed = parse("@article{broken, title = {missing close brace}").unwrap();
    assert_eq!(parsed.records, Vec::<Record>::new());
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// An entry without a title is kept, and the missing title is reported at the entry's line.
#[test]
fn missing_title_kept_and_reported() {
    let input = r"
@article{x,
  year = {2024},
}
";
    let parsed = parse(input).unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].title, None);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 2 }]
    );
}

// -- Serialization --

/// The entry type, key and extra fields are written back, so they read back unchanged.
#[test]
fn serialize_writes_type_key_and_extras() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:@type", "book")
        .extra("bibtex:@key", "k1")
        .extra("bibtex:note", "A note")
        .build();
    let output = serialize(std::slice::from_ref(&record));
    assert!(output.starts_with("@book{k1,\n"), "{output}");
    assert!(output.contains("  note = {A note},\n"));
    assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Only `bibtex:` extras are written, and only the first value of each.
#[test]
fn serialize_writes_only_bibtex_extras_once() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:note", "first")
        .extra("bibtex:note", "second")
        .extra("ris:KW", "cats")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("note = {first}"));
    assert!(!output.contains("second"));
    assert!(!output.contains("cats"));
}

/// Extras that are not valid field names, name a field the parser reads, or cannot be resolved,
/// are not written; an invalid type or key falls back to the default.
#[test]
fn serialize_skips_unwritable_extras() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:@type", "a b")
        .extra("bibtex:@key", "has space")
        .extra("bibtex:Bad Name", "x")
        .extra("bibtex:xdata", "x")
        .extra("bibtex:journal", "would become the journal")
        .build();
    assert_eq!(
        serialize(&[record]),
        "@article{unknown,\n  title = {T},\n}\n"
    );

    let comment = Record::builder()
        .extra("bibtex:@type", "comment")
        .extra("bibtex:@key", "self")
        .extra("bibtex:crossref", "self")
        .build();
    assert_eq!(serialize(&[comment]), "@article{self,\n}\n");
}

/// A month from the date is written instead of a `bibtex:month` extra, so the field appears once.
#[test]
fn date_month_wins_over_month_extra() {
    let record = Record::builder()
        .title("T")
        .date(PublicationDate {
            year: 2024,
            month: Some(3),
            day: None,
        })
        .extra("bibtex:month", "January")
        .build();
    let output = serialize(&[record]);
    assert_eq!(output.matches("month =").count(), 1, "{output}");
    assert!(output.contains("month = {3}"));
}

/// A `crossref` that would close a cycle is not written, whether the cycle runs through another
/// entry or through the entry's own `ids` alias, so the output can always be read back. A
/// `crossref` that only leads into other entries is kept.
#[test]
fn crossref_cycles_not_written() {
    let entry = |key: &str, crossref: &str| {
        Record::builder()
            .title(key)
            .extra("bibtex:@key", key)
            .extra("bibtex:crossref", crossref)
            .build()
    };
    let pair = serialize(&[entry("a", "b"), entry("b", "a")]);
    assert!(!pair.contains("crossref"), "{pair}");
    assert_eq!(parse(&pair).unwrap().records.len(), 2);

    let alias = Record::builder()
        .title("A")
        .extra("bibtex:@key", "a")
        .extra("bibtex:ids", "x, b")
        .extra("bibtex:crossref", "b")
        .build();
    let output = serialize(&[alias]);
    assert!(!output.contains("crossref"), "{output}");
    assert_eq!(parse(&output).unwrap().records.len(), 1);

    let parent = Record::builder()
        .title("P")
        .extra("bibtex:@key", "p")
        .build();
    let chain = serialize(&[entry("c", "p"), parent]);
    assert!(chain.contains("crossref = {p}"), "{chain}");
}

/// An extra with no values writes nothing.
#[test]
fn serialize_skips_extras_without_values() {
    let mut record = Record::builder().title("T").build();
    record.extras.insert("bibtex:note".to_owned(), Vec::new());
    assert_eq!(
        serialize(&[record]),
        "@article{unknown,\n  title = {T},\n}\n"
    );
}

/// An entry whose `crossref` leads into a cycle it is not part of keeps its `crossref`; only the
/// entries on the cycle lose theirs.
#[test]
fn crossref_into_a_cycle_kept() {
    let entry = |key: &str, crossref: &str| {
        Record::builder()
            .title(key)
            .extra("bibtex:@key", key)
            .extra("bibtex:crossref", crossref)
            .build()
    };
    let output = serialize(&[entry("c", "a"), entry("a", "b"), entry("b", "a")]);
    let records = parse(&output).unwrap().records;
    let crossrefs: Vec<_> = records
        .iter()
        .map(|r| first_extra(r, "bibtex:crossref"))
        .collect();
    assert_eq!(crossrefs, [Some("a"), None, None], "{output}");
}

/// A field name with an uppercase letter is not written, even when it is otherwise valid, because
/// the parser lower-cases names and it would read back under a different key.
#[test]
fn serialize_skips_uppercase_field_names() {
    let record = Record::builder()
        .title("T")
        .extra("bibtex:Note", "x")
        .build();
    assert_eq!(
        serialize(&[record]),
        "@article{unknown,\n  title = {T},\n}\n"
    );
}

/// A `bibtex:month` extra is written when the date has no month to write instead, or there is no
/// date at all.
#[test]
fn month_extra_written_without_date_month() {
    let year_only = Record::builder()
        .title("T")
        .date(PublicationDate {
            year: 2024,
            month: None,
            day: None,
        })
        .extra("bibtex:month", "March")
        .build();
    assert!(serialize(&[year_only]).contains("month = {March}"));

    let no_date = Record::builder()
        .title("T")
        .extra("bibtex:month", "March")
        .build();
    assert!(serialize(&[no_date]).contains("month = {March}"));
}

/// The special entry types `@comment`, `@string` and `@preamble`, in any case, are never written as
/// the entry type, because the entry would vanish or fail to parse.
#[test]
fn special_entry_types_fall_back_to_article() {
    for entry_type in ["comment", "string", "preamble", "Comment", "PREAMBLE"] {
        let record = Record::builder()
            .title("T")
            .extra("bibtex:@type", entry_type)
            .build();
        let output = serialize(&[record]);
        assert!(output.starts_with("@article{"), "{entry_type}: {output}");
    }
}

/// A field name starting with a character the parser does not accept at the start of a name is not
/// written; the same character later in the name is fine.
#[test]
fn field_names_with_invalid_first_character_skipped() {
    for first in [':', '<', '-', '>'] {
        let record = Record::builder()
            .title("T")
            .extra(format!("bibtex:{first}x"), "skipped")
            .extra(format!("bibtex:x{first}y"), "kept")
            .build();
        let output = serialize(&[record]);
        assert!(!output.contains("skipped"), "{first}: {output}");
        assert!(
            output.contains(&format!("x{first}y = {{kept}}")),
            "{first}: {output}"
        );
    }
}

/// Values are escaped so that braces, backslashes and dollar signs read back unchanged, except
/// that `$` is left alone in verbatim fields such as `url`.
#[test]
fn serialize_escapes_values() {
    let record = Record::builder()
        .title("A {braced} title")
        .extra("bibtex:@key", "k")
        .extra("bibtex:@type", "article")
        .extra("bibtex:note", "costs $5, ends with \\")
        .extra("bibtex:url", "https://example.org/$id")
        .build();
    let output = serialize(std::slice::from_ref(&record));
    assert!(
        output.contains("url = {https://example.org/$id}"),
        "{output}"
    );
    assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Converting `BibTeX` to `RIS` writes no `BibTeX` key or fields into the `RIS`, so it reads back
/// cleanly. Regression test: the citation key used to be written as a `key  -` line, which read
/// back as part of the previous field.
#[test]
fn bibtex_to_ris_writes_no_foreign_tags() {
    let input = "\
@article{doe2024,
  title  = {An interesting finding},
  author = {Doe, Jane and Smith, John},
  year   = {2024},
  journal = {Nature},
  note   = {A note},
}
";
    let records = parse(input).unwrap().records;
    let ris_output = biblio::ris::serialize(&records);
    assert!(!ris_output.contains("doe2024"), "{ris_output}");
    assert!(!ris_output.contains("A note"));
    let reparsed = biblio::ris::parse(&ris_output).unwrap();
    assert_eq!(reparsed.diagnostics, Vec::<Diagnostic>::new());
    assert_eq!(reparsed.records[0].title, records[0].title);
    assert_eq!(reparsed.records[0].journal, records[0].journal);
}

/// Several entries with distinct keys are written and read back whole, keys included.
#[test]
fn serialize_multiple_entries() {
    let a = Record::builder()
        .title("First")
        .extra("bibtex:@key", "a")
        .build();
    let b = Record::builder()
        .title("Second")
        .extra("bibtex:@key", "b")
        .build();
    let output = serialize(&[a, b]);
    let parsed = parse(&output).unwrap();
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
    let reparsed = parsed.records;
    assert_eq!(first_extra(&reparsed[0], "bibtex:@key"), Some("a"));
    assert_eq!(first_extra(&reparsed[1], "bibtex:@key"), Some("b"));
    assert_eq!(reparsed.len(), 2);
    assert_eq!(reparsed[0].title.as_deref(), Some("First"));
    assert_eq!(reparsed[1].title.as_deref(), Some("Second"));
}

#[test]
fn serialize_with_month() {
    let record = Record::builder()
        .title("Test")
        .date(PublicationDate {
            year: 2024,
            month: Some(3),
            day: None,
        })
        .extra("bibtex:@key", "test")
        .build();
    let output = serialize(&[record]);
    assert!(output.contains("year = {2024}"));
    assert!(output.contains("month = {3}"));
}

#[test]
fn serialize_skips_none_fields() {
    let record = Record::builder()
        .title("Minimal")
        .extra("bibtex:@key", "min")
        .build();
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

/// Fields the generator never uses as extras: the fields the `BibTeX` parser reads (a copy of its
/// list), `xdata` (consumed while resolving), and `crossref`, `ids` and `set`, which refer to other
/// entries.
const BIBTEX_RESERVED_FIELDS: [&str; 17] = [
    "title",
    "author",
    "date",
    "year",
    "journaltitle",
    "journal",
    "doi",
    "pages",
    "volume",
    "number",
    "abstract",
    "isbn",
    "issn",
    "xdata",
    "crossref",
    "ids",
    "set",
];

/// Entry types that do not read back as themselves: aliases that the parser renames, `unknown`,
/// and the special entries `@comment`, `@string`, `@preamble`, `@xdata` and `@set`.
const UNSTABLE_ENTRY_TYPES: [&str; 9] = [
    "conference",
    "electronic",
    "www",
    "unknown",
    "comment",
    "string",
    "preamble",
    "xdata",
    "set",
];

/// An entry type that reads back as itself: a common one, or any lower-case name that is not
/// special.
fn arb_entry_type() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::sample::select(vec!["article", "book", "inproceedings", "misc", "dataset"])
            .prop_map(str::to_owned),
        "[a-z]{3,10}".prop_filter("entry type must read back unchanged", |name| {
            !UNSTABLE_ENTRY_TYPES.contains(&name.as_str())
        }),
    ]
}

/// Text that reads back unchanged from a `BibTeX` field: no braces (which would break the format),
/// hyphens (`--` becomes an en dash), backslashes or tildes (`~` becomes a no-break space), with
/// single spaces only and no surrounding whitespace, because the parser collapses whitespace.
fn arb_bib_text(max_len: usize) -> impl Strategy<Value = String> {
    let bib_safe = "[A-Za-z0-9 .,;:!?'()\\[\\]/<>@#%^*+=_]";
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
}

/// Up to four fields the parser does not read, each with one value (a field appears once per
/// entry).
fn arb_bibtex_unread_fields() -> impl Strategy<Value = HashMap<String, String>> {
    proptest::collection::hash_map(
        "[a-z]{2,10}".prop_filter("field must not be read or special", |name| {
            // `month` would give way to a date's month when written.
            !BIBTEX_RESERVED_FIELDS.contains(&name.as_str()) && name != "month"
        }),
        arb_bib_text(40),
        0..=4,
    )
}

fn arb_bibtex_record() -> impl Strategy<Value = Record> {
    (
        proptest::option::of(arb_bib_text(80)),
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
            arb_entry_type(),
            arb_bibtex_unread_fields(),
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
                (isbn, key, entry_type, unread),
            )| {
                let mut extras: HashMap<String, Vec<String>> = unread
                    .into_iter()
                    .map(|(name, value)| (format!("bibtex:{name}"), vec![value]))
                    .collect();
                extras.insert("bibtex:@key".to_owned(), vec![key]);
                extras.insert("bibtex:@type".to_owned(), vec![entry_type]);
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
fn serialize_round_trip(#[strategy(arb_bibtex_record())] record: Record) {
    let serialized = serialize(std::slice::from_ref(&record));
    let reparsed = parse(&serialized).unwrap().records;
    prop_assert_eq!(vec![record], reparsed);
}

/// Parsing arbitrary text wrapped in fragments of `BibTeX` syntax (record and field openers,
/// closers and line endings) never panics.
#[proptest]
fn adversarial_parse_no_panic(
    #[strategy("(@article\\{|@misc\\{|title = \\{|author = \\{|\\}|\n){0,5}")] prefix: String,
    #[strategy("\\PC{0,200}")] body: String,
    #[strategy("(\\}|\n){0,3}")] suffix: String,
) {
    let input = format!("{prefix}{body}{suffix}");
    let _ = parse(&input);
}

// -- Identifier properties --

/// Identifier values as they appear in exports: single ISSNs and ISBN-13s, pairs of them, empty,
/// whitespace only, and padded. They contain no braces, `--`, `~`, `\` or repeated inner spaces,
/// which the parser rewrites, so a parsed value is the generated value trimmed.
fn identifier_value() -> impl Strategy<Value = String> {
    let serial_number = "[0-9]{4}-[0-9]{3}[0-9X]";
    let book_number = "97[89]-[0-9]{1,5}-[0-9]{1,7}-[0-9]{1,6}-[0-9X]";
    let single = prop_oneof![serial_number, book_number];
    let pair = (
        single.clone(),
        prop_oneof![Just(", "), Just(" ")],
        single.clone(),
    )
        .prop_map(|(a, separator, b)| format!("{a}{separator}{b}"));
    let padded = ("[ ]{1,2}", single.clone(), "[ ]{1,2}")
        .prop_map(|(before, value, after)| format!("{before}{value}{after}"));
    prop_oneof![
        3 => single,
        1 => pair,
        1 => Just(String::new()),
        1 => "[ ]{1,3}",
        1 => padded,
    ]
}

/// The identifier fields of one entry: `isbn`, `issn` or both, in any order, each with a value.
#[derive(Arbitrary, Debug)]
struct IdentifierFields {
    /// One or both identifier fields, shuffled. No field repeats.
    #[strategy(subsequence(vec!["isbn", "issn"], 1..=2).prop_shuffle())]
    fields: Vec<&'static str>,
    /// One value for each entry in `fields`, in the same order.
    #[strategy(vec(identifier_value(), #fields.len()))]
    values: Vec<String>,
}

/// The parsed identifier matches a direct reference: trim the values, put `isbn` before `issn`,
/// then take the first non-blank value, or the first value if all are blank.
#[proptest]
fn identifier_fallback_matches_reference(input: IdentifierFields) {
    let mut field_lines = String::new();
    for (field, value) in input.fields.iter().zip(&input.values) {
        writeln!(field_lines, "  {field} = {{{value}}},").unwrap();
    }
    let identifier = identifier_of(&field_lines);

    let mut by_preference: Vec<(&str, String)> = input
        .fields
        .into_iter()
        .zip(input.values)
        .map(|(field, value)| (field, value.trim().to_owned()))
        .collect();
    by_preference.sort_by_key(|(field, _)| *field != "isbn");
    let expected = by_preference
        .iter()
        .find(|(_, value)| !value.is_empty())
        .or(by_preference.first())
        .map(|(_, value)| value.clone());

    prop_assert_eq!(identifier, expected);
}

// -- Lenient parsing properties --

/// One entry of a generated input: a valid entry with a title, or a broken one.
#[derive(Debug, Clone)]
enum GeneratedEntry {
    /// A valid entry with this key and title.
    Valid {
        /// The citation key, drawn from a small set so keys repeat.
        key: String,
        /// The title.
        title: String,
    },
    /// An entry whose title brace is never closed.
    Broken,
}

/// A valid entry (keys drawn from two letters, so they often repeat) or a broken one. Titles are
/// words separated by single spaces, because the parser collapses runs of spaces.
fn arb_generated_entry() -> impl Strategy<Value = GeneratedEntry> {
    prop_oneof![
        3 => ("[ab]", "[A-Za-z]{1,8}( [A-Za-z]{1,8}){0,3}")
            .prop_map(|(key, title)| GeneratedEntry::Valid { key, title }),
        1 => Just(GeneratedEntry::Broken),
    ]
}

/// Broken entries among valid ones are skipped and reported at their lines; every valid entry is
/// kept, in order.
#[proptest]
fn broken_entries_skipped(
    #[strategy(vec(arb_generated_entry(), 1..10))] entries: Vec<GeneratedEntry>,
) {
    let mut input = String::new();
    let mut expected_titles = Vec::new();
    let mut broken_lines = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            GeneratedEntry::Valid { key, title } => {
                writeln!(input, "@article{{{key}, title = {{{title}}}}}").unwrap();
                expected_titles.push(Some(title.as_str()));
            }
            GeneratedEntry::Broken => {
                writeln!(input, "@article{{broken, title = {{never closed").unwrap();
                broken_lines.push(index + 1);
            }
        }
    }

    let parsed = parse(&input).unwrap();
    prop_assert_eq!(titles(&parsed), expected_titles);
    let skipped: Vec<usize> = parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::SkippedEntry { .. }))
        .map(Diagnostic::line)
        .collect();
    prop_assert_eq!(skipped, broken_lines);
}

/// Each entry without a usable title is reported once, and diagnostics come in line order.
#[proptest]
fn missing_titles_reported_in_order(
    #[strategy(vec(proptest::option::of("[A-Za-z ]{0,10}"), 1..10))] titles: Vec<Option<String>>,
) {
    let mut input = String::new();
    for (index, title) in titles.iter().enumerate() {
        match title {
            Some(title) => writeln!(input, "@article{{k{index}, title = {{{title}}}}}").unwrap(),
            None => writeln!(input, "@article{{k{index}, year = {{2024}}}}").unwrap(),
        }
    }
    let parsed = parse(&input).unwrap();

    let untitled: Vec<usize> = titles
        .iter()
        .enumerate()
        .filter(|(_, title)| title.as_deref().is_none_or(|t| t.trim().is_empty()))
        .map(|(index, _)| index + 1)
        .collect();
    let missing: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    prop_assert_eq!(missing, untitled);
}

// -- Byte-order mark --

/// Any input, as serialised records or as arbitrary text. It never starts with a byte-order mark:
/// U+FEFF is a format character, which `\PC` excludes, and the serialiser never writes it.
fn arb_input_without_byte_order_mark() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::collection::vec(arb_bibtex_record(), 0..4)
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

/// Keys of the other formats, which `BibTeX` serialisation must never write.
const FOREIGN_KEYS: [&str; 3] = ["ris:KW", "nbib:OT", "endnote:keywords/keyword"];

/// An extra naming a field the parser reads (or `xdata`, which is consumed when resolving) is never
/// written, so it cannot leak into a field.
#[proptest]
fn read_fields_never_written_from_extras(
    #[strategy(proptest::sample::select(BIBTEX_RESERVED_FIELDS[..14].to_vec()))] name: &'static str,
    #[strategy(arb_bibtex_record())] record: Record,
) {
    let mut with_read_field = record.clone();
    with_read_field
        .extras
        .insert(format!("bibtex:{name}"), vec!["leaked value".to_owned()]);
    prop_assert_eq!(serialize(&[with_read_field]), serialize(&[record]));
}

/// Extras of other formats are kept on the record but never written as `BibTeX` fields.
#[proptest]
fn foreign_extras_not_written(
    #[strategy(arb_bibtex_record())] record: Record,
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

/// Pieces of a field value in `BibTeX` source: words, escaped special characters, groups, inline
/// maths and dashes, which the parser rewrites.
fn arb_value_token() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[A-Za-z0-9]{1,8}",
        1 => proptest::sample::select(vec!["\\{", "\\}", "\\$", "\\%", "\\&", "--"])
            .prop_map(str::to_owned),
        1 => "[A-Za-z]{1,6}".prop_map(|word| format!("{{{word}}}")),
        1 => "[A-Za-z]{1,6}".prop_map(|word| format!("${word}$")),
    ]
}

/// Entries read from source with arbitrary extra fields, whose values the parser rewrites, come
/// back unchanged when written and read again.
#[proptest]
fn records_stable_through_serialize(
    #[strategy(proptest::collection::hash_map(
        "[a-z]{2,10}".prop_filter("field must not be read or special", |name| {
            !BIBTEX_RESERVED_FIELDS.contains(&name.as_str()) && name != "month"
        }),
        proptest::collection::vec(arb_value_token(), 1..6).prop_map(|tokens| tokens.join(" ")),
        0..5,
    ))]
    fields: HashMap<String, String>,
) {
    let mut input = String::from("@misc{k,\n  title = {A title},\n");
    for (name, value) in &fields {
        writeln!(input, "  {name} = {{{value}}},").unwrap();
    }
    input.push_str("}\n");
    let first = parse(&input).unwrap();
    prop_assert_eq!(&first.diagnostics, &Vec::<Diagnostic>::new());
    let second = parse(&serialize(&first.records)).unwrap().records;
    prop_assert_eq!(second, first.records);
}

// -- Hostile input --

/// Parse `input` on a thread with the stack a test thread gets by default, set explicitly so that
/// settings such as `RUST_MIN_STACK` cannot hide a shrinking margin. An input that overflows the
/// stack still aborts the test run.
fn parse_hostile(input: &str) -> biblio::Parsed {
    let input = input.to_owned();
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || parse(&input).unwrap())
        .unwrap()
        .join()
        .unwrap()
}

// -- Hostile input: nesting --

/// The deepest brace nesting the parser accepts in a value; a copy of the parser's limit.
const MAX_NESTING: usize = 128;

/// A command argument nested `depth` levels deep, such as `\x{\x{x}}` for depth 2.
fn nested_commands(depth: usize) -> String {
    format!("{}x{}", "\\x{".repeat(depth), "}".repeat(depth))
}

/// A value nested exactly at the limit is read; one level deeper, the entry is skipped and
/// reported, and the other entries are kept.
#[test]
fn nesting_at_limit_kept_beyond_skipped() {
    let at_limit = nested_commands(MAX_NESTING);
    let parsed = parse_hostile(&format!(
        "@article{{a, title = {{T}}, note = {{{at_limit}}}}}\n"
    ));
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());

    let beyond = nested_commands(MAX_NESTING + 1);
    let input = format!("@article{{a, title = {{A}}}}\n@article{{b, note = {{{beyond}}}}}\n");
    let parsed = parse_hostile(&input);
    assert_eq!(titles(&parsed), vec![Some("A")]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 2, message }] if message == "fields nested too deeply"
    ));
}

/// Deep nesting is also caught when entries are parsed one by one, here forced by a broken entry
/// and with two entries on one line.
#[test]
fn nesting_caught_when_parsing_entries_one_by_one() {
    let beyond = nested_commands(MAX_NESTING + 1);
    let input = format!(
        "@article{{a, note = {{{beyond}}}}} @article{{b, title = {{B}}}}\n{}",
        "@article{c title = {C}}\n"
    );
    let parsed = parse_hostile(&input);
    assert_eq!(titles(&parsed), vec![Some("B")]);
    let lines: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    assert_eq!(lines, vec![1, 2]);
}

/// Plain groups and quoted values with unbalanced braces count too, so they are skipped beyond
/// the limit.
#[test]
fn deep_groups_and_quoted_braces_skipped() {
    let groups = format!(
        "{}x{}",
        "{".repeat(MAX_NESTING + 1),
        "}".repeat(MAX_NESTING + 1)
    );
    let quoted = "{".repeat(MAX_NESTING + 1);
    for value in [format!("{{{groups}}}"), format!("\"{quoted}\"")] {
        let parsed = parse_hostile(&format!("@article{{a, note = {value}}}\n"));
        assert_eq!(parsed.records, Vec::<Record>::new(), "{value}");
        assert!(
            matches!(
                parsed.diagnostics.as_slice(),
                [Diagnostic::SkippedEntry { line: 1, message }]
                    if message == "fields nested too deeply"
            ),
            "{value}: {:?}",
            parsed.diagnostics
        );
    }
}

/// A deeply nested `crossref` value is skipped like any other field.
#[test]
fn deep_crossref_value_skipped() {
    let beyond = nested_commands(MAX_NESTING + 1);
    let parsed = parse_hostile(&format!("@article{{a, crossref = {{{beyond}}}}}\n"));
    assert_eq!(parsed.records, Vec::<Record>::new());
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, .. }]
    ));
}

/// A `@string` whose first definition nests too deeply is removed, with every later definition of
/// the same name, and the entry that uses it is skipped.
#[test]
fn deep_string_removed_with_its_redefinitions() {
    let beyond = nested_commands(MAX_NESTING + 1);
    let input = format!(
        "@string{{s = {{{beyond}}}}}\n{}",
        "@string{s = {shallow}}\n@article{a, title = s}\n@article{b, title = {B}}\n"
    );
    let parsed = parse_hostile(&input);
    assert_eq!(titles(&parsed), vec![Some("B")]);
    let reported: Vec<(usize, String)> = parsed
        .diagnostics
        .iter()
        .map(|d| match d {
            Diagnostic::SkippedEntry { line, message } => (*line, message.clone()),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        reported[0],
        (1, "@string s is nested too deeply".to_owned())
    );
    assert_eq!(reported[1].0, 3);
    assert_eq!(reported.len(), 2);
}

/// Values at the nesting limit, reached through a long chain of `@string`s, parse on a 2 MiB
/// stack.
#[test]
fn nesting_limit_fits_on_a_test_thread() {
    let mut input = format!("@string{{s0 = {{{}}}}}\n", nested_commands(MAX_NESTING));
    for i in 1..64 {
        writeln!(input, "@string{{s{i} = s{}}}", i - 1).unwrap();
    }
    input.push_str("@article{a, title = {T}, note = s63}\n");
    assert_eq!(parse_hostile(&input).records.len(), 1);
}

/// Command arguments nested 10,000 deep are skipped rather than overflowing the stack.
#[test]
fn very_deep_nesting_skipped() {
    let parsed = parse_hostile(&format!(
        "@article{{a, title = {{{}}}}}\n",
        nested_commands(10_000)
    ));
    assert_eq!(parsed.records, Vec::<Record>::new());
    assert_eq!(parsed.diagnostics.len(), 1);
}

/// Pieces of deeply nested LaTeX: command arguments, groups, escaped braces, maths and letters.
fn arb_nesting_piece() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec!["\\x{", "{", "}", "\\{", "$", "a"])
}

/// The deepest brace nesting in `text`, counted as the parser's guard counts it: every `{` and `}`,
/// never below zero. A copy of the parser's rule, so that the tests pin it.
fn brace_depth(text: &str) -> usize {
    let (mut depth, mut deepest) = (0_usize, 0);
    for c in text.chars() {
        match c {
            '{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// Any mixture of commands, groups and braces, nested up to twice the limit, parses without
/// overflowing the stack.
#[proptest]
fn nesting_never_aborts(
    #[strategy(proptest::collection::vec(arb_nesting_piece(), 0..2 * MAX_NESTING * 2))] pieces: Vec<
        &'static str,
    >,
) {
    let value: String = pieces.concat();
    let input = format!("@article{{a, title = {{T}}, note = {{{value}}}}}\n");
    parse_hostile(&input);
}

/// A well-formed value made of commands, groups, maths and letters, with every brace closed, up to
/// twice the limit deep.
fn arb_balanced_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(vec!["\\x{", "{", "}", "$a$", "a"]),
        0..4 * MAX_NESTING,
    )
    .prop_map(|pieces| {
        let mut value = String::new();
        let mut open = 0_usize;
        for piece in pieces {
            if piece == "}" {
                if open == 0 {
                    continue;
                }
                open -= 1;
            } else if piece.ends_with('{') {
                open += 1;
            }
            value.push_str(piece);
        }
        value.push_str(&"}".repeat(open));
        value
    })
}

/// A well-formed value is skipped exactly when its braces nest deeper than the limit.
#[proptest]
fn deep_values_skipped_exactly(#[strategy(arb_balanced_value())] value: String) {
    let input = format!("@article{{a, title = {{T}}, note = {{{value}}}}}\n");
    let parsed = parse_hostile(&input);
    let too_deep = brace_depth(&value) > MAX_NESTING;
    prop_assert_eq!(parsed.records.is_empty(), too_deep);
    prop_assert_eq!(parsed.diagnostics.len(), usize::from(too_deep));
}

/// A deep `@string` in a later part of the input is reported at its own line when entries are
/// parsed one by one.
#[test]
fn deep_string_line_on_the_per_entry_path() {
    let beyond = nested_commands(MAX_NESTING + 1);
    let input = format!(
        "@article{{x title = {{X}}}}\n@article{{a, title = {{A}}}}\n\n@string{{s = {{{beyond}}}}}\n"
    );
    let parsed = parse_hostile(&input);
    let lines: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    assert_eq!(lines, vec![1, 4]);
}

/// In a concatenated `@string`, each part is judged on its own, because the parser parses each
/// part separately: two parts at the limit together are fine.
#[test]
fn concatenated_string_parts_judged_separately() {
    let at_limit = nested_commands(MAX_NESTING);
    let input = format!(
        "@string{{a = {{{at_limit}}}}}\n@string{{s = a # {{{at_limit}}}}}\n{}",
        "@article{k, title = {T}, note = s}\n"
    );
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// Many `@string` definitions are checked in linear time. Regression test from code review: the
/// check once compared every name with every other.
#[test]
fn many_strings_parse_quickly() {
    let mut input = String::new();
    for i in 0..40_000 {
        writeln!(input, "@string{{s{i} = {{x}}}}").unwrap();
    }
    input.push_str("@article{a, title = {T}}\n");
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 1);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
}

// -- Hostile input: reference cycles --

/// The diagnostics of `input`, which must parse.
fn diagnostics_of(input: &str) -> Vec<Diagnostic> {
    parse_hostile(input).diagnostics
}

/// The `crossref` each record kept, in order.
fn crossrefs(parsed: &biblio::Parsed) -> Vec<Option<&str>> {
    parsed
        .records
        .iter()
        .map(|r| first_extra(r, "bibtex:crossref"))
        .collect()
}

/// An entry whose `crossref` names itself, directly or through its own `ids` alias, keeps its
/// fields but loses the `crossref`, and the cycle is reported.
#[test]
fn crossref_to_itself_removed() {
    for input in [
        "@article{a, title = {A}, crossref = {a}}\n",
        "@article{a, title = {A}, ids = {x}, crossref = {x}}\n",
    ] {
        let parsed = parse_hostile(input);
        assert_eq!(titles(&parsed), vec![Some("A")], "{input}");
        assert_eq!(crossrefs(&parsed), vec![None], "{input}");
        assert_eq!(
            parsed.diagnostics,
            vec![Diagnostic::ReferenceCycle { line: 1 }]
        );
    }
}

/// In a cycle between two entries, the reference that closes it is removed: the second entry's,
/// because entries are searched in order.
#[test]
fn crossref_cycle_between_entries_broken() {
    let parsed = parse_hostile(
        "@article{a, title = {A}, crossref = {b}}\n@article{b, title = {B}, crossref = {a}}\n",
    );
    assert_eq!(crossrefs(&parsed), vec![Some("b"), None]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::ReferenceCycle { line: 2 }]
    );
}

/// An `xdata` that refers back to its own entry is removed.
#[test]
fn xdata_cycle_broken() {
    let parsed = parse_hostile(
        "@xdata{d, note = {N}, xdata = {d}}\n@article{a, title = {A}, xdata = {d}}\n",
    );
    assert_eq!(parsed.records.len(), 2);
    assert!(
        parsed
            .diagnostics
            .contains(&Diagnostic::ReferenceCycle { line: 1 })
    );
}

/// A cycle is found however the key is written: through a `@string`, or with LaTeX that the
/// parser turns into the same key.
#[test]
fn cycles_found_through_strings_and_latex_in_keys() {
    for input in [
        "@string{k = {a}}\n@article{a, title = {A}, crossref = k}\n",
        "@misc{a_b, title = {A}, crossref = {a\\_b}}\n",
        "@misc{a\\b, title = {A}, crossref = {a\\\\b}}\n",
        "@misc{a–b, title = {A}, crossref = {a--b}}\n",
        "@misc{é, title = {A}, crossref = {\\'e}}\n",
    ] {
        let parsed = parse_hostile(input);
        assert_eq!(parsed.records.len(), 1, "{input}");
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::ReferenceCycle { .. })),
            "{input}: {:?}",
            parsed.diagnostics
        );
    }
}

/// A cycle hidden behind a duplicate field is still found, because the parser keeps the last of
/// duplicate fields, and every copy of the field is removed.
#[test]
fn cycle_behind_duplicate_field_broken() {
    let parsed = parse_hostile("@article{a, title = {A}, crossref = {x}, crossref = {a}}\n");
    assert_eq!(crossrefs(&parsed), vec![None]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::ReferenceCycle { line: 1 }]
    );
}

/// A later `ids` alias takes over an earlier key, as the parser resolves it, and a cycle through
/// it is found.
#[test]
fn cycle_through_alias_taking_over_a_key_broken() {
    let input = "\
@article{a, title = {A}}
@article{b, title = {B}, ids = {a}, crossref = {c}}
@article{c, title = {C}, crossref = {a}}
";
    let parsed = parse_hostile(input);
    assert_eq!(parsed.records.len(), 3);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::ReferenceCycle { line: 3 }]
    );
}

/// A cycle between two entries on one line is broken when entries are parsed one by one, here
/// forced by a broken entry.
#[test]
fn cycle_broken_when_parsing_entries_one_by_one() {
    let input = concat!(
        "@article{a, title = {A}, crossref = {b}} @article{b, title = {B}, crossref = {a}}\n",
        "@article{c title = {C}}\n",
    );
    let parsed = parse_hostile(input);
    assert_eq!(titles(&parsed), vec![Some("A"), Some("B")]);
    assert!(
        parsed
            .diagnostics
            .contains(&Diagnostic::ReferenceCycle { line: 1 })
    );
}

/// Only the entry whose reference closes a cycle loses it: entries leading into the cycle, and the
/// rest of the cycle, keep their inheritance.
#[test]
fn chain_into_a_cycle_keeps_inheritance() {
    let input = "\
@book{a, title = {A}, publisher = {PubA}, crossref = {b}}
@book{b, title = {B}, publisher = {PubB}, crossref = {a}}
@incollection{c, title = {C}, crossref = {a}}
";
    let parsed = parse_hostile(input);
    assert_eq!(crossrefs(&parsed), vec![Some("b"), None, Some("a")]);
    assert_eq!(
        first_extra(&parsed.records[2], "bibtex:publisher"),
        Some("PubA")
    );
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::ReferenceCycle { line: 2 }]
    );
}

/// `@string` definitions that refer to themselves are removed, with every redefinition, and an
/// entry using one is skipped; unused ones are reported too.
#[test]
fn string_cycles_removed() {
    let parsed = parse_hostile("@string{p = q}\n@string{q = p}\n@article{a, title = p}\n");
    assert_eq!(parsed.records, Vec::<Record>::new());
    let lines: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    assert_eq!(lines, vec![1, 2, 3]);

    let parsed = parse_hostile("@string{p = p}\n@string{p = {x}}\n@article{a, title = p}\n");
    assert_eq!(parsed.records, Vec::<Record>::new());
    assert!(matches!(
        &parsed.diagnostics[0],
        Diagnostic::SkippedEntry { line: 1, message }
            if message.starts_with("@string p refers to itself")
    ));

    let unused = diagnostics_of("@string{p = p}\n@article{a, title = {A}}\n");
    assert_eq!(unused.len(), 1);
}

/// A month `@string` that refers to itself is removed, and the month name is used instead.
#[test]
fn cyclic_month_string_falls_back_to_month() {
    let parsed = parse_hostile("@string{jan = jan}\n@article{a, title = {A}, month = jan}\n");
    assert_eq!(parsed.records[0].extras["bibtex:month"], vec!["January"]);
}

/// A chain of `@string`s longer than the limit, or one that doubles at each step, is removed from
/// the point where it goes too far; shorter chains are kept. Doubling is judged by the look-ups it
/// costs, not the text it produces.
#[test]
fn string_chains_bounded() {
    let mut chain = String::from("@string{s0 = {x}}\n");
    for i in 1..70 {
        writeln!(chain, "@string{{s{i} = s{}}}", i - 1).unwrap();
    }
    chain.push_str("@article{a, title = {A}, note = s63}\n@article{b, title = {B}, note = s64}\n");
    let parsed = parse_hostile(&chain);
    assert_eq!(titles(&parsed), vec![Some("A")]);
    assert_eq!(parsed.diagnostics[0].line(), 65);

    let mut doubling = String::from("@string{a0 = {x}}\n");
    for i in 1..26 {
        writeln!(doubling, "@string{{a{i} = a{p} # a{p}}}", p = i - 1).unwrap();
    }
    doubling
        .push_str("@article{a, title = {A}, note = a11}\n@article{b, title = {B}, note = a12}\n");
    let parsed = parse_hostile(&doubling);
    assert_eq!(titles(&parsed), vec![Some("A")]);
    // `a11` costs 4,095 look-ups, within the limit; `a12` costs 8,191.
    assert_eq!(parsed.diagnostics[0].line(), 13);
}

/// A `@string` cycle in a later part of the input is reported at its own lines when entries are
/// parsed one by one.
#[test]
fn string_cycle_lines_on_the_per_entry_path() {
    let input =
        "@article{x title = {X}}\n@article{a, title = {A}}\n\n@string{p = q}\n@string{q = p}\n";
    let lines: Vec<usize> = diagnostics_of(input).iter().map(Diagnostic::line).collect();
    assert_eq!(lines, vec![1, 4, 5]);
}

/// Entries for a random reference graph: distinct keys, optional `ids` aliases (which may clash
/// with each other but never with a key), and optional `crossref` and `xdata` targets naming keys
/// or aliases.
#[derive(Arbitrary, Debug)]
struct ReferenceGraph {
    /// For each entry: an alias, a `crossref` target and `xdata` targets, as indexes into
    /// [`REFERENCE_NAMES`].
    #[strategy(vec((
        proptest::option::of(0_usize..4),
        proptest::option::of(0_usize..12),
        vec(0_usize..12, 0..3),
    ), 1..=8))]
    entries: Vec<(Option<usize>, Option<usize>, Vec<usize>)>,
}

/// Names in a random reference graph: eight keys, then four aliases.
const REFERENCE_NAMES: [&str; 12] = [
    "k0", "k1", "k2", "k3", "k4", "k5", "k6", "k7", "x0", "x1", "x2", "x3",
];

/// Whether `start` can reach itself in the graph `edges`.
fn reaches_itself(start: usize, edges: &[Vec<usize>]) -> bool {
    let mut seen = vec![false; edges.len()];
    let mut stack: Vec<usize> = edges[start].clone();
    while let Some(node) = stack.pop() {
        if node == start {
            return true;
        }
        if !std::mem::replace(&mut seen[node], true) {
            stack.extend(&edges[node]);
        }
    }
    false
}

/// However entries refer to each other, parsing never aborts; no `crossref` cycle is left; an entry
/// on no cycle keeps its `crossref`; and each entry that lost a reference is reported once.
#[proptest]
fn reference_graphs_never_abort(input: ReferenceGraph) {
    let mut source = String::new();
    for (index, (alias, crossref, xdata)) in input.entries.iter().enumerate() {
        write!(source, "@misc{{k{index}, title = {{T}}").unwrap();
        if let Some(alias) = alias {
            write!(source, ", ids = {{{}}}", REFERENCE_NAMES[8 + alias]).unwrap();
        }
        if let Some(target) = crossref {
            write!(source, ", crossref = {{{}}}", REFERENCE_NAMES[*target]).unwrap();
        }
        if !xdata.is_empty() {
            let names: Vec<&str> = xdata.iter().map(|&t| REFERENCE_NAMES[t]).collect();
            write!(source, ", xdata = {{{}}}", names.join(", ")).unwrap();
        }
        source.push_str("}\n");
    }
    let parsed = parse_hostile(&source);
    prop_assert_eq!(parsed.records.len(), input.entries.len());

    // Resolve names as the parser does: keys first, then each alias, a later one overwriting.
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (index, (alias, _, _)) in input.entries.iter().enumerate() {
        by_name.insert(REFERENCE_NAMES[index], index);
        if let Some(alias) = alias {
            by_name.insert(REFERENCE_NAMES[8 + alias], index);
        }
    }
    let resolve = |name: &str| by_name.get(name).copied();
    let all_edges: Vec<Vec<usize>> = input
        .entries
        .iter()
        .map(|(_, crossref, xdata)| {
            crossref
                .iter()
                .chain(xdata)
                .filter_map(|&t| resolve(REFERENCE_NAMES[t]))
                .collect()
        })
        .collect();
    let kept_crossrefs: Vec<Vec<usize>> = parsed
        .records
        .iter()
        .map(|r| {
            first_extra(r, "bibtex:crossref")
                .and_then(resolve)
                .into_iter()
                .collect()
        })
        .collect();

    for index in 0..input.entries.len() {
        prop_assert!(!reaches_itself(index, &kept_crossrefs));
        if !reaches_itself(index, &all_edges) {
            prop_assert_eq!(
                first_extra(&parsed.records[index], "bibtex:crossref"),
                input.entries[index].1.map(|t| REFERENCE_NAMES[t])
            );
        }
    }
    let lost = input
        .entries
        .iter()
        .zip(&parsed.records)
        .filter(|((_, crossref, _), record)| {
            crossref.is_some() && first_extra(record, "bibtex:crossref").is_none()
        })
        .count();
    let reported = parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::ReferenceCycle { .. }))
        .count();
    prop_assert!(lost <= reported);
    let mut reported_lines = HashSet::new();
    for diagnostic in &parsed.diagnostics {
        if let Diagnostic::ReferenceCycle { line } = diagnostic {
            prop_assert!(reaches_itself(line - 1, &all_edges), "line {line}");
            prop_assert!(reported_lines.insert(*line), "line {line} reported twice");
        }
    }
}

/// Random `@string` definitions that use each other, with text, empty or month-name leaves and
/// redefinitions: parsing never aborts, and every name reported as referring to itself is on a
/// cycle.
#[proptest]
fn string_graphs_never_abort(
    #[strategy(vec(vec(0_usize..6, 0..3), 1..=6))] definitions: Vec<Vec<usize>>,
    #[strategy(proptest::sample::select(vec!["{x}", "{}", "\"\"", "jan"]))] leaf: &'static str,
    redefine: bool,
) {
    let mut source = String::new();
    for (index, uses) in definitions.iter().enumerate() {
        let mut parts: Vec<String> = uses.iter().map(|u| format!("s{u}")).collect();
        parts.push(leaf.to_owned());
        writeln!(source, "@string{{s{index} = {}}}", parts.join(" # ")).unwrap();
    }
    if redefine {
        // Later definitions are never used, so they must not change the outcome.
        source.push_str("@string{s0 = {later}}\n");
    }
    for index in 0..definitions.len() {
        writeln!(source, "@misc{{k{index}, title = {{T}}, note = s{index}}}").unwrap();
    }
    let parsed = parse_hostile(&source);

    let edges: Vec<Vec<usize>> = definitions
        .iter()
        .map(|uses| {
            uses.iter()
                .copied()
                .filter(|&u| u < definitions.len())
                .collect()
        })
        .collect();
    let cyclic = parsed.diagnostics.iter().filter_map(|d| match d {
        Diagnostic::SkippedEntry { message, .. } => message
            .strip_prefix("@string s")
            .and_then(|rest| rest.split_once(" refers to itself"))
            .map(|(index, _)| index.parse::<usize>().unwrap()),
        _ => None,
    });
    for index in cyclic {
        prop_assert!(reaches_itself(index, &edges), "s{index}");
    }
}

// -- Hostile input: resolution work --

/// A `@string` chain that doubles at each step: `a0 = leaf`, `a_i = a_{i-1} # a_{i-1}`.
fn doubling_strings(leaf: &str, levels: usize) -> String {
    let mut source = format!("@string{{a0 = {leaf}}}\n");
    for i in 1..=levels {
        writeln!(source, "@string{{a{i} = a{p} # a{p}}}", p = i - 1).unwrap();
    }
    source
}

/// A doubling chain of empty values is bounded by the look-ups it costs, even though it produces no
/// text. Regression test from code review: it used to run for minutes.
#[test]
fn empty_doubling_strings_bounded() {
    for leaf in ["{}", "\"\""] {
        let mut input = doubling_strings(leaf, 59);
        input.push_str(
            "@article{a, title = {A}, note = a59}\n@article{b, title = {B}, note = a11}\n",
        );
        let parsed = parse_hostile(&input);
        assert_eq!(titles(&parsed), vec![Some("B")], "{leaf}");
    }
}

/// The entries, by line, skipped for using `@string`s that expand too far in total.
fn over_budget_lines(parsed: &biblio::Parsed) -> Vec<usize> {
    parsed
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(d, Diagnostic::SkippedEntry { message, .. }
                if message == "uses @string definitions that expand too far in total")
        })
        .map(Diagnostic::line)
        .collect()
}

/// Uses of `@string`s draw on one budget for the whole input: once it runs out, every further
/// entry that uses them is skipped and reported, and the entries before are kept. Uses in `ids`,
/// `crossref` and `xdata` count twice.
#[test]
fn string_use_budget_shared_by_the_input() {
    // `a0` to `a11` take lines 1 to 12; the entries follow.
    let mut input = doubling_strings("{x}", 11);
    for i in 0..1_100 {
        writeln!(input, "@article{{e{i}, title = {{T}}, note = a11}}").unwrap();
    }
    let parsed = parse_hostile(&input);
    let kept = parsed.records.len();
    assert!(kept > 0 && kept < 1_100, "{kept}");
    let skipped: Vec<usize> = (13 + kept..13 + 1_100).collect();
    assert_eq!(over_budget_lines(&parsed), skipped);

    let mut twice = doubling_strings("{x}", 11);
    for i in 0..1_100 {
        writeln!(twice, "@article{{e{i}, title = {{T}}, ids = a11}}").unwrap();
    }
    assert_eq!(parse_hostile(&twice).records.len(), kept / 2);
}

/// The budget is shared across the input when entries are parsed one by one, here forced by a
/// broken entry, so splitting the input does not multiply it.
#[test]
fn string_use_budget_shared_when_parsing_entries_one_by_one() {
    let mut whole = doubling_strings("{x}", 11);
    for i in 0..1_100 {
        writeln!(whole, "@article{{e{i}, title = {{T}}, note = a11}}").unwrap();
    }
    let kept = parse_hostile(&whole).records.len();

    let mut input = doubling_strings("{x}", 11);
    input.push_str("@article{broken title = {X}}\n");
    for i in 0..1_100 {
        writeln!(input, "@article{{e{i}, title = {{T}}, note = a11}}").unwrap();
    }
    assert_eq!(parse_hostile(&input).records.len(), kept);
}

/// Reference fields are recognised in any case.
#[test]
fn reference_fields_in_any_case_checked() {
    for input in [
        "@article{a, title = {A}, CROSSREF = {a}}\n",
        "@xdata{a, title = {A}, XData = {a}}\n",
    ] {
        assert_eq!(
            diagnostics_of(input),
            vec![Diagnostic::ReferenceCycle { line: 1 }],
            "{input}"
        );
    }
}

/// An `xdata` that closes a cycle is removed in full, so its other keys go too.
#[test]
fn cyclic_xdata_removed_in_full() {
    let input = "\
@xdata{z, title = {Z}, publisher = {PubZ}}
@book{a, title = {A}, xdata = {z, a}}
";
    let parsed = parse_hostile(input);
    assert_eq!(first_extra(&parsed.records[1], "bibtex:publisher"), None);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::ReferenceCycle { line: 2 }]
    );
}

/// A cycle between two `@xdata` entries is broken, and an entry using them still parses.
#[test]
fn xdata_entries_on_a_cycle_broken() {
    let input = "\
@xdata{d, note = {N}, xdata = {e}}
@xdata{e, note = {M}, xdata = {d}}
@article{a, title = {A}, xdata = {d}}
";
    let parsed = parse_hostile(input);
    assert_eq!(parsed.records.len(), 3);
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::ReferenceCycle { .. }))
            .count(),
        1
    );
}

/// An `ids` alias equal to the entry's own key is a self-reference like any other.
#[test]
fn alias_equal_to_own_key_checked() {
    assert_eq!(
        diagnostics_of("@article{a, title = {A}, ids = {a}, crossref = {a}}\n"),
        vec![Diagnostic::ReferenceCycle { line: 1 }]
    );
}

/// A `crossref` given by a `@string` that was removed fails like any unknown abbreviation, and a
/// self-reference elsewhere in the input is still broken.
#[test]
fn crossref_through_removed_string_skipped() {
    let input = "\
@string{p = p}
@article{a, title = {A}, crossref = p}
@article{b, title = {B}, crossref = {b}}
";
    let parsed = parse_hostile(input);
    assert_eq!(titles(&parsed), vec![Some("B")]);
    assert!(
        parsed
            .diagnostics
            .contains(&Diagnostic::ReferenceCycle { line: 3 })
    );
}

/// Entries whose `ids` aliases may equal other entries' keys, which makes the whole input fail at
/// once and parse entry by entry, never abort the parse.
#[proptest]
fn reference_graphs_with_key_clashes_never_abort(
    #[strategy(vec((
        proptest::option::of(0_usize..12),
        proptest::option::of(0_usize..12),
        vec(0_usize..12, 0..3),
    ), 1..=8))]
    entries: Vec<(Option<usize>, Option<usize>, Vec<usize>)>,
) {
    let mut source = String::new();
    for (index, (alias, crossref, xdata)) in entries.iter().enumerate() {
        write!(source, "@misc{{k{index}, title = {{T}}").unwrap();
        if let Some(alias) = alias {
            write!(source, ", ids = {{{}}}", REFERENCE_NAMES[*alias]).unwrap();
        }
        if let Some(target) = crossref {
            write!(source, ", crossref = {{{}}}", REFERENCE_NAMES[*target]).unwrap();
        }
        if !xdata.is_empty() {
            let names: Vec<&str> = xdata.iter().map(|&t| REFERENCE_NAMES[t]).collect();
            write!(source, ", xdata = {{{}}}", names.join(", ")).unwrap();
        }
        source.push_str("}\n");
    }
    let parsed = parse_hostile(&source);
    prop_assert!(parsed.records.len() <= entries.len());
}

/// Looking up a `@string` among many definitions is charged by the definitions the input uses,
/// not all of them: many unused definitions neither slow parsing down nor make entries be
/// skipped. Regression test from code review: the parser scans its list of definitions for every
/// look-up.
#[test]
fn many_unused_strings_do_not_cost_look_ups() {
    let mut input = String::new();
    for i in 0..10_000 {
        writeln!(input, "@string{{f{i} = {{x}}}}").unwrap();
    }
    input.push_str(&doubling_strings("{x}", 6));
    for i in 0..32 {
        let uses = vec!["a6"; 32].join(" # ");
        writeln!(input, "@article{{e{i}, title = {{T}}, note = {uses}}}").unwrap();
    }
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 32);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}

/// Entries parsed one by one each get only the `@string` definitions they use, so many
/// definitions and many entries together stay fast.
#[test]
fn many_strings_and_entries_parse_quickly_one_by_one() {
    let mut input = String::new();
    for i in 0..5_000 {
        writeln!(input, "@string{{f{i} = {{x}}}}").unwrap();
    }
    input.push_str("@article{broken title}\n");
    for i in 0..5_000 {
        writeln!(
            input,
            "@article{{e{i}, title = {{T}}, crossref = {{e{i}}}}}"
        )
        .unwrap();
    }
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 5_000);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}

/// A use of a month `@string` that was removed for referring to itself still costs a look-up,
/// because the month name stands in for it, so a chain of such uses is bounded. Regression test
/// from code review: the uses were counted as free and the input ran without limit.
#[test]
fn uses_of_removed_month_strings_cost_look_ups() {
    let mut input = String::from("@string{jan = jan}\n");
    writeln!(input, "@string{{v0 = {}}}", vec!["jan"; 2_000].join(" # ")).unwrap();
    for i in 1..=11 {
        writeln!(input, "@string{{v{i} = v{p} # v{p}}}", p = i - 1).unwrap();
    }
    for i in 0..10 {
        writeln!(input, "@article{{e{i}, title = {{T}}, note = v11}}").unwrap();
    }
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records, Vec::<Record>::new());
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}

/// A `@string` whose name is longer than 64 bytes is dropped, because comparing long names makes
/// every look-up slow; shorter names are kept.
#[test]
fn long_string_names_dropped() {
    let long = "n".repeat(65);
    let short = "n".repeat(64);
    let input = format!(
        "@string{{{long} = {{x}}}}\n@string{{{short} = {{y}}}}\n{}",
        format_args!("@misc{{a, title = {{A}}, note = {short}}}\n")
    );
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records[0].extras["bibtex:note"], vec!["y"]);
    assert!(matches!(
        parsed.diagnostics.as_slice(),
        [Diagnostic::SkippedEntry { line: 1, message }]
            if message == "@string with a name longer than 64 bytes"
    ));
}

// -- Hostile input: reference chains --

/// A `crossref` chain of `links` links: `e0` is the parent, and each `e{i}` refers to `e{i-1}`.
fn crossref_chain(links: usize) -> String {
    let mut source = String::from("@book{e0, title = {T}, publisher = {P}}\n");
    for i in 1..=links {
        writeln!(
            source,
            "@book{{e{i}, title = {{T}}, crossref = {{e{}}}}}",
            i - 1
        )
        .unwrap();
    }
    source
}

/// The references reported as going too deep or copying too much, by line.
fn too_deep_lines(parsed: &biblio::Parsed) -> Vec<usize> {
    parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::ReferenceTooDeep { .. }))
        .map(Diagnostic::line)
        .collect()
}

/// A chain of 64 references resolves in full; at 65, only the head loses its reference, and every
/// entry is kept.
#[test]
fn reference_chain_depth_bounded() {
    let parsed = parse_hostile(&crossref_chain(64));
    assert_eq!(parsed.records.len(), 65);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
    assert_eq!(
        first_extra(&parsed.records[64], "bibtex:publisher"),
        Some("P")
    );

    let parsed = parse_hostile(&crossref_chain(65));
    assert_eq!(parsed.records.len(), 66);
    assert_eq!(too_deep_lines(&parsed), vec![66]);
    assert_eq!(first_extra(&parsed.records[65], "bibtex:crossref"), None);
}

/// A very long chain no longer overflows the stack: it is cut where it goes too deep.
#[test]
fn very_long_reference_chain_cut() {
    let parsed = parse_hostile(&crossref_chain(20_000));
    assert_eq!(parsed.records.len(), 20_001);
    assert_ne!(too_deep_lines(&parsed), Vec::<usize>::new());
}

/// An `xdata` that doubles at each level is cut where the copies it would make go over a limit,
/// and the count starts again above the cut, so the cuts come at regular intervals.
#[test]
fn doubling_xdata_cut() {
    let mut input = String::from("@xdata{x0, title = {X}, note = {N}}\n");
    for i in 1..=31 {
        writeln!(
            input,
            "@xdata{{x{i}, title = {{X}}, xdata = {{x{p}, x{p}}}}}",
            p = i - 1
        )
        .unwrap();
    }
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 32);
    // The copies double at each level, so the limit is reached every few levels, and the count
    // starts again above each cut.
    let cut = too_deep_lines(&parsed);
    assert!(cut.len() >= 2, "{cut:?}");
    assert!(
        cut.windows(2).all(|pair| pair[1] - pair[0] == cut[0] - 1),
        "{cut:?}"
    );
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}

/// An entry listing one parent 4,096 times is resolved; 4,097 times is too many copies. The parent
/// has no fields, so only the number of copies counts.
#[test]
fn xdata_repeats_bounded() {
    for (repeats, cut) in [(4_096, false), (4_097, true)] {
        let input = format!(
            "@xdata{{leaf,}}\n@misc{{k, title = {{K}}, xdata = {{{}}}}}\n",
            vec!["leaf"; repeats].join(", ")
        );
        let parsed = parse_hostile(&input);
        assert_eq!(
            too_deep_lines(&parsed),
            if cut { vec![2] } else { vec![] },
            "{repeats}"
        );
    }
}

/// A parent whose `xdata` lists many keys that name no entry is read again each time it is copied,
/// so the keys count towards each referrer's limit. Regression test from code review: they were
/// counted as free, and a 1 MB parent copied 1,000 times took seconds.
#[test]
fn copies_of_long_key_lists_bounded() {
    let keys: Vec<String> = (0..100_000).map(|i| format!("missing{i}")).collect();
    let mut input = format!("@xdata{{leaf, xdata = {{{}}}}}\n", keys.join(","));
    for i in 0..1_000 {
        writeln!(input, "@misc{{k{i}, title = {{K}}, xdata = {{leaf}}}}").unwrap();
    }
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 1_001);
    assert_eq!(too_deep_lines(&parsed), (2..=1_001).collect::<Vec<_>>());
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );

    // A few missing keys cost little.
    let parsed = parse_hostile(
        "@xdata{leaf, note = {N}, xdata = {a, b, c}}\n@misc{k, title = {K}, xdata = {leaf}}\n",
    );
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 1 }]
    );

    // Keys listed in a `@string` count the same. Regression test from code review: their commas
    // were not counted, and this took seconds.
    let mut input = format!(
        "@string{{keys = {{{}}}}}\n@xdata{{leaf, xdata = keys}}\n",
        vec!["q"; 100_000].join(",")
    );
    for i in 0..1_000 {
        writeln!(input, "@misc{{k{i}, title = {{K}}, xdata = {{leaf}}}}").unwrap();
    }
    let start = std::time::Instant::now();
    let parsed = parse_hostile(&input);
    assert_eq!(parsed.records.len(), 1_001);
    assert_eq!(too_deep_lines(&parsed), (3..=1_002).collect::<Vec<_>>());
    assert!(
        start.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );

    // A key given by a `@string` counts once, like any other.
    let parsed = parse_hostile(
        "@string{lf = {leaf}}\n@xdata{leaf, note = {N}}\n@misc{k, title = {K}, xdata = lf}\n",
    );
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::MissingTitle { line: 2 }]
    );
}

/// An entry that copies a parent made of many empty fields is bounded by the fields' cost, not
/// just their text. Regression test from code review: such copies were counted as free and could
/// take gigabytes.
#[test]
fn copies_of_wide_entries_bounded() {
    let mut leaf = String::from("@xdata{leaf");
    for i in 0..2_000 {
        write!(leaf, ", f{i:040} = {{}}").unwrap();
    }
    leaf.push_str("}\n");
    let input = format!(
        "{leaf}@misc{{k, title = {{K}}, xdata = {{{}}}}}\n",
        vec!["leaf"; 4_096].join(", ")
    );
    assert_eq!(too_deep_lines(&parse_hostile(&input)), vec![2]);
}

/// An entry that would copy too many bytes loses its references, even with few copies, counting
/// the text its parents' `@string`s expand to.
#[test]
fn copied_bytes_count_string_expansions() {
    let input = format!(
        "@string{{big = {{{}}}}}\n@xdata{{leaf, title = {{L}}, note = big}}\n{}",
        "x".repeat(65_536),
        format_args!(
            "@misc{{k, title = {{K}}, xdata = {{{}}}}}\n",
            vec!["leaf"; 300].join(", ")
        )
    );
    assert_eq!(too_deep_lines(&parse_hostile(&input)), vec![3]);
}

/// An entry that would copy too many bytes loses its references, even with few copies.
#[test]
fn copied_bytes_bounded() {
    let input = format!(
        "@xdata{{leaf, title = {{L}}, note = {{{}}}}}\n@misc{{k, title = {{K}}, xdata = {{{}}}}}\n",
        "x".repeat(65_536),
        vec!["leaf"; 300].join(", ")
    );
    assert_eq!(too_deep_lines(&parse_hostile(&input)), vec![2]);
}

/// Entries draw on one budget for resolving references: once it runs out, later entries lose
/// theirs, in order, and earlier ones keep them.
#[test]
fn reference_budget_shared_by_the_input() {
    let leaves: Vec<String> = (0..4_000).map(|i| format!("l{i}")).collect();
    let mut input = String::new();
    // Leaves without fields, so that the number of copies is what runs out.
    for leaf in &leaves {
        writeln!(input, "@xdata{{{leaf},}}").unwrap();
    }
    writeln!(
        input,
        "@xdata{{hub, title = {{H}}, xdata = {{{}}}}}",
        leaves.join(", ")
    )
    .unwrap();
    for i in 0..100 {
        writeln!(input, "@misc{{k{i}, title = {{T}}, xdata = {{hub}}}}").unwrap();
    }
    let parsed = parse_hostile(&input);
    // The hub copies 4,000 entries and each referrer 4,001, so the budget of 262,144 covers the
    // hub and 64 referrers.
    let first_referrer = 4_002;
    let cut: Vec<usize> = (first_referrer + 64..first_referrer + 100).collect();
    assert_eq!(too_deep_lines(&parsed), cut);
}

/// A reference graph with no cycle, in any file order: each entry refers to entries earlier in a
/// hidden order, through its `crossref` (the first target) and `xdata` (the rest), with repeats and
/// keys that name no entry.
#[derive(Arbitrary, Debug)]
struct AcyclicGraph {
    /// For each entry in the hidden order, the entries it refers to, as offsets back from it; an
    /// offset that reaches past the start names no entry.
    #[strategy(vec(vec(1_usize..5, 0..4), 1..24))]
    back: Vec<Vec<usize>>,
    /// The file position of each entry in the hidden order.
    #[strategy(Just((0..#back.len()).collect::<Vec<usize>>()).prop_shuffle())]
    position: Vec<usize>,
    /// The length of each entry's note, so that the limit on copied bytes can be reached.
    #[strategy(vec(proptest::sample::select(vec![0_usize, 1, 64, 4_096]), #back.len()))]
    note: Vec<usize>,
}

/// The copies resolving entry `index` makes, and the longest chain below it, counted the way the
/// parser resolves: each target is copied and resolved in turn, nothing remembered. Counting stops
/// once the copies go over `cap`, so that a graph the parser would never resolve is not walked in
/// full either.
fn resolution(index: usize, targets: &[Vec<usize>], cap: u64) -> (u64, usize) {
    let mut copies = 0;
    let mut deepest = 0;
    for &target in &targets[index] {
        let (below, depth) = resolution(target, targets, cap.saturating_sub(copies + 1));
        copies += 1 + below;
        deepest = deepest.max(depth + 1);
        if copies > cap {
            break;
        }
    }
    (copies, deepest)
}

/// However entries branch and are ordered, parsing never aborts or hangs and keeps every entry.
/// Every entry that keeps its references stays within the limits on copies and depth, counted
/// independently by following the parser's recursion, and an entry far below every limit keeps
/// them.
#[proptest]
fn acyclic_reference_graphs_bounded(input: AcyclicGraph) {
    let count = input.back.len();
    // `targets` is indexed by file position.
    let mut targets: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut source_lines: Vec<String> = vec![String::new(); count];
    for (hidden, back) in input.back.iter().enumerate() {
        let file = input.position[hidden];
        let names: Vec<String> = back
            .iter()
            .map(|&b| match hidden.checked_sub(b) {
                Some(target) => {
                    targets[file].push(input.position[target]);
                    format!("x{}", input.position[target])
                }
                None => "missing".to_owned(),
            })
            .collect();
        let mut line = format!(
            "@xdata{{x{file}, note = {{{}}}",
            "n".repeat(input.note[hidden])
        );
        if let Some((first, rest)) = names.split_first() {
            write!(line, ", crossref = {{{first}}}").unwrap();
            if !rest.is_empty() {
                write!(line, ", xdata = {{{}}}", rest.join(", ")).unwrap();
            }
        }
        line.push('}');
        source_lines[file] = line;
    }
    let source = source_lines.join("\n") + "\n";
    let parsed = parse_hostile(&source);
    prop_assert_eq!(parsed.records.len(), count);

    let cut: HashSet<usize> = too_deep_lines(&parsed)
        .into_iter()
        .map(|line| line - 1)
        .collect();
    let mut kept_targets = targets.clone();
    for &entry in &cut {
        kept_targets[entry].clear();
    }
    for entry in 0..count {
        if cut.contains(&entry) {
            continue;
        }
        let (copies, depth) = resolution(entry, &kept_targets, 4_096);
        prop_assert!(
            copies <= 4_096 && depth <= 64,
            "entry {entry}: {copies} copies, depth {depth}"
        );
        let (all_copies, _) = resolution(entry, &targets, 16);
        if all_copies <= 16 && input.note.iter().all(|&n| n <= 64) {
            prop_assert!(!cut.contains(&entry));
        }
    }
    for &entry in &cut {
        prop_assert!(
            !targets[entry].is_empty(),
            "entry {entry} cut without references"
        );
    }
}

// -- Hostile input: dates the parser cannot read --

/// The year of each record's date, in order.
fn years(parsed: &biblio::Parsed) -> Vec<Option<i32>> {
    parsed
        .records
        .iter()
        .map(|record| record.date.map(|date| date.year))
        .collect()
}

/// A `date` the underlying parser would panic on is read as free text: one written in digits that
/// are not ASCII, or, with unknown digits (`X`), a year of more than four digits or a month `00`.
/// The entry is kept, the `year` gives its date, and nothing is reported.
#[test]
fn dates_the_parser_panics_on_read_as_text() {
    let dates = [
        "١٢",
        "1٢3",
        "-١٢",
        "٣",
        "²³",
        "2020-٣",
        "2020-²",
        "2020-01-٣",
        "2020-01-01T٣:00",
        "2020-01-01T12:٣",
        "2020-01-01T12:00:٣",
        "2020/١٢",
        "١٢/2020",
        "١٢?",
        "2020-٣/2021",
        "20200X",
        "2020-00-XX",
        "2020-00-xx~",
    ];
    for date in dates {
        let input = format!("@article{{k, title = {{T}}, date = {{{date}}}, year = {{1999}}}}\n");
        let parsed = parse_hostile(&input);
        assert_eq!(years(&parsed), vec![Some(1999)], "date = {{{date}}}");
        assert_eq!(
            parsed.diagnostics,
            Vec::<Diagnostic>::new(),
            "date = {{{date}}}"
        );
    }
}

/// Free text that starts with a year still gives that year when digits that are not ASCII follow.
#[test]
fn year_before_digits_that_are_not_ascii_read() {
    let parsed = parse_hostile("@article{k, title = {T}, date = {2020 (١٤٤١ هـ)}}\n");
    assert_eq!(years(&parsed), vec![Some(2020)]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// A `crossref` child of a type that needs a date reads its parent's date while resolving. A
/// parent `date` the parser would panic on is removed and reported, and the parent's `year` then
/// gives both their dates.
#[test]
fn parent_date_the_parser_panics_on_dropped() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, date = {١٢}, year = {2019}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n",
    );
    assert_eq!(years(&parsed), vec![Some(2019), Some(2019)]);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::DroppedField {
            line: 1,
            field: "date".to_owned(),
        }]
    );
}

/// The same holds when entries are parsed one by one, here forced by a broken entry and with the
/// parent and child on one line.
#[test]
fn parent_date_the_parser_panics_on_dropped_one_by_one() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, date = {١٢}} \
         @inproceedings{c, title = {C}, crossref = {p}}\n\
         @article{x title = {X}}\n",
    );
    assert_eq!(titles(&parsed), vec![Some("P"), Some("C")]);
    assert_eq!(
        parsed.diagnostics[0],
        Diagnostic::DroppedField {
            line: 1,
            field: "date".to_owned(),
        }
    );
    assert_eq!(parsed.diagnostics.len(), 2);
    assert_eq!(parsed.diagnostics[1].line(), 2);
}

/// Without a `date`, the child reads its parent's `year` and `month`. A `year` with a space after
/// its sign, which cannot be read anyway, is removed without a report; a `month` followed by a day
/// number too large is removed and reported, so it is not in the parent's extras either.
#[test]
fn parent_year_and_month_the_parser_panics_on_dropped() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, year = {- 2020}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n",
    );
    assert_eq!(years(&parsed), vec![None, None]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());

    for month in ["June 1000", "-300", "jan-256"] {
        let parsed = parse_hostile(&format!(
            "@proceedings{{p, title = {{P}}, year = {{2020}}, month = {{{month}}}}}\n\
             @inproceedings{{c, title = {{C}}, crossref = {{p}}}}\n"
        ));
        assert_eq!(years(&parsed), vec![Some(2020), Some(2020)], "{month}");
        assert_eq!(first_extra(&parsed.records[0], "bibtex:month"), None);
        assert_eq!(
            parsed.diagnostics,
            vec![Diagnostic::DroppedField {
                line: 1,
                field: "month".to_owned(),
            }],
            "{month}"
        );
    }
}

/// A parent that keeps its `date` keeps its `month` too, because the parser then never reads the
/// month; and an entry nobody refers to keeps it as well.
#[test]
fn month_kept_where_the_parser_does_not_read_it() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, date = {2019}, month = {June 1000}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n\
         @article{a, title = {A}, year = {2020}, month = {June 1000}}\n",
    );
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
    assert_eq!(
        first_extra(&parsed.records[0], "bibtex:month"),
        Some("June 1000")
    );
    assert_eq!(
        first_extra(&parsed.records[2], "bibtex:month"),
        Some("June 1000")
    );
}

/// A parent's `date` that starts with a year, followed by digits that are not ASCII, does not make
/// the parser fail, so it is kept, and parent and child both get its year. Regression test from
/// code review: any digit that is not ASCII removed it.
#[test]
fn parent_date_with_year_before_other_digits_kept() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, date = {2020 (١٤٤١ هـ)}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n",
    );
    assert_eq!(years(&parsed), vec![Some(2020), Some(2020)]);
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
}

/// An entry referred to only by entries of types that have no date, such as `misc`, keeps its
/// date fields, because resolving reads none of them. Regression test from code review: every
/// target lost them.
#[test]
fn fields_of_targets_whose_date_nobody_reads_kept() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, date = {١٢}, year = {2019}}\n\
         @misc{c, title = {C}, crossref = {p}}\n\
         @proceedings{q, title = {Q}, year = {2020}, month = {June 1000}}\n\
         @misc{d, title = {D}, crossref = {q}}\n",
    );
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
    assert_eq!(years(&parsed)[0], Some(2019));
    assert_eq!(
        first_extra(&parsed.records[2], "bibtex:month"),
        Some("June 1000")
    );
}

/// An entry of a type that has no date copies the fields of the entries it refers to, so their
/// dates are read where its own is: here a `misc` between the parent and a child that reads it.
#[test]
fn date_copied_through_an_entry_without_a_date_dropped() {
    let parsed = parse_hostile(
        "@proceedings{p, title = {P}, date = {١٢}, year = {2019}}\n\
         @misc{m, title = {M}, crossref = {p}}\n\
         @inproceedings{c, title = {C}, crossref = {m}}\n",
    );
    assert_eq!(parsed.records.len(), 3);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::DroppedField {
            line: 1,
            field: "date".to_owned(),
        }]
    );
}

/// A date whose day is unknown, in a month of fewer than 31 days, is written by the parser as a
/// range ending on day 31 when an entry inherits it. The child cannot read that range, so it has
/// no date, and the parent keeps its own.
#[test]
fn inherited_unknown_day_in_a_short_month_unread() {
    let parsed = parse_hostile(
        "@proceedings{g, title = {G}, date = {2021-04-XX}}\n\
         @inproceedings{p, title = {P}, crossref = {g}}\n",
    );
    assert_eq!(parsed.diagnostics, Vec::<Diagnostic>::new());
    assert_eq!(
        parsed.records[0].date,
        Some(PublicationDate {
            year: 2021,
            month: Some(4),
            day: Some(1),
        })
    );
    assert_eq!(parsed.records[1].date, None);
}

/// When a further entry reads that inherited date again while resolving, the parser would panic,
/// so the original `date` is removed and reported, through `crossref` and through `xdata` alike.
#[test]
fn unknown_day_in_a_short_month_read_again_dropped() {
    let chains = [
        "@proceedings{g, title = {G}, date = {2021-02-XX}}\n\
         @inproceedings{p, title = {P}, crossref = {g}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n",
        "@xdata{g, date = {2021-11-XX~}}\n\
         @inproceedings{p, title = {P}, xdata = {g}}\n\
         @inproceedings{c, title = {C}, crossref = {p}}\n",
    ];
    for input in chains {
        let parsed = parse_hostile(input);
        assert_eq!(parsed.records.len(), 3, "{input}");
        let dropped: Vec<&Diagnostic> = parsed
            .diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::DroppedField { .. }))
            .collect();
        assert_eq!(
            dropped,
            vec![&Diagnostic::DroppedField {
                line: 1,
                field: "date".to_owned(),
            }],
            "{input}"
        );
    }
}

std::thread_local! {
    /// How many panics this thread has started, counted by the hook [`count_panics`] installs.
    static PANICS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Install, once for the process, a panic hook that counts each thread's panics before passing
/// them on, so a test can tell whether a panic happened even where it was caught.
fn count_panics() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICS.with(|panics| panics.set(panics.get() + 1));
            previous(info);
        }));
    });
}

/// Where the generated text goes.
#[derive(Debug, Arbitrary)]
enum DatePlacement {
    /// An entry's own `date`.
    Own,
    /// The `date` of the first entry in a chain of references.
    Date,
    /// The `year` of the first entry in a chain of references.
    Year,
    /// The `month` of the first entry in a chain of references, which has a `year` too.
    Month,
}

/// Text shaped like a date, a year or a month, reaching every way the parser may fail on one.
fn date_like_text() -> impl Strategy<Value = String> {
    let part = || {
        prop_oneof![
            "[0-9]{1,6}",
            "[0-9]{2,4}[Xx]{1,3}",
            "[+-] ?[0-9]{1,4}",
            "00|XX|xx",
            "[0-9]?[١٢٣²³]{1,2}",
        ]
    };
    prop_oneof![
        (
            part(),
            proptest::option::of(part()),
            proptest::option::of(part()),
            proptest::option::of("T[0-9٣]{1,2}(:[0-9]{2})?"),
            proptest::option::of(part()),
            "[?~%]?",
        )
            .prop_map(|(year, month, day, time, end, mark)| {
                let mut text = year;
                for part in [month, day].into_iter().flatten() {
                    write!(text, "-{part}").unwrap();
                }
                text.push_str(time.as_deref().unwrap_or(""));
                if let Some(end) = end {
                    write!(text, "/{end}").unwrap();
                }
                text + &mark
            }),
        "[0-9١٢٣²³ⅫXx+/.~?%TZ: \u{a0}-]{0,16}",
        "[A-Za-z]{0,5}[ \u{a0}-]{0,2}[0-9]{1,6}",
        "[0-9]{4}-(0[0-9]|1[0-3])-[Xx]{2}[?~%]?",
    ]
}

/// The type of an entry in a chain of references.
#[derive(Debug, Arbitrary)]
enum LinkKind {
    /// A type that has a date, so resolving reads the dates of the entries referred to.
    InProceedings,
    /// A type without a date, which copies the fields of the entries referred to.
    Misc,
    /// An `xdata` entry, which copies every field of the entries referred to.
    XData,
}

/// An entry that refers to the one before it.
#[derive(Debug, Arbitrary)]
struct Link {
    /// The entry's type.
    kind: LinkKind,
    /// Whether it refers through `xdata`, rather than `crossref`.
    xdata: bool,
}

/// A date-like value in an entry, or at the start of a chain of references.
#[derive(Debug, Arbitrary)]
struct DateInput {
    /// The value.
    #[strategy(date_like_text())]
    text: String,
    /// Where it goes.
    placement: DatePlacement,
    /// The entries that refer, one to the next, to the entry holding the value.
    #[strategy(vec(any::<Link>(), 1..=3))]
    links: Vec<Link>,
    /// Whether the entry holding the value is an `xdata` entry, rather than `proceedings`.
    xdata: bool,
    /// Whether a broken entry forces the entries to be parsed one by one.
    one_by_one: bool,
}

/// However a date, year or month is written, and wherever it is read from, parsing never panics,
/// not even where the panic would be caught.
#[proptest(cases = 512)]
fn date_like_values_never_panic(input: DateInput) {
    count_panics();
    let text = &input.text;
    let mut source = match input.placement {
        DatePlacement::Own => format!("@article{{e0, title = {{T}}, date = {{{text}}}}}\n"),
        DatePlacement::Date | DatePlacement::Year | DatePlacement::Month => {
            let field = match input.placement {
                DatePlacement::Date => format!("date = {{{text}}}"),
                DatePlacement::Year => format!("year = {{{text}}}"),
                _ => format!("year = {{2020}}, month = {{{text}}}"),
            };
            let kind = if input.xdata { "xdata" } else { "proceedings" };
            let mut source = format!("@{kind}{{e0, title = {{T}}, {field}}}\n");
            for (i, link) in (1..).zip(&input.links) {
                let kind = match link.kind {
                    LinkKind::InProceedings => "inproceedings",
                    LinkKind::Misc => "misc",
                    LinkKind::XData => "xdata",
                };
                let reference = if link.xdata { "xdata" } else { "crossref" };
                writeln!(
                    source,
                    "@{kind}{{e{i}, title = {{T}}, {reference} = {{e{}}}}}",
                    i - 1
                )
                .unwrap();
            }
            source
        }
    };
    if input.one_by_one {
        source.push_str("@article{x title = {X}}\n");
    }
    let before = PANICS.with(std::cell::Cell::get);
    let parsed = parse(&source);
    prop_assert!(parsed.is_ok());
    prop_assert_eq!(PANICS.with(std::cell::Cell::get), before, "{}", source);
}
