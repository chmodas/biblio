mod common;

use std::collections::HashMap;

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;
use test_strategy::{Arbitrary, proptest};

use biblio::csv::{Layout, detect_layout, parse, parse_as, serialize, serialize_as};
use biblio::{Diagnostic, Error, PublicationDate, Record};

use common::{IdentifierValues, arb_date, arb_identifier_values, arb_url, first_extra};

/// The single record of `input`, read with its layout detected.
fn only_record(input: &str) -> Record {
    let mut records = parse(input).unwrap().records;
    assert_eq!(records.len(), 1, "{records:?}");
    records.remove(0)
}

/// A date with the year only.
fn year(year: i32) -> PublicationDate {
    PublicationDate {
        year,
        month: None,
        day: None,
    }
}

// -- Generic layout --

/// Columns named after the record's fields are read into them; other columns are kept in extras.
#[test]
fn generic_columns_read() {
    let input = "\
author,title,year,journal,abstract,doi,number,pages,volume,isbn,record_id,label,source
\"Doe, Jane; Roe, Rick\",A title,2008,Curr Opin Lipidol,An abstract,10.1/x,3,221-8,19,0957-9672,1,In_database,unique
";
    let record = only_record(input);
    assert_eq!(record.authors, vec!["Doe, Jane", "Roe, Rick"]);
    assert_eq!(record.title.as_deref(), Some("A title"));
    assert_eq!(record.date, Some(year(2008)));
    assert_eq!(record.journal.as_deref(), Some("Curr Opin Lipidol"));
    assert_eq!(record.abstract_text.as_deref(), Some("An abstract"));
    assert_eq!(record.doi.as_deref(), Some("10.1/x"));
    assert_eq!(record.number.as_deref(), Some("3"));
    assert_eq!(record.pages.as_deref(), Some("221-8"));
    assert_eq!(record.volume.as_deref(), Some("19"));
    assert_eq!(record.isbn.as_deref(), Some("0957-9672"));
    assert_eq!(record.label.as_deref(), Some("In_database"));
    assert_eq!(record.database.as_deref(), Some("unique"));
    assert_eq!(first_extra(&record, "csv:record_id"), Some("1"));
    assert_eq!(record.extras.len(), 1);
}

/// Generic column names are matched in any case and without surrounding whitespace.
#[test]
fn generic_headers_in_any_case() {
    let record = only_record(" TITLE ,Author,Abstract_Text\nT,\"Doe, J\",An abstract\n");
    assert_eq!(record.title.as_deref(), Some("T"));
    assert_eq!(record.authors, vec!["Doe, J"]);
    assert_eq!(record.abstract_text.as_deref(), Some("An abstract"));
}

/// Authors, keywords and links are split on `;`, trimmed, and blank parts dropped.
#[test]
fn generic_lists_split_on_semicolons() {
    let input = "title,authors,keywords,urls\nT,\"Doe, J;  Roe, R ;\",cats;;dogs,https://a.org; https://b.org\n";
    let record = only_record(input);
    assert_eq!(record.authors, vec!["Doe, J", "Roe, R"]);
    assert_eq!(record.keywords, vec!["cats", "dogs"]);
    assert_eq!(record.urls, vec!["https://a.org", "https://b.org"]);
}

/// The date column is preferred over the year column, which is then not kept; a year that cannot
/// be read gives no date.
#[test]
fn generic_date_preferred_over_year() {
    let record = only_record("title,year,date\nT,2020,2019-05-03\n");
    assert_eq!(
        record.date,
        Some(PublicationDate {
            year: 2019,
            month: Some(5),
            day: Some(3),
        })
    );
    assert!(record.extras.is_empty(), "{:?}", record.extras);

    assert_eq!(only_record("title,year\nT,in press\n").date, None);
    assert_eq!(
        only_record("title,year,date\nT,2020,May\n").date,
        Some(year(2020))
    );
}

/// R writes a missing value as `NA`, which the generic layout reads as blank.
#[test]
fn generic_na_is_blank() {
    let record = only_record("title,doi,pages,author\nT,NA, NA ,NA\n");
    assert_eq!(record.doi, None);
    assert_eq!(record.pages, None);
    assert_eq!(record.authors, Vec::<String>::new());
    assert!(record.extras.is_empty());
    // Only the exact text is blank.
    assert_eq!(only_record("title,doi\nT,na\n").doi.as_deref(), Some("na"));
}

/// Unknown, blank-named and duplicate columns are kept in extras under their header as written.
#[test]
fn unread_and_duplicate_columns_kept_in_extras() {
    let input = "title,Notes,,doi,DOI,author,authors\nT,a note,unnamed,10.1/a,10.1/b,\"Doe, J\",\"Roe, R\"\n";
    let record = only_record(input);
    assert_eq!(record.doi.as_deref(), Some("10.1/a"));
    assert_eq!(record.authors, vec!["Doe, J"]);
    assert_eq!(first_extra(&record, "csv:Notes"), Some("a note"));
    assert_eq!(first_extra(&record, "csv:"), Some("unnamed"));
    assert_eq!(first_extra(&record, "csv:DOI"), Some("10.1/b"));
    assert_eq!(first_extra(&record, "csv:authors"), Some("Roe, R"));
}

/// A blank column does not hide a later alias with a value.
#[test]
fn blank_column_does_not_hide_alias() {
    let record = only_record("title,author,authors\nT,,\"Doe, J\"\n");
    assert_eq!(record.authors, vec!["Doe, J"]);
}

/// Quoted cells may hold commas, doubled quotes and line breaks; a quote inside an unquoted cell is
/// read as written.
#[test]
fn quoted_cells_read() {
    let input =
        "title,abstract,pages\r\n\"A, \"\"quoted\"\" title\",\"Line one\r\nline two\",5\" disk\r\n";
    let record = only_record(input);
    assert_eq!(record.title.as_deref(), Some("A, \"quoted\" title"));
    assert_eq!(
        record.abstract_text.as_deref(),
        Some("Line one\r\nline two")
    );
    assert_eq!(record.pages.as_deref(), Some("5\" disk"));
}

