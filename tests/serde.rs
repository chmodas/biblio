//! JSON form of the types that support serde, built only with the `serde` feature.

use std::collections::HashMap;

use proptest::collection::{hash_map, vec};
use proptest::option;
use proptest::prelude::*;
use proptest::sample::select;
use serde_json::{Value, json};
use test_strategy::{Arbitrary, proptest};

use biblio::csv::Layout;
use biblio::{Diagnostic, Error, Parsed, PublicationDate, Record};

/// `value` read as JSON into `T`.
fn read<T: serde::de::DeserializeOwned>(value: Value) -> serde_json::Result<T> {
    serde_json::from_value(value)
}

/// One diagnostic of every kind, with its JSON form.
fn every_diagnostic() -> Vec<(Diagnostic, Value)> {
    vec![
        (
            Diagnostic::MissingTitle { line: 1 },
            json!({"kind": "missing_title", "line": 1}),
        ),
        (
            Diagnostic::UnterminatedRecord { line: 2 },
            json!({"kind": "unterminated_record", "line": 2}),
        ),
        (
            Diagnostic::IgnoredLine { line: 3 },
            json!({"kind": "ignored_line", "line": 3}),
        ),
        (
            Diagnostic::SkippedEntry {
                line: 4,
                message: "expected comma".into(),
            },
            json!({"kind": "skipped_entry", "line": 4, "message": "expected comma"}),
        ),
        (
            Diagnostic::ReferenceCycle { line: 5 },
            json!({"kind": "reference_cycle", "line": 5}),
        ),
        (
            Diagnostic::ReferenceTooDeep { line: 6 },
            json!({"kind": "reference_too_deep", "line": 6}),
        ),
        (
            Diagnostic::DroppedField {
                line: 7,
                field: "month".into(),
            },
            json!({"kind": "dropped_field", "line": 7, "field": "month"}),
        ),
        (
            Diagnostic::Truncated {
                line: 8,
                message: "input ended".into(),
            },
            json!({"kind": "truncated", "line": 8, "message": "input ended"}),
        ),
        (
            Diagnostic::ExtraCells { line: 9, count: 2 },
            json!({"kind": "extra_cells", "line": 9, "count": 2}),
        ),
    ]
}

// -- Shape --

/// A record is an object with a member for every field: `null` without a value, `[]` for an empty
/// list, and extras as an object of string arrays.
#[test]
fn record_shape() {
    let record = Record::builder()
        .title("A title")
        .author("Doe, Jane")
        .date(PublicationDate {
            year: 2024,
            month: Some(9),
            day: None,
        })
        .keyword("cats")
        .extra("ris:N1", "a note")
        .extra("ris:N1", "another")
        .extra("csv:record_id", "1")
        .build();
    assert_eq!(
        serde_json::to_value(&record).unwrap(),
        json!({
            "title": "A title",
            "authors": ["Doe, Jane"],
            "date": {"year": 2024, "month": 9, "day": null},
            "journal": null,
            "doi": null,
            "pages": null,
            "volume": null,
            "number": null,
            "abstract_text": null,
            "isbn": null,
            "database": null,
            "label": null,
            "keywords": ["cats"],
            "urls": [],
            "extras": {"ris:N1": ["a note", "another"], "csv:record_id": ["1"]},
        })
    );
}

/// A date is an object of year, month and day.
#[test]
fn date_shape() {
    let full = PublicationDate {
        year: 2024,
        month: Some(9),
        day: Some(1),
    };
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        json!({"year": 2024, "month": 9, "day": 1})
    );
    let year_only = PublicationDate {
        year: 2020,
        month: None,
        day: None,
    };
    assert_eq!(
        serde_json::to_value(year_only).unwrap(),
        json!({"year": 2020, "month": null, "day": null})
    );
}

/// Every diagnostic is an object tagged by its kind, next to its fields.
#[test]
fn diagnostic_shapes() {
    for (diagnostic, expected) in every_diagnostic() {
        assert_eq!(serde_json::to_value(&diagnostic).unwrap(), expected);
        assert_eq!(read::<Diagnostic>(expected).unwrap(), diagnostic);
    }
}

