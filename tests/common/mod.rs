#![allow(dead_code)]

use proptest::prelude::*;

use biblio::PublicationDate;

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