// -- Zotero layout --

/// A header like Zotero's export.
const ZOTERO_HEADER: &str = "\"Key\",\"Item Type\",\"Publication Year\",\"Author\",\"Title\",\"Publication Title\",\"ISBN\",\"ISSN\",\"DOI\",\"Url\",\"Abstract Note\",\"Date\",\"Pages\",\"Issue\",\"Volume\",\"Manual Tags\",\"Automatic Tags\",\"Extra\"";

/// Zotero's columns are read into their fields, and its other columns kept in extras.
#[test]
fn zotero_columns_read() {
    let input = format!(
        "\u{feff}{ZOTERO_HEADER}\n\"ACSRAG7N\",\"journalArticle\",\"2022\",\"Rotter, Carina; Rusbridge, Clare\",\"Myoclonus\",\"J Vet Intern Med\",\"\",\"1939-1676 0891-6640\",\"10.1111/jvim.16404\",\"https://example.org\",\"An abstract\",\"2022-05\",\"1032-1038\",\"3\",\"36\",\"Animals; *Dog Diseases/genetics\",\"auto\",\"PMID: 35319117\"\n"
    );
    assert_eq!(detect_layout(&input), Layout::Zotero);
    let record = only_record(&input);
    assert_eq!(record.authors, vec!["Rotter, Carina", "Rusbridge, Clare"]);
    assert_eq!(record.title.as_deref(), Some("Myoclonus"));
    assert_eq!(record.journal.as_deref(), Some("J Vet Intern Med"));
    assert_eq!(record.isbn.as_deref(), Some("0891-6640"));
    assert_eq!(record.extras["csv:ISSN"], ["1939-1676"]);
    assert_eq!(record.doi.as_deref(), Some("10.1111/jvim.16404"));
    assert_eq!(record.urls, vec!["https://example.org"]);
    assert_eq!(record.abstract_text.as_deref(), Some("An abstract"));
    assert_eq!(
        record.date,
        Some(PublicationDate {
            year: 2022,
            month: Some(5),
            day: None,
        })
    );
    assert_eq!(record.pages.as_deref(), Some("1032-1038"));
    assert_eq!(record.number.as_deref(), Some("3"));
    assert_eq!(record.volume.as_deref(), Some("36"));
    assert_eq!(record.keywords, vec!["Animals", "*Dog Diseases/genetics"]);
    assert_eq!(first_extra(&record, "csv:Key"), Some("ACSRAG7N"));
    assert_eq!(
        first_extra(&record, "csv:Item Type"),
        Some("journalArticle")
    );
    assert_eq!(first_extra(&record, "csv:Automatic Tags"), Some("auto"));
    assert_eq!(first_extra(&record, "csv:Extra"), Some("PMID: 35319117"));
    assert_eq!(record.extras.len(), 5);
}

/// Zotero's `ISBN` is preferred over `ISSN`, which is then kept in extras.
#[test]
fn zotero_isbn_preferred_over_issn() {
    let input = "Key,Item Type,Publication Year,Title,ISBN,ISSN\nK,book,2020,T,978-3-16-148410-0,1234-5678\n";
    let record = only_record(input);
    assert_eq!(record.isbn.as_deref(), Some("978-3-16-148410-0"));
    assert_eq!(first_extra(&record, "csv:ISSN"), Some("1234-5678"));
}

/// A day of `00` is left out, and a `Date` that is not ISO falls back to `Publication Year`.
#[test]
fn zotero_dates() {
    let header = "Key,Item Type,Publication Year,Title,Date";
    let date_of = |row: &str| only_record(&format!("{header}\n{row}\n")).date;
    assert_eq!(
        date_of("K,book,2019,T,2019-05-00"),
        Some(PublicationDate {
            year: 2019,
            month: Some(5),
            day: None,
        })
    );
    assert_eq!(date_of("K,book,2019,T,Spring 2019"), Some(year(2019)));
}

/// Zotero writes an identifier made of ISSNs to `ISSN`, any other to `ISBN`, and a second
/// identifier kept in extras to the other column.
#[test]
fn zotero_identifier_columns_written() {
    let identifier_cells = |record: Record| {
        let output = serialize_as(Layout::Zotero, &[record]);
        let row = output.lines().nth(1).unwrap().to_owned();
        let cells: Vec<String> = row.split(',').map(str::to_owned).collect();
        (cells[6].clone(), cells[7].clone())
    };
    let issns = Record::builder().isbn("1234-5678 2345-678X").build();
    assert_eq!(
        identifier_cells(issns),
        (String::new(), "1234-5678 2345-678X".to_owned())
    );
    let isbn10 = Record::builder().isbn("012345678X").build();
    assert_eq!(
        identifier_cells(isbn10),
        ("012345678X".to_owned(), String::new())
    );
    let both = Record::builder()
        .isbn("978-3-16-148410-0")
        .extra("csv:ISSN", "1234-5678")
        .build();
    assert_eq!(
        identifier_cells(both.clone()),
        ("978-3-16-148410-0".to_owned(), "1234-5678".to_owned())
    );
    let output = serialize_as(Layout::Zotero, std::slice::from_ref(&both));
    assert_eq!(parse(&output).unwrap().records, vec![both]);
}

/// A record read from Zotero and written in the generic layout is read back as generic.
#[test]
fn zotero_record_written_as_generic_is_generic() {
    let input = "Key,Item Type,Publication Year,Title,Date\nK,book,2019,T,2019-05\n";
    let records = parse(input).unwrap().records;
    let generic = serialize(&records);
    assert_eq!(detect_layout(&generic), Layout::Generic);
    assert_eq!(parse(&generic).unwrap().records, records);
}

// -- SyRF layout --