/// Every error is an object tagged by its kind, including the one holding only a message.
#[test]
fn error_shapes() {
    let cases = [
        (Error::EmptyInput, json!({"kind": "empty_input"})),
        (
            Error::MalformedSyntax {
                line: 3,
                message: "unexpected end tag".into(),
            },
            json!({"kind": "malformed_syntax", "line": 3, "message": "unexpected end tag"}),
        ),
        (
            Error::Internal("reader failed".into()),
            json!({"kind": "internal", "message": "reader failed"}),
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(serde_json::to_value(&error).unwrap(), expected);
        assert_eq!(read::<Error>(expected).unwrap(), error);
    }
}

/// CSV layouts are lower-case strings.
#[test]
fn layout_shapes() {
    for (layout, name) in [
        (Layout::Generic, "generic"),
        (Layout::Zotero, "zotero"),
        (Layout::Syrf, "syrf"),
    ] {
        assert_eq!(serde_json::to_value(layout).unwrap(), json!(name));
        assert_eq!(read::<Layout>(json!(name)).unwrap(), layout);
    }
}

/// A parse result is its records and diagnostics.
#[test]
fn parsed_shape() {
    let parsed = biblio::ris::parse("TY  - JOUR\nER  -\n").unwrap();
    let value = serde_json::to_value(&parsed).unwrap();
    assert_eq!(value["records"][0]["title"], Value::Null);
    assert_eq!(
        value["diagnostics"],
        json!([{"kind": "missing_title", "line": 1}])
    );
}

// -- Reading --

/// Members left out take their empty value, and members the type has no field for are ignored.
#[test]
fn missing_and_unknown_members() {
    let title_only: Record = read(json!({"title": "X"})).unwrap();
    assert_eq!(title_only, Record::builder().title("X").build());
    assert_eq!(read::<Record>(json!({})).unwrap(), Record::default());
    assert_eq!(
        read::<Record>(json!({"titel": "X", "title": "Y"})).unwrap(),
        Record::builder().title("Y").build()
    );
    assert_eq!(read::<Parsed>(json!({})).unwrap(), Parsed::default());
    assert_eq!(
        read::<PublicationDate>(json!({"year": 2020})).unwrap(),
        PublicationDate {
            year: 2020,
            month: None,
            day: None,
        }
    );
}

/// The ends of each number's range are read exactly.
#[test]
fn boundary_numbers_read() {
    for year in [i32::MIN, i32::MAX] {
        let date: PublicationDate = read(json!({"year": year, "month": 255, "day": 0})).unwrap();
        assert_eq!(
            (date.year, date.month, date.day),
            (year, Some(255), Some(0))
        );
    }
    let diagnostic: Diagnostic =
        read(json!({"kind": "missing_title", "line": usize::MAX})).unwrap();
    assert_eq!(diagnostic, Diagnostic::MissingTitle { line: usize::MAX });
}

/// Values of the wrong kind or out of range, unknown kinds and missing required members cannot be
/// read.
#[test]
fn invalid_json_rejected() {
    assert!(read::<Diagnostic>(json!({"kind": "no_such_kind", "line": 1})).is_err());
    assert!(read::<Diagnostic>(json!({"line": 1})).is_err());
    assert!(read::<Diagnostic>(json!({"kind": "missing_title"})).is_err());
    assert!(read::<Diagnostic>(json!({"kind": "missing_title", "line": -1})).is_err());
    assert!(read::<Error>(json!({"kind": "internal"})).is_err());
    assert!(read::<Error>(json!({"kind": "no_such_kind"})).is_err());
    assert!(read::<Layout>(json!("tsv")).is_err());
    for date in [
        json!({}),
        json!({"year": 2020, "month": 256}),
        json!({"year": 2020, "month": -1}),
        json!({"year": 2_147_483_648_i64}),
        json!({"year": 2020.0}),
    ] {
        assert!(read::<PublicationDate>(date.clone()).is_err(), "{date}");
    }
    for record in [
        json!({"authors": null}),
        json!({"authors": "x"}),
        json!({"extras": {"k": "v"}}),
        json!({"title": 1}),
    ] {
        assert!(read::<Record>(record.clone()).is_err(), "{record}");
    }
    assert!(serde_json::from_str::<Record>(r#"{"title": "a", "title": "b"}"#).is_err());
}

/// Members a diagnostic or error has no field for are ignored.
#[test]
fn unknown_members_of_tagged_types_ignored() {
    assert_eq!(
        read::<Diagnostic>(json!({"kind": "missing_title", "line": 1, "bogus": 2})).unwrap(),
        Diagnostic::MissingTitle { line: 1 }
    );
    assert_eq!(
        read::<Error>(json!({"kind": "empty_input", "line": 3})).unwrap(),
        Error::EmptyInput
    );
}

/// A record can also be read from an array of its fields in order.
#[test]
fn record_read_from_array() {
    assert_eq!(read::<Record>(json!([])).unwrap(), Record::default());
    assert_eq!(
        read::<Record>(json!(["t"])).unwrap(),
        Record::builder().title("t").build()
    );
}

// -- End to end --

/// Parse results with diagnostics come back the same through JSON.
#[test]
fn parse_results_round_trip() {
    let ris = biblio::ris::parse("TY  - JOUR\nAU  - Doe, J\nER  -\nTY  - BOOK\nTI  - T\n").unwrap();
    let csv = biblio::csv::parse("title,doi\nT,10.1/x,surplus\n").unwrap();
    assert_eq!(
        ris.diagnostics,
        vec![
            Diagnostic::MissingTitle { line: 1 },
            Diagnostic::UnterminatedRecord { line: 4 },
        ]
    );
    assert_eq!(
        csv.diagnostics,
        vec![Diagnostic::ExtraCells { line: 2, count: 1 }]
    );
    for parsed in [ris, csv] {
        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(serde_json::from_str::<Parsed>(&json).unwrap(), parsed);
    }
}

// -- Properties --

/// Text for an optional field: none, empty, or any string.
fn arb_optional_text() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some(String::new())),
        any::<String>().prop_map(Some)
    ]
}

