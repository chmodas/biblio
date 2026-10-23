use biblio::{Diagnostic, PublicationDate, Record};

/// Every builder method sets its own field; `authors` appends after `author`, and `extra` appends
/// to its key.
#[test]
fn builder_sets_every_field() {
    let date = PublicationDate {
        year: 2024,
        month: Some(9),
        day: None,
    };
    let record = Record::builder()
        .title("A title")
        .author("Doe, Jane")
        .authors(["Smith, John", "Roe, Rick"])
        .date(date)
        .journal("Nature")
        .doi("10.1000/xyz")
        .pages("1-10")
        .volume("12")
        .number("3")
        .abstract_text("An abstract.")
        .isbn("1234-5678")
        .database("Scopus")
        .label("OLD")
        .keyword("cats")
        .keywords(["dogs", "mice"])
        .url("https://example.org/a")
        .urls(["https://example.org/b"])
        .extra("nbib:PMID", "123")
        .extra("ris:N1", "cats")
        .extra("ris:N1", "dogs")
        .build();

    assert_eq!(record.title.as_deref(), Some("A title"));
    assert_eq!(
        record.authors,
        vec!["Doe, Jane", "Smith, John", "Roe, Rick"]
    );
    assert_eq!(record.date, Some(date));
    assert_eq!(record.journal.as_deref(), Some("Nature"));
    assert_eq!(record.doi.as_deref(), Some("10.1000/xyz"));
    assert_eq!(record.pages.as_deref(), Some("1-10"));
    assert_eq!(record.volume.as_deref(), Some("12"));
    assert_eq!(record.number.as_deref(), Some("3"));
    assert_eq!(record.abstract_text.as_deref(), Some("An abstract."));
    assert_eq!(record.isbn.as_deref(), Some("1234-5678"));
    assert_eq!(record.database.as_deref(), Some("Scopus"));
    assert_eq!(record.label.as_deref(), Some("OLD"));
    assert_eq!(record.keywords, vec!["cats", "dogs", "mice"]);
    assert_eq!(
        record.urls,
        vec!["https://example.org/a", "https://example.org/b"]
    );
    assert_eq!(record.extras["nbib:PMID"], vec!["123"]);
    assert_eq!(record.extras["ris:N1"], vec!["cats", "dogs"]);
}

/// A builder with nothing set gives the empty record.
#[test]
fn empty_builder_gives_default_record() {
    assert_eq!(Record::builder().build(), Record::default());
}

/// Every diagnostic reports its line and describes itself.
#[test]
fn diagnostic_line_and_display() {
    let cases = [
        (
            Diagnostic::MissingTitle { line: 1 },
            "record at line 1 has no title",
        ),
        (
            Diagnostic::UnterminatedRecord { line: 2 },
            "record at line 2 was not closed with ER",
        ),
        (Diagnostic::IgnoredLine { line: 3 }, "ignored line 3"),
        (
            Diagnostic::SkippedEntry {
                line: 4,
                message: "expected comma".into(),
            },
            "skipped entry at line 4: expected comma",
        ),
        (
            Diagnostic::Truncated {
                line: 5,
                message: "input ended inside a record".into(),
            },
            "input truncated at line 5: input ended inside a record",
        ),
        (
            Diagnostic::ReferenceCycle { line: 6 },
            "reference cycle at line 6: crossref or xdata removed",
        ),
        (
            Diagnostic::ReferenceTooDeep { line: 7 },
            "references at line 7 go too deep or copy too much: crossref and xdata removed",
        ),
        (
            Diagnostic::DroppedField {
                line: 8,
                field: "month".into(),
            },
            "field month of the entry at line 8 could not be read and was removed",
        ),
    ];
    for (expected_line, (diagnostic, text)) in (1..).zip(cases) {
        assert_eq!(diagnostic.line(), expected_line);
        assert_eq!(diagnostic.to_string(), text);
    }
}