/// A file in the `SyRF` import layout is read, and written back the same.
#[test]
fn syrf_read_and_written() {
    let input = "\"Title\",\"Authors\",\"PublicationName\",\"AlternateName\",\"Abstract\",\"Url\",\"AuthorAddress\",\"Year\",\"DOI\",\"ReferenceType\",\"Keywords\",\"PdfRelativePath\",\"CustomId\"\r\n\"Title 1\",\"Author. 1; Author. 2\",\"Journal 1\",\"\",\"Abstract 1\",\"https://a.org\",\"\",\"2020\",\"DOI 1\",\"\",\"cats; dogs\",\"1.pdf\",\"1\"\r\n";
    assert_eq!(detect_layout(input), Layout::Syrf);
    let records = parse(input).unwrap().records;
    let record = &records[0];
    assert_eq!(record.title.as_deref(), Some("Title 1"));
    assert_eq!(record.authors, vec!["Author. 1", "Author. 2"]);
    assert_eq!(record.journal.as_deref(), Some("Journal 1"));
    assert_eq!(record.abstract_text.as_deref(), Some("Abstract 1"));
    assert_eq!(record.urls, vec!["https://a.org"]);
    assert_eq!(record.date, Some(year(2020)));
    assert_eq!(record.doi.as_deref(), Some("DOI 1"));
    assert_eq!(record.keywords, vec!["cats", "dogs"]);
    assert_eq!(first_extra(record, "csv:PdfRelativePath"), Some("1.pdf"));
    assert_eq!(first_extra(record, "csv:CustomId"), Some("1"));

    let output = serialize_as(Layout::Syrf, &records);
    assert!(output.starts_with("Title,Authors,PublicationName,AlternateName,Abstract,Url,AuthorAddress,Year,DOI,ReferenceType,Keywords,PdfRelativePath,CustomId\r\n"));
    assert_eq!(parse(&output).unwrap().records, records);
}

// -- Detection --

/// A header with only some of Zotero's signature columns is generic.
#[test]
fn partial_signatures_are_generic() {
    assert_eq!(detect_layout("Key,Item Type,Title\n"), Layout::Generic);
    assert_eq!(detect_layout("Title,PublicationName\n"), Layout::Generic);
    assert_eq!(detect_layout(""), Layout::Generic);
    assert_eq!(detect_layout("\"open"), Layout::Generic);
}

// -- Delimiter --

/// A file separated by semicolons is read; a quoted cell holding `;` stays one cell, then splits as
/// a list where its column is a list.
#[test]
fn semicolon_separated_file_read() {
    let input = "title;author;doi\r\nA, title;\"Doe, J; Roe, R\";10.1/x\r\n";
    let record = only_record(input);
    assert_eq!(record.title.as_deref(), Some("A, title"));
    assert_eq!(record.authors, vec!["Doe, J", "Roe, R"]);
    assert_eq!(record.doi.as_deref(), Some("10.1/x"));

    let zotero = "Key;Item Type;Publication Year;Title\nK;book;2020;T\n";
    assert_eq!(detect_layout(zotero), Layout::Zotero);
    assert_eq!(only_record(zotero).date, Some(year(2020)));
}

/// Semicolons inside quotes do not count towards the delimiter.
#[test]
fn quoted_semicolons_do_not_change_delimiter() {
    let record = only_record("title,\"a;b;c\"\nT,x\n");
    assert_eq!(record.title.as_deref(), Some("T"));
    assert_eq!(first_extra(&record, "csv:a;b;c"), Some("x"));
}

// -- Lines and leniency --

/// Diagnostics report the line where the row starts, counting quoted line breaks and skipped blank
/// lines, with either line ending.
#[test]
fn diagnostics_at_row_start_lines() {
    for newline in ["\n", "\r\n"] {
        let input = [
            "title,abstract",
            "T,\"two",
            "lines\"",
            "",
            ",no title",
            "T,a,surplus",
            "T,a,,  ",
        ]
        .join(newline)
            + newline;
        let parsed = parse(&input).unwrap();
        assert_eq!(parsed.records.len(), 4);
        assert_eq!(
            parsed.diagnostics,
            vec![
                Diagnostic::MissingTitle { line: 5 },
                Diagnostic::ExtraCells { line: 6, count: 1 },
            ],
            "{newline:?}"
        );
    }
}

/// Rows whose cells are all blank are skipped, and a short row reads the missing cells as blank.
#[test]
fn blank_rows_skipped_and_short_rows_padded() {
    let parsed = parse("title,doi,pages\nT\n,,\n , , \nU,10.1/x\n").unwrap();
    let titles: Vec<_> = parsed.records.iter().map(|r| r.title.as_deref()).collect();
    assert_eq!(titles, vec![Some("T"), Some("U")]);
    assert_eq!(parsed.records[0].doi, None);
    assert_eq!(parsed.diagnostics, Vec::new());
}

/// A row before an open last row is still reported at its own line. Regression test: counting the
/// open row's line first moved the earlier rows' lines to it.
#[test]
fn rows_before_open_quote_keep_their_lines() {
    let parsed = parse("title,abstract\n,untitled\nB,\"open\n").unwrap();
    assert_eq!(parsed.diagnostics[0], Diagnostic::MissingTitle { line: 2 });
    assert_eq!(parsed.diagnostics[1].line(), 3);
}

/// A quoted cell left open in the last row drops that row, keeps the others, and is reported at
/// the row's line.
#[test]
fn open_quote_in_last_row_truncates() {
    let parsed = parse("title,abstract\nA,one\nB,\"never\nclosed\n").unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(
        parsed.diagnostics,
        vec![Diagnostic::Truncated {
            line: 3,
            message: "a quoted cell is not closed before the end of the input".into(),
        }]
    );
}

/// A quoted cell left open before any record is complete is an error.
#[test]
fn open_quote_before_any_record_fails() {
    for (input, line) in [
        ("title,abstract\nA,\"never closed\n", 2),
        ("title,\"abstract\nA,b\n", 1),
        ("title\n,,\n\"open", 3),
    ] {
        assert_eq!(
            parse(input),
            Err(Error::MalformedSyntax {
                line,
                message: "a quoted cell is not closed before the end of the input".into(),
            }),
            "{input:?}"
        );
    }
}