/// A record with every field set from arbitrary values, including text with control characters
/// and characters beyond the Basic Multilingual Plane, and dates out of their usual ranges.
#[derive(Arbitrary, Debug)]
struct GeneratedRecord {
    /// Title, journal, DOI, pages, volume, number and abstract.
    #[strategy([arb_optional_text(), arb_optional_text(), arb_optional_text(), arb_optional_text(),
        arb_optional_text(), arb_optional_text(), arb_optional_text()])]
    text: [Option<String>; 7],
    /// ISBN, database and label.
    #[strategy([arb_optional_text(), arb_optional_text(), arb_optional_text()])]
    more_text: [Option<String>; 3],
    /// Authors, keywords and links.
    #[strategy([vec(any::<String>(), 0..4), vec(any::<String>(), 0..4), vec(any::<String>(), 0..4)])]
    lists: [Vec<String>; 3],
    /// The date, with any year, month and day.
    #[strategy(option::of((any::<i32>(), any::<Option<u8>>(), any::<Option<u8>>())))]
    date: Option<(i32, Option<u8>, Option<u8>)>,
    /// Extras with any keys, each with any number of values.
    #[strategy(hash_map(any::<String>(), vec(any::<String>(), 0..4), 0..4))]
    extras: HashMap<String, Vec<String>>,
}

impl GeneratedRecord {
    /// The record, with every public field set.
    fn into_record(self) -> Record {
        let [title, journal, doi, pages, volume, number, abstract_text] = self.text;
        let [isbn, database, label] = self.more_text;
        let [authors, keywords, urls] = self.lists;
        let mut record = Record::default();
        record.title = title;
        record.authors = authors;
        record.date = self
            .date
            .map(|(year, month, day)| PublicationDate { year, month, day });
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
        record.extras = self.extras;
        record
    }
}

/// A diagnostic of any kind, with any line and text.
fn arb_diagnostic() -> impl Strategy<Value = Diagnostic> {
    (0_usize..9, any::<usize>(), any::<usize>(), any::<String>()).prop_map(
        |(kind, line, count, text)| match kind {
            0 => Diagnostic::MissingTitle { line },
            1 => Diagnostic::UnterminatedRecord { line },
            2 => Diagnostic::IgnoredLine { line },
            3 => Diagnostic::SkippedEntry {
                line,
                message: text,
            },
            4 => Diagnostic::ReferenceCycle { line },
            5 => Diagnostic::ReferenceTooDeep { line },
            6 => Diagnostic::DroppedField { line, field: text },
            7 => Diagnostic::Truncated {
                line,
                message: text,
            },
            _ => Diagnostic::ExtraCells { line, count },
        },
    )
}

