#![allow(dead_code)]

use proptest::prelude::*;

use biblio::{PublicationDate, Record};

/// Printable ASCII without `\n` `\r` – safe for single-line bibliographic values.
pub const VALUE_CHARS: &str = "[A-Za-z0-9 .,;:!?'\"()\\[\\]{}/<>@#$%^&*+=_~-]";

pub fn arb_text(max_len: usize) -> impl Strategy<Value = String> {
    proptest::string::string_regex(&format!("{VALUE_CHARS}{{1,{max_len}}}"))
        .unwrap()
        .prop_map(|s| s.trim_end().to_owned())
        .prop_filter("must not be empty after trim", |s| !s.is_empty())
}

pub fn arb_author() -> impl Strategy<Value = String> {
    (
        "[A-Z][a-z'-]{1,15}( [A-Z][a-z'-]{1,10}){0,2}",
        "[A-Z][a-z'-]{1,15}( [A-Z]\\.?){0,2}",
    )
        .prop_map(|(last, first)| format!("{last}, {first}"))
}

pub fn arb_date() -> impl Strategy<Value = PublicationDate> {
    (
        1900_i32..2100,
        proptest::option::of(1_u8..=12),
        proptest::option::of(1_u8..=28),
    )
        .prop_map(|(year, month, day)| {
            let day = if month.is_some() { day } else { None };
            PublicationDate { year, month, day }
        })
}

pub fn arb_pages() -> impl Strategy<Value = String> {
    prop_oneof![
        "[0-9]{1,4}-[0-9]{1,4}",   // range
        "e[0-9]{3,6}",             // e-article
        "S[0-9]{1,3}-S[0-9]{1,3}", // supplement
        "[0-9]{1,4}",              // single page
    ]
}

pub fn arb_volume() -> impl Strategy<Value = String> {
    prop_oneof![
        "[0-9]{1,3}",
        "[0-9]{1,3}[A-Z]", // e.g. "12A"
        "S[0-9]{1,2}",     // supplement
        "Suppl [0-9]",
    ]
}

/// Raw values that could follow a tag's separator, including the edge cases: empty, whitespace
/// only (ASCII and Unicode), leading whitespace and long values. `\PC` excludes control characters,
/// so a value never contains a line break. Parsers trim the end of a value, so the expected parsed
/// value is `value.trim_end()`.
pub fn arb_raw_value() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "\\PC{1,60}",
        1 => Just(String::new()),
        1 => "[ \u{a0}\u{3000}\u{2028}]{1,5}",
        1 => "[ \u{3000}]{1,3}\\PC{1,40}",
        1 => "\\PC{200,400}",
    ]
}

/// One word of a keyword: letters, digits and punctuation that every format keeps as written, with
/// at most one hyphen inside (`BibTeX` turns `--` into a dash).
const KEYWORD_WORD: &str = "[A-Za-z0-9&/*().:'+_]{1,8}(-[A-Za-z0-9&/*().:'+_]{1,8})?";

/// A keyword of one to four words, separated by a single space, or by `, ` or `; ` as in subject
/// headings such as `Carcinoma, Squamous Cell`. It never starts or ends with whitespace, and never
/// holds a run of spaces, which `BibTeX` collapses.
pub fn arb_keyword() -> impl Strategy<Value = String> {
    proptest::string::string_regex(&format!("{KEYWORD_WORD}((, |; | ){KEYWORD_WORD}){{0,3}}"))
        .unwrap()
}

/// A link: a scheme, a host of two to four labels, up to three path segments and an optional query
/// with `&` and `%`, as real links have. It holds no whitespace, braces or `--`, which `BibTeX`
/// would change.
pub fn arb_url() -> impl Strategy<Value = String> {
    "https?://[a-z0-9]{1,10}(\\.[a-z0-9]{1,10}){1,3}(/[A-Za-z0-9._~%]{1,10}(-[A-Za-z0-9._~%]{1,10})?){0,3}(\\?[a-z]{1,5}=[A-Za-z0-9%]{1,8}(&[a-z]{1,5}=[A-Za-z0-9%]{1,8})?)?"
}

/// Blank values for a list field: empty, or ASCII and Unicode whitespace only.
pub fn arb_blank() -> impl Strategy<Value = String> {
    "[ \t\u{a0}\u{3000}]{0,4}"
}

/// The first value of `record`'s extra `key`, if any.
pub fn first_extra<'a>(record: &'a Record, key: &str) -> Option<&'a str> {
    record.extras.get(key)?.first().map(String::as_str)
}

/// An ISSN, or an ISBN-13 hyphenated in one of several ways, as exports write them. A check digit
/// may be `X` or `x`.
pub fn arb_identifier() -> impl Strategy<Value = String> {
    prop_oneof![
        "[0-9]{4}-[0-9]{3}[0-9Xx]",
        "97[89]-[0-9]{1,2}-[0-9]{3,4}-[0-9]{3,4}-[0-9X]",
        "97[89][0-9]{9}[0-9X]",
    ]
}

/// The identifiers of one record as an export lists them, and the values that list them.
#[derive(Debug, Clone)]
pub struct IdentifierValues {
    /// Every identifier, without labels, in the order listed.
    pub identifiers: Vec<String>,
    /// One to three values, each listing one to three identifiers with or without a label, as
    /// `(Print)`, `ISBN` or a glued `ISBN-13:`, separated by `, `, `; ` or a space, and sometimes a
    /// value of text with no identifier, `n/a`. A value holds no line break, brace or `--`.
    pub values: Vec<String>,
}

impl IdentifierValues {
    /// The identifier a record keeps: the one with the lowest digits, compared as text, then the
    /// lowest as written.
    pub fn lowest(&self) -> &str {
        self.identifiers
            .iter()
            .min_by(|a, b| digit_key(a).cmp(&digit_key(b)).then(a.cmp(b)))
            .map_or("", String::as_str)
    }
}

/// The digits of `identifier`, and its check digit `X` or `x`, upper-cased.
pub fn digit_key(identifier: &str) -> String {
    identifier
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The identifiers of one record, listed in one to three values.
pub fn arb_identifier_values() -> impl Strategy<Value = IdentifierValues> {
    let labels = proptest::sample::select(vec![
        "",
        " (Print)",
        " (Electronic)",
        " (Linking)",
        " (ISBN)",
        "ISBN ",
        "ISBN-13:",
    ]);
    let written = (arb_identifier(), labels).prop_map(|(identifier, label)| {
        let text = if label.ends_with([' ', ':']) {
            format!("{label}{identifier}")
        } else {
            format!("{identifier}{label}")
        };
        (identifier, text)
    });
    let value = (
        proptest::collection::vec(written, 1..=3),
        proptest::sample::select(vec![", ", "; ", " "]),
    );
    let values = (proptest::collection::vec(value, 1..=3), any::<bool>());
    values.prop_map(|(values, with_text)| {
        let identifiers = values
            .iter()
            .flat_map(|(written, _)| written.iter().map(|(identifier, _)| identifier.clone()))
            .collect();
        let values = values
            .iter()
            .map(|(written, separator)| {
                written
                    .iter()
                    .map(|(_, text)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(separator)
            })
            .collect::<Vec<_>>();
        let values = if with_text {
            [values, vec!["n/a".to_owned()]].concat()
        } else {
            values
        };
        IdentifierValues {
            identifiers,
            values,
        }
    })
}