/// Empty input is an error, a header alone gives no records, and a byte-order mark is ignored.
#[test]
fn empty_header_only_and_byte_order_mark() {
    assert_eq!(parse(""), Err(Error::EmptyInput));
    assert_eq!(parse("\u{feff} \r\n"), Err(Error::EmptyInput));
    assert_eq!(
        parse("title,doi\r\n").unwrap().records,
        Vec::<Record>::new()
    );
    let record = only_record("\u{feff}title\nT\n");
    assert_eq!(record.title.as_deref(), Some("T"));
}

// -- Writing --

/// The layout's columns come first, then the extras of every record, sorted; an extra naming one of
/// the layout's columns, and extras of other formats, are not written.
#[test]
fn serialize_columns_then_sorted_extras() {
    let a = Record::builder()
        .title("A")
        .extra("csv:zeta", "z")
        .extra("csv:Title", "would be read as the title")
        .extra("ris:N1", "other format")
        .build();
    let b = Record::builder().title("B").extra("csv:alpha", "a").build();
    let output = serialize(&[a, b]);
    assert_eq!(
        output,
        "title,author,year,date,journal,doi,pages,volume,number,abstract,isbn,source,label,keywords,url,alpha,zeta\r\n\
         A,,,,,,,,,,,,,,,,z\r\n\
         B,,,,,,,,,,,,,,,a,\r\n"
    );
}

/// Lists are joined with `; `, skipping blank values, and cells that need it are quoted.
#[test]
fn serialize_lists_and_quoting() {
    let record = Record::builder()
        .title("A, \"quoted\"\ntitle")
        .authors(["Doe, J", " ", "Roe, R"])
        .urls(["https://a.org", ""])
        .build();
    let output = serialize(&[record]);
    let row = output.split_once("\r\n").unwrap().1;
    assert!(
        row.starts_with("\"A, \"\"quoted\"\"\ntitle\",\"Doe, J; Roe, R\","),
        "{row}"
    );
    assert!(row.ends_with(",https://a.org\r\n"), "{row}");
}

/// With no records, only the header row is written, which reads back as no records.
#[test]
fn serialize_no_records_writes_header() {
    for layout in [Layout::Generic, Layout::Zotero, Layout::Syrf] {
        let output = serialize_as(layout, &[]);
        assert_eq!(output.lines().count(), 1);
        assert_eq!(detect_layout(&output), layout);
        assert_eq!(parse(&output).unwrap().records, Vec::<Record>::new());
    }
}

/// Zotero and `SyRF` hold one link, so only the first non-blank one is written.
#[test]
fn single_link_layouts_write_first_link() {
    let record = Record::builder()
        .title("T")
        .urls([" ", "https://a.org", "https://b.org"])
        .build();
    for layout in [Layout::Zotero, Layout::Syrf] {
        let output = serialize_as(layout, std::slice::from_ref(&record));
        assert!(output.contains("https://a.org"));
        assert!(!output.contains("https://b.org"));
    }
}

// -- Properties --

/// A list cell built from pieces, some blank, separated by `;` with or without spaces, gives its
/// trimmed non-blank pieces in order.
#[proptest]
fn list_cells_match_reference(
    #[strategy(vec(("[A-Za-z ]{0,8}", select(vec![";", "; ", " ;", " ; "])), 1..6))] pieces: Vec<(
        String,
        &'static str,
    )>,
) {
    let cell: String = pieces
        .iter()
        .flat_map(|(piece, separator)| [piece.as_str(), separator])
        .collect();
    let record = only_record(&format!("title,keywords\nT,{cell}\n"));
    let reference: Vec<&str> = pieces
        .iter()
        .map(|(piece, _)| piece.trim())
        .filter(|piece| !piece.is_empty())
        .collect();
    prop_assert_eq!(record.keywords, reference);
}

/// Text that every layout keeps as written: trimmed, not blank, not `NA`, and possibly holding
/// commas, quotes, semicolons, line breaks and characters beyond ASCII.
fn arb_cell() -> impl Strategy<Value = String> {
    "[A-Za-z0-9é\"',.;()-]([A-Za-z0-9é\"',.;() -]|\n|\r\n){0,30}[A-Za-z0-9é\"',.;()-]|[A-Za-z0-9é]"
        .prop_filter("NA is read as blank", |cell| cell != "NA")
}

/// One ISSN or ISBN, as a record holds it once read: a single identifier without labels.
fn arb_identifier() -> impl Strategy<Value = String> {
    prop_oneof![
        "[0-9]{4}-[0-9]{3}[0-9X]",
        "97[89]-[0-9]{1,2}-[0-9]{3,4}-[0-9]{3,4}-[0-9X]",
    ]
}

/// One value of a list cell: as [`arb_cell`], but without `;` or line breaks.
fn arb_list_value() -> impl Strategy<Value = String> {
    "[A-Za-z0-9é\"',.()-]([A-Za-z0-9é\"',.() -]{0,20}[A-Za-z0-9é\"',.()-])?"
        .prop_filter("NA is read as blank", |value| value != "NA")
}

/// Extras whose names no layout reads: one value each.
fn arb_csv_extras() -> impl Strategy<Value = HashMap<String, Vec<String>>> {
    // Names start with `x`, so no layout reads them and they complete no layout's signature, and
    // may hold the characters that need quoting.
    let name = "x[a-zA-Z ;,\"]{0,8}[a-z]";
    proptest::collection::hash_map(name, arb_cell(), 0..3).prop_map(|extras| {
        extras
            .into_iter()
            .map(|(name, value)| (format!("csv:{name}"), vec![value]))
            .collect()
    })
}