/// An error of any kind, with any line and text.
fn arb_error() -> impl Strategy<Value = Error> {
    (0_usize..3, any::<usize>(), any::<String>()).prop_map(|(kind, line, message)| match kind {
        0 => Error::EmptyInput,
        1 => Error::MalformedSyntax { line, message },
        _ => Error::Internal(message),
    })
}

/// A record comes back the same through JSON, written compactly or pretty-printed.
#[proptest]
fn record_round_trips(generated: GeneratedRecord) {
    let record = generated.into_record();
    let compact = serde_json::to_string(&record).unwrap();
    prop_assert_eq!(&serde_json::from_str::<Record>(&compact).unwrap(), &record);
    let pretty = serde_json::to_string_pretty(&record).unwrap();
    prop_assert_eq!(&serde_json::from_str::<Record>(&pretty).unwrap(), &record);
}

/// A parse result comes back the same through JSON.
#[proptest]
fn parsed_round_trips(
    #[strategy(vec(any::<GeneratedRecord>(), 0..3))] records: Vec<GeneratedRecord>,
    #[strategy(vec(arb_diagnostic(), 0..4))] diagnostics: Vec<Diagnostic>,
) {
    let mut parsed = Parsed::default();
    parsed.records = records
        .into_iter()
        .map(GeneratedRecord::into_record)
        .collect();
    parsed.diagnostics = diagnostics;
    let json = serde_json::to_string(&parsed).unwrap();
    prop_assert_eq!(serde_json::from_str::<Parsed>(&json).unwrap(), parsed);
}

/// Diagnostics, errors and layouts come back the same through JSON.
#[proptest]
fn diagnostics_errors_and_layouts_round_trip(
    #[strategy(arb_diagnostic())] diagnostic: Diagnostic,
    #[strategy(arb_error())] error: Error,
    #[strategy(select(vec![Layout::Generic, Layout::Zotero, Layout::Syrf]))] layout: Layout,
) {
    let json = serde_json::to_string(&diagnostic).unwrap();
    prop_assert_eq!(
        serde_json::from_str::<Diagnostic>(&json).unwrap(),
        diagnostic
    );
    let json = serde_json::to_string(&error).unwrap();
    prop_assert_eq!(serde_json::from_str::<Error>(&json).unwrap(), error);
    let json = serde_json::to_string(&layout).unwrap();
    prop_assert_eq!(serde_json::from_str::<Layout>(&json).unwrap(), layout);
}

/// Reading arbitrary text as JSON never panics.
#[proptest]
fn reading_arbitrary_text_never_panics(text: String) {
    let _ = serde_json::from_str::<Parsed>(&text);
    let _ = serde_json::from_str::<Record>(&text);
    let _ = serde_json::from_str::<Diagnostic>(&text);
    let _ = serde_json::from_str::<Error>(&text);
}

/// A JSON value of any type, to put under real member names.
fn arb_json_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<f64>().prop_map(Value::from),
        any::<String>().prop_map(Value::from),
        vec(any::<String>(), 0..3).prop_map(Value::from),
        Just(json!({"k": ["v"]})),
        Just(json!({"year": 1})),
    ]
}

/// Objects built from the real member names and kinds, with values of any type, never panic when
/// read; they are read or rejected.
#[proptest]
fn reading_structured_json_never_panics(
    #[strategy(hash_map(select(vec![
        "kind", "line", "message", "count", "field", "title", "authors", "date", "extras", "year",
        "month", "day", "records", "diagnostics", "keywords",
    ]), arb_json_value(), 0..6))]
    members: HashMap<&'static str, Value>,
    #[strategy(select(vec!["missing_title", "extra_cells", "internal", "malformed_syntax", "bogus"]))]
    kind: &'static str,
) {
    let mut object: serde_json::Map<String, Value> = members
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    let _ = read::<Record>(Value::Object(object.clone()));
    let _ = read::<Parsed>(json!({"records": [Value::Object(object.clone())]}));
    object.insert("kind".into(), json!(kind));
    let _ = read::<Diagnostic>(Value::Object(object.clone()));
    let _ = read::<Error>(Value::Object(object));
}