/// A record holding only what `layout` has columns for.
fn arb_csv_record(layout: Layout) -> impl Strategy<Value = Record> {
    let scalar = || option::of(arb_cell());
    let lists = (
        vec(arb_list_value(), 0..3),
        vec(arb_list_value(), 0..3),
        vec(arb_url(), 0..3),
    );
    let pass_through = vec(
        (
            select(vec![
                "Key",
                "Item Type",
                "AlternateName",
                "AuthorAddress",
                "ReferenceType",
                "PdfRelativePath",
                "CustomId",
            ]),
            arb_cell(),
        ),
        0..3,
    );
    (
        (scalar(), scalar(), scalar(), scalar()),
        (scalar(), scalar(), scalar(), option::of(arb_identifier())),
        (scalar(), scalar()),
        option::of(arb_date()),
        lists,
        arb_csv_extras(),
        pass_through,
    )
        .prop_map(
            move |(
                (title, journal, doi, abstract_text),
                (pages, volume, number, isbn),
                (database, label),
                date,
                (authors, keywords, urls),
                mut extras,
                pass_through,
            )| {
                let mut record = Record::default();
                record.title = title;
                record.journal = journal;
                record.doi = doi;
                record.abstract_text = abstract_text;
                record.authors = authors;
                record.keywords = keywords;
                record.date = date;
                record.urls = urls;
                if layout != Layout::Syrf {
                    record.pages = pages;
                    record.volume = volume;
                    record.number = number;
                    record.isbn = isbn;
                }
                if layout == Layout::Generic {
                    record.database = database;
                    record.label = label;
                } else {
                    record.urls.truncate(1);
                    // Pass-through columns of the layout only, so they never complete another
                    // layout's signature.
                    let own: &[&str] = if layout == Layout::Zotero {
                        &["Key", "Item Type"]
                    } else {
                        &[
                            "AlternateName",
                            "AuthorAddress",
                            "ReferenceType",
                            "PdfRelativePath",
                            "CustomId",
                        ]
                    };
                    for (name, value) in pass_through {
                        if own.contains(&name) {
                            extras.insert(format!("csv:{name}"), vec![value]);
                        }
                    }
                }
                if layout == Layout::Syrf {
                    record.date = record.date.map(|date| year(date.year));
                }
                record.extras = extras;
                record
            },
        )
        // A record with no values is written as a blank row, which reading skips.
        .prop_filter("a record needs a value", |record| {
            *record != Record::default()
        })
}

/// A layout and records holding only what it has columns for.
#[derive(Arbitrary, Debug)]
struct LayoutRecords {
    /// The layout to write and read.
    #[strategy(select(vec![Layout::Generic, Layout::Zotero, Layout::Syrf]))]
    layout: Layout,
    /// The records, holding only what `layout` has columns for.
    #[strategy(vec(arb_csv_record(#layout), 0..4))]
    records: Vec<Record>,
}

/// Records written in a layout read back the same in that layout, and the layout is detected.
#[proptest]
fn round_trip_in_every_layout(input: LayoutRecords) {
    let output = serialize_as(input.layout, &input.records);
    prop_assert_eq!(detect_layout(&output), input.layout);
    let parsed = parse_as(input.layout, &output).unwrap();
    prop_assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|d| !matches!(d, Diagnostic::MissingTitle { .. }))
            .count(),
        0
    );
    prop_assert_eq!(parsed.records, input.records);
}

/// Any year, and any valid month and day, survive the generic and Zotero layouts.
#[proptest]
fn dates_round_trip(
    #[strategy(select(vec![Layout::Generic, Layout::Zotero]))] layout: Layout,
    #[strategy(any::<i32>())] year: i32,
    #[strategy(option::of((1_u8..=12, option::of(1_u8..=31))))] month_day: Option<(u8, Option<u8>)>,
) {
    let date = PublicationDate {
        year,
        month: month_day.map(|(month, _)| month),
        day: month_day.and_then(|(_, day)| day),
    };
    let record = Record::builder().title("T").date(date).build();
    let output = serialize_as(layout, std::slice::from_ref(&record));
    prop_assert_eq!(parse_as(layout, &output).unwrap().records, vec![record]);
}

/// Rewrite comma-separated `input` with `;` between cells, quoting cells that hold `;`, `"` or a
/// line break.
fn with_semicolons(input: &str) -> String {
    let mut reader = ::csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(input.as_bytes());
    let mut out = String::new();
    for row in reader.records() {
        let row = row.unwrap();
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                if cell.contains([';', '"', '\r', '\n']) {
                    format!("\"{}\"", cell.replace('"', "\"\""))
                } else {
                    cell.to_owned()
                }
            })
            .collect();
        out.push_str(&cells.join(";"));
        out.push_str("\r\n");
    }
    out
}

/// A file separated by semicolons reads the same as the same file separated by commas.
#[proptest]
fn semicolon_files_read_the_same(
    #[strategy(vec(arb_csv_record(Layout::Generic), 0..4))] records: Vec<Record>,
) {
    let output = serialize(&records);
    prop_assert_eq!(parse(&with_semicolons(&output)).unwrap().records, records);
}

/// One row of a generated file: its cells, as written.
#[derive(Arbitrary, Debug)]
struct GeneratedRow {
    /// The cells, which may be blank, `NA`, lists, or hold quotes and line breaks.
    #[strategy(vec(select(vec![
        "", " ", "NA", "T", "a, b", "x; y", "Doe, J; Roe, R", "line\nbreak", "\"q\"", "5\" disk",
        "2020", "2019-05-03", "10.1/x",
    ]), 0..7))]
    cells: Vec<&'static str>,
}

/// A generated CSV file: a header from the names of every layout and others, then rows.
#[derive(Arbitrary, Debug)]
struct GeneratedFile {
    /// The column names, from every layout and others, possibly repeated or blank.
    #[strategy(vec(select(vec![
        "title", "Title", "author", "Authors", "year", "date", "doi", "DOI", "keywords",
        "Manual Tags", "Key", "Item Type", "Publication Year", "PublicationName", "ISBN", "ISSN",
        "url", "Url", "label", "source", "other", "x\"y", "",
    ]), 1..8))]
    header: Vec<&'static str>,
    /// The rows after the header.
    rows: Vec<GeneratedRow>,
    /// The line break: `\n`, `\r\n` or a lone `\r`.
    #[strategy(select(vec!["\n", "\r\n", "\r"]))]
    newline: &'static str,
    /// Blank lines before the header, which the reader skips.
    #[strategy(0_usize..3)]
    leading_blank_lines: usize,
    /// Whether cells are separated by `;` rather than `,`, when the header has two or more columns.
    semicolons: bool,
}

impl GeneratedFile {
    /// The file's text, and the line where each row starts.
    fn text(&self) -> (String, Vec<usize>) {
        // A header of one column holds no delimiter, so it is read as separated by commas.
        let delimiter = if self.semicolons && self.header.len() > 1 {
            ";"
        } else {
            ","
        };
        let newline = self.newline;
        // A quote inside an unquoted cell is read as it is, so only cells that need quoting get
        // quotes.
        let quote = |cell: &str| {
            if cell.contains([',', ';', '\n']) || cell.starts_with('"') || cell.trim().is_empty() {
                format!("\"{}\"", cell.replace('"', "\"\"").replace('\n', newline))
            } else {
                cell.to_owned()
            }
        };
        let mut text = newline.repeat(self.leading_blank_lines);
        let mut starts = Vec::new();
        let mut line = 1 + self.leading_blank_lines;
        for cells in std::iter::once(&self.header).chain(self.rows.iter().map(|row| &row.cells)) {
            starts.push(line);
            let row = cells
                .iter()
                .map(|cell| quote(cell))
                .collect::<Vec<_>>()
                .join(delimiter);
            line += row.matches(newline).count() + 1;
            text.push_str(&row);
            text.push_str(newline);
        }
        (text, starts)
    }
}

/// However a file is built, every non-blank row gives one record, and the diagnostics point at the
/// lines where their rows start.
#[proptest]
fn records_and_lines_match_rows(file: GeneratedFile) {
    let (text, starts) = file.text();
    let Ok(parsed) = parse(&text) else {
        // Only a file that is blank throughout is rejected.
        prop_assert!(text.trim().is_empty() || file.header.iter().all(|name| name.is_empty()));
        return Ok(());
    };
    let layout = detect_layout(&text);
    let blank =
        |cell: &str| cell.trim().is_empty() || (layout == Layout::Generic && cell.trim() == "NA");
    let kept: Vec<(&GeneratedRow, usize)> = file
        .rows
        .iter()
        .zip(starts[1..].iter().copied())
        .filter(|(row, _)| !row.cells.iter().all(|cell| blank(cell)))
        .collect();
    prop_assert_eq!(parsed.records.len(), kept.len());

    let width = file.header.len();
    let expected_extra_cells: Vec<Diagnostic> = kept
        .iter()
        .filter(|(row, _)| row.cells.iter().skip(width).any(|cell| !blank(cell)))
        .map(|&(row, line)| Diagnostic::ExtraCells {
            line,
            count: row.cells.len() - width,
        })
        .collect();
    let extra_cells: Vec<Diagnostic> = parsed
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::ExtraCells { .. }))
        .cloned()
        .collect();
    prop_assert_eq!(extra_cells, expected_extra_cells);
    for diagnostic in &parsed.diagnostics {
        prop_assert!(
            kept.iter().any(|&(_, line)| line == diagnostic.line()),
            "{diagnostic}"
        );
    }
}

/// Reading a file, then writing and reading it in its layout, settles after one pass.
#[proptest]
fn write_and_read_is_idempotent(file: GeneratedFile) {
    let (text, _) = file.text();
    let Ok(parsed) = parse(&text) else {
        return Ok(());
    };
    let layout = detect_layout(&text);
    let pass = |records: &[Record]| {
        parse_as(layout, &serialize_as(layout, records))
            .unwrap()
            .records
    };
    let once = pass(&parsed.records);
    prop_assert_eq!(pass(&once), once);
}

/// Valid rows followed by a row that opens a quoted cell and never closes it keep every earlier
/// record, or fail when there are none.
#[proptest]
fn open_last_row_truncates(
    #[strategy(vec(arb_csv_record(Layout::Generic), 0..3))] records: Vec<Record>,
    #[strategy("[a-z ,\n]{0,10}")] tail: String,
) {
    let mut input = serialize(&records);
    let line = input.matches('\n').count() + 1;
    input.push_str("x,\"");
    input.push_str(&tail);
    let message = "a quoted cell is not closed before the end of the input".to_owned();
    match parse(&input) {
        Ok(parsed) => {
            prop_assert_eq!(parsed.records, records);
            let truncated = Diagnostic::Truncated { line, message };
            prop_assert_eq!(parsed.diagnostics.last(), Some(&truncated));
        }
        Err(error) => {
            prop_assert!(records.is_empty());
            prop_assert_eq!(error, Error::MalformedSyntax { line, message });
        }
    }
}

/// Parsing arbitrary text never panics.
#[proptest]
fn parse_never_panics(#[strategy("(\\PC|[\"\n\r,;]){0,300}")] input: String) {
    let _ = detect_layout(&input);
    let _ = parse(&input);
}

// -- Regression tests --

/// A column name holding semicolons is quoted when written, so the file is not read back as
/// separated by semicolons. Regression test: such a name used to be written as it is.
#[test]
fn column_name_with_semicolons_round_trips() {
    let record = Record::builder()
        .title("T")
        .extra("csv:a;b;c;d;e;f;g;h;i;j;k;l;m;n;o;p;q;r;s", "x")
        .build();
    let output = serialize(std::slice::from_ref(&record));
    assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Writing Zotero keeps an identifier read beside the generic record's own: an `ISSN` column is
/// Zotero's own, so it is written there, and the other identifiers of the generic `isbn` columns
/// are written in an `isbn` column of their own. Reading the output gives the same identifier.
/// Regression test: the second one used to be dropped.
#[test]
fn zotero_keeps_second_identifier() {
    for (input, expected, column, other) in [
        (
            "title,isbn,ISSN\nT,0957-9672,1234-5678\n",
            "0957-9672",
            "csv:ISSN",
            "1234-5678",
        ),
        (
            "title,isbn,ISBN\nT,978-3-16-148410-0,0-306-40615-2\n",
            "0-306-40615-2",
            "csv:isbn",
            "978-3-16-148410-0",
        ),
    ] {
        let records = parse(input).unwrap().records;
        let zotero = serialize_as(Layout::Zotero, &records);
        let back = parse(&zotero).unwrap().records;
        assert_eq!(back[0].isbn.as_deref(), Some(expected), "{zotero}");
        assert_eq!(back[0].extras[column], [other], "{zotero}");
    }
}

/// Date and year text that cannot be read as a date is kept in extras, and written back in its
/// column. Regression test: it used to be dropped.
#[test]
fn unreadable_dates_kept_and_written_back() {
    let generic = only_record("title,year,date\nT,in press,n.d.\n");
    assert_eq!(generic.date, None);
    assert_eq!(first_extra(&generic, "csv:year"), Some("in press"));
    assert_eq!(first_extra(&generic, "csv:date"), Some("n.d."));
    let output = serialize(std::slice::from_ref(&generic));
    assert_eq!(parse(&output).unwrap().records, vec![generic]);

    let zotero_input = "Key,Item Type,Publication Year,Title,Date\nK,book,,T,May 2019\n";
    let zotero = only_record(zotero_input);
    assert_eq!(first_extra(&zotero, "csv:Date"), Some("May 2019"));
    let output = serialize_as(Layout::Zotero, std::slice::from_ref(&zotero));
    assert_eq!(parse(&output).unwrap().records, vec![zotero]);
}

/// Surplus cells of exactly `NA` are blank in the generic layout, so they are not reported there,
/// but are in Zotero, which keeps `NA` as written.
#[test]
fn na_surplus_cells_reported_only_outside_generic() {
    assert_eq!(
        parse("title,doi\nT,x,NA\n").unwrap().diagnostics,
        Vec::new()
    );
    let zotero = parse("Key,Item Type,Publication Year,Title\nK,book,2020,T,NA\n").unwrap();
    assert_eq!(
        zotero.diagnostics,
        vec![Diagnostic::ExtraCells { line: 2, count: 1 }]
    );
}

/// Rows ending with a lone `\r`, as old Mac programs write, are counted as lines.
#[test]
fn lone_carriage_returns_count_as_lines() {
    let parsed = parse("title,abstract\r,a\rT,b\r,\"two\rlines\"\r,c\r").unwrap();
    let lines: Vec<usize> = parsed.diagnostics.iter().map(Diagnostic::line).collect();
    assert_eq!(lines, vec![2, 4, 6]);
}

/// A time after the day does not hide the day.
#[test]
fn day_followed_by_time_read() {
    let record = only_record("title,date\nT,2019-05-15T10:00\n");
    assert_eq!(
        record.date,
        Some(PublicationDate {
            year: 2019,
            month: Some(5),
            day: Some(15),
        })
    );
}

/// A quote inside an unquoted header cell is text, as the reader treats it, so it does not hide
/// the delimiters after it.
#[test]
fn quote_inside_header_cell_does_not_hide_delimiters() {
    let record = only_record("ti\"tle;a;b;doi\nT;1;2;10.1/x\n");
    assert_eq!(record.doi.as_deref(), Some("10.1/x"));
}

/// Blank lines before the header are skipped when detecting the delimiter, as the reader skips
/// them. Regression test: the delimiter used to be detected from the first, blank, line.
#[test]
fn blank_lines_before_semicolon_header_skipped() {
    let record = only_record("\n\r\ntitle;doi\nA, title;10.1/x\n");
    assert_eq!(record.title.as_deref(), Some("A, title"));
    assert_eq!(record.doi.as_deref(), Some("10.1/x"));
}

/// A date whose year has too many digits for a year is not read; the text is kept in extras.
#[test]
fn year_out_of_range_gives_no_date() {
    for date in ["99999999999999999999999", "2147483648-05-01", "-2147483649"] {
        let record = only_record(&format!("title,date\nT,{date}\n"));
        assert_eq!(record.date, None, "{date}");
        assert_eq!(first_extra(&record, "csv:date"), Some(date));
    }
}

/// An ISSN whose check digit is a lower-case `x` is written to Zotero's `ISSN` column.
#[test]
fn zotero_lower_case_issn_check_digit() {
    let record = Record::builder().isbn("2345-678x").build();
    let output = serialize_as(Layout::Zotero, &[record]);
    let row = output.lines().nth(1).unwrap();
    assert_eq!(row.split(',').nth(7), Some("2345-678x"));
}

// -- Identifiers --

/// Identifiers in the generic `isbn` column, separated by `;`, give the lowest; the others are
/// kept in its extra, and written back in the same cell, joined with `; `.
#[test]
fn generic_identifiers_split_and_written_back() {
    let record = only_record("title,isbn\nT,\"1939-1676 (Electronic); 0891-6640 (Print)\"\n");
    assert_eq!(record.isbn.as_deref(), Some("0891-6640"));
    assert_eq!(record.extras["csv:isbn"], ["1939-1676 (Electronic)"]);
    let output = serialize(std::slice::from_ref(&record));
    assert!(output.contains("\"0891-6640; 1939-1676 (Electronic)\""));
    assert_eq!(parse(&output).unwrap().records, [record]);
}

/// An ISSN-shaped identifier read from Zotero's `ISBN` stays in `ISBN` when other identifiers are
/// kept for that column, so reading the output chooses it again.
#[test]
fn zotero_issn_shaped_isbn_stays_in_its_column() {
    let header = "Key,Item Type,Publication Year,Title,ISBN,ISSN";
    let record = only_record(&format!(
        "{header}\nK,book,2020,T,0891-6640 978-3-16-148410-0,1939-1676\n"
    ));
    assert_eq!(record.isbn.as_deref(), Some("0891-6640"));
    let output = serialize_as(Layout::Zotero, std::slice::from_ref(&record));
    assert!(output.contains(",\"0891-6640; 978-3-16-148410-0\",1939-1676,"));
    assert_eq!(parse(&output).unwrap().records, [record]);
}

/// An identifier read from Zotero's `ISBN` stays there when `ISSN` lists a lower one, so reading
/// the output does not choose the lower ISSN instead.
#[test]
fn zotero_isbn_stays_when_issn_is_lower() {
    let header = "Key,Item Type,Publication Year,Title,ISBN,ISSN";
    let record = only_record(&format!("{header}\nK,book,2020,T,1939-1676,0891-6640\n"));
    assert_eq!(record.isbn.as_deref(), Some("1939-1676"));
    let output = serialize_as(Layout::Zotero, std::slice::from_ref(&record));
    assert!(output.contains(",1939-1676,0891-6640,"));
    assert_eq!(parse(&output).unwrap().records, [record]);
}

/// Identifiers in the generic `isbn` column give the lowest, and writing and reading the record
/// again gives the same record.
#[proptest]
fn generic_identifiers_round_trip(#[strategy(arb_identifier_values())] listed: IdentifierValues) {
    let cell = listed.values.join("; ");
    let record = only_record(&format!("title,isbn\nT,\"{cell}\"\n"));
    prop_assert_eq!(record.isbn.as_deref(), Some(listed.lowest()));
    let output = serialize(std::slice::from_ref(&record));
    prop_assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Identifiers in Zotero's `ISBN`, `ISSN` or both give the lowest of `ISBN`, or of `ISSN` when
/// `ISBN` is empty, and writing and reading the record again gives the same record.
#[proptest]
fn zotero_identifiers_round_trip(
    #[strategy(proptest::option::of(arb_identifier_values()))] isbns: Option<IdentifierValues>,
    #[strategy(proptest::option::of(arb_identifier_values()))] issns: Option<IdentifierValues>,
) {
    let cell = |listed: &Option<IdentifierValues>| {
        listed
            .as_ref()
            .map_or(String::new(), |listed| listed.values.join(" "))
    };
    let record = only_record(&format!(
        "Key,Item Type,Publication Year,Title,ISBN,ISSN\nK,book,2020,T,\"{}\",\"{}\"\n",
        cell(&isbns),
        cell(&issns)
    ));
    let expected = isbns
        .as_ref()
        .or(issns.as_ref())
        .map(IdentifierValues::lowest);
    prop_assert_eq!(record.isbn.as_deref(), expected);
    let output = serialize_as(Layout::Zotero, std::slice::from_ref(&record));
    prop_assert_eq!(parse(&output).unwrap().records, vec![record]);
}

/// Identifiers kept for Zotero's `ISSN` are all written when the record is written in another
/// layout, where `ISSN` is a column of its own, joined with `; `. Regression test: only the first
/// was written.
#[test]
fn identifier_extras_all_written_in_other_layouts() {
    let header = "Key,Item Type,Publication Year,Title,ISBN,ISSN";
    let record = only_record(&format!(
        "{header}\nK,book,2020,T,978-3-16-148410-0,2222-2222 1111-1111\n"
    ));
    assert_eq!(record.extras["csv:ISSN"], ["2222-2222", "1111-1111"]);
    let generic = serialize_as(Layout::Generic, std::slice::from_ref(&record));
    assert!(generic.contains("\"2222-2222; 1111-1111\""), "{generic}");
    let syrf = serialize_as(Layout::Syrf, std::slice::from_ref(&record));
    assert!(syrf.contains("\"2222-2222; 1111-1111\""), "{syrf}");
}

/// Two identifier columns are read as one list: the lowest of both is chosen, and the record
/// comes back the same when written and read again. Regression test: the choice was made from the
/// first column only, and changed once both were written in one cell.
#[test]
fn duplicate_identifier_columns_round_trip() {
    for (layout, input) in [
        (Layout::Generic, "title,isbn,ISBN\nT,2222-2222,1111-1111\n"),
        (
            Layout::Zotero,
            "Key,Item Type,Publication Year,Title,ISBN,ISBN\nK,book,2020,T,978-3-16-148410-0,0-306-40615-2\n",
        ),
    ] {
        let record = parse_as(layout, input).unwrap().records.remove(0);
        let output = serialize_as(layout, std::slice::from_ref(&record));
        assert_eq!(parse_as(layout, &output).unwrap().records, [record]);
    }
    let generic = only_record("title,isbn,ISBN\nT,2222-2222,1111-1111\n");
    assert_eq!(generic.isbn.as_deref(), Some("1111-1111"));
    assert_eq!(generic.extras["csv:isbn"], ["2222-2222"]);
}

/// A generic identifier column whose header is not written `isbn` keeps the other identifiers
/// under `csv:isbn`, the column it is written back to. Regression test: they were kept under the
/// header as written, which changed when the record was written and read again.
#[test]
fn identifier_extras_keyed_by_written_column() {
    let record = only_record("title, ISBN \nT,\"2222-2222; 1111-1111\"\n");
    assert_eq!(record.extras["csv:isbn"], ["2222-2222"]);
    let output = serialize(std::slice::from_ref(&record));
    assert_eq!(parse(&output).unwrap().records, [record]);
}

/// A cell with no identifier or text, only separators, gives no identifier, so Zotero's `ISSN` is
/// read when `ISBN` holds one; and a generic cell of `NA` with a separator is blank. Regression
/// test: they gave an empty identifier, which hid the `ISSN` and could not be written back.
#[test]
fn separator_only_cell_gives_no_identifier() {
    let header = "Key,Item Type,Publication Year,Title,ISBN,ISSN";
    let fallback = only_record(&format!("{header}\nK,book,2020,T,;,1111-1111\n"));
    assert_eq!(fallback.isbn.as_deref(), Some("1111-1111"));
    let alone = only_record(&format!("{header}\nK,book,2020,T,;,\n"));
    assert_eq!(alone.isbn, None);
    assert_eq!(only_record("title,isbn\nT,\"NA,\"\n").isbn, None);
}
