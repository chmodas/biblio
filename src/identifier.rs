//! Choosing one ISBN or ISSN from the several a record can list. See [`choose`] for the rule, and
//! [`Record::isbn`](crate::Record::isbn) for how the formats use it.

use std::cmp::Ordering;
use std::collections::HashMap;

/// The identifier a record keeps, and the other candidates it lists.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Identifiers {
    /// The chosen identifier, without its labels; `None` when no value was given.
    pub(crate) chosen: Option<String>,
    /// Every other candidate, as written, in input order.
    pub(crate) rest: Vec<String>,
}

/// The fewest characters in an identifier's digit key: an ISSN has 8, an ISBN 10 or 13. A shorter
/// run of digits, such as the `13` of `ISBN-13:`, is a label.
const MIN_KEY_LEN: usize = 8;

/// One candidate identifier read from a value.
#[derive(Debug)]
struct Candidate {
    /// The candidate's tokens, labels included, joined by one space.
    as_written: String,
    /// The identifier without its labels or the text around its digits; the whole text for a
    /// candidate without an identifier.
    bare: String,
    /// The ASCII digits of `bare`, and `X` for every `X` or `x` in it; empty for a candidate
    /// without an identifier.
    key: String,
    /// Whether the candidate has no identifier, only text such as `n/a`.
    text_only: bool,
}

impl Candidate {
    /// The order in which candidates are chosen: identifiers before text, then by digit key, then
    /// by bare identifier, then an identifier written alone before one with labels, then by the
    /// text as written. It is a total order, so the choice does not depend on input order.
    fn rank(&self, other: &Self) -> Ordering {
        let exact = |candidate: &Self| candidate.as_written == candidate.bare;
        self.text_only
            .cmp(&other.text_only)
            .then_with(|| self.key.cmp(&other.key))
            .then_with(|| self.bare.cmp(&other.bare))
            .then_with(|| exact(other).cmp(&exact(self)))
            .then_with(|| self.as_written.cmp(&other.as_written))
    }
}

/// The candidates in one value, each as written, in order. See [`choose`] for how a value is read.
pub(crate) fn split(value: &str) -> Vec<String> {
    candidates(value)
        .into_iter()
        .map(|candidate| candidate.as_written)
        .collect()
}

/// Choose the identifier a record keeps from every value its format gives for it.
///
/// - **Segments:** each value is split at `,`, `;`, `\r` and `\n`, even inside parentheses, so a
///   label never belongs to an identifier in another segment.
/// - **Tokens:** a segment is split at whitespace. A group in parentheses, such as `(Print)`, is
///   one token even with spaces inside, and ends the token before it, so `(Print)0964-6906` gives
///   `(Print)` and `0964-6906`. A `(` without a `)` after it in the segment is an ordinary
///   character.
/// - **Identifiers:** a token outside parentheses is an identifier when its digit key has at least
///   eight characters. Every other token is a label: `(Print)`, `ISBN`, `ISBN-13:`, `(2nd ed.)`.
///   The identifier's bare form runs from its first digit, after the last `:` if there is one, to
///   its last digit or `X`, so `ISBN-13:978…` gives `978…`. The digit key is the ASCII digits of
///   the bare form and every `X` or `x` in it, upper-cased.
/// - **Candidates:** each identifier takes the labels after it in its segment, and the first one
///   also the labels before it. A segment with no identifier is one candidate of text only.
///
/// The chosen candidate is the one with the lowest digit key, compared as text; text-only
/// candidates come last. Ties go to the lower bare identifier, then to one written exactly as its
/// bare form, without labels or other text, then to the lower text as written. Its bare identifier,
/// or its text, is [`Identifiers::chosen`]. When values are given but hold no candidate, because
/// every one is blank or only separators, the chosen identifier is empty.
pub(crate) fn choose<'a>(values: impl IntoIterator<Item = &'a str>) -> Identifiers {
    let mut given = false;
    let mut candidates: Vec<Candidate> = Vec::new();
    for value in values {
        given = true;
        candidates.extend(self::candidates(value));
    }

    let Some(index) = (0..candidates.len()).min_by(|&a, &b| candidates[a].rank(&candidates[b]))
    else {
        return Identifiers {
            chosen: given.then(String::new),
            rest: Vec::new(),
        };
    };
    let chosen = candidates.remove(index).bare;
    Identifiers {
        chosen: Some(chosen),
        rest: candidates
            .into_iter()
            .map(|candidate| candidate.as_written)
            .collect(),
    }
}

/// [`choose`] from `values`, adding the rest to `extras` under `key`, and return the chosen
/// identifier. Nothing is added when there is no rest.
pub(crate) fn choose_into<'a>(
    values: impl IntoIterator<Item = &'a str>,
    extras: &mut HashMap<String, Vec<String>>,
    key: &str,
) -> Option<String> {
    let Identifiers { chosen, rest } = choose(values);
    if !rest.is_empty() {
        extras.entry(key.to_owned()).or_default().extend(rest);
    }
    chosen
}

/// Add every candidate in `value` to `extras` under `key`, as [`split`] gives them. Nothing is
/// added when there is none.
pub(crate) fn split_into(value: &str, extras: &mut HashMap<String, Vec<String>>, key: &str) {
    let candidates = split(value);
    if !candidates.is_empty() {
        extras.entry(key.to_owned()).or_default().extend(candidates);
    }
}

/// The candidates in `value`, in order.
fn candidates(value: &str) -> Vec<Candidate> {
    value
        .split([',', ';', '\r', '\n'])
        .flat_map(segment_candidates)
        .collect()
}

/// The candidates in one segment: one per identifier, with its labels, or a single text-only
/// candidate when the segment has no identifier. A blank segment has none.
fn segment_candidates(segment: &str) -> Vec<Candidate> {
    let tokens = tokens(segment);
    if tokens.is_empty() {
        return Vec::new();
    }
    let identifiers: Vec<Option<(String, String)>> =
        tokens.iter().map(|token| identifier(token)).collect();
    let Some(first) = identifiers.iter().position(Option::is_some) else {
        let text = tokens.join(" ");
        return vec![Candidate {
            as_written: text.clone(),
            bare: text,
            key: String::new(),
            text_only: true,
        }];
    };

    // Each candidate starts at an identifier, or at the segment's start for the first one, and
    // runs up to the next identifier.
    let starts: Vec<usize> = identifiers
        .iter()
        .enumerate()
        .filter(|&(index, identifier)| identifier.is_some() && index != first)
        .map(|(index, _)| index)
        .collect();
    let bounds = std::iter::once(0)
        .chain(starts.iter().copied())
        .zip(starts.iter().copied().chain(std::iter::once(tokens.len())));
    bounds
        .map(|(start, end)| {
            let (bare, key) = identifiers[start..end]
                .iter()
                .flatten()
                .next()
                .cloned()
                .unwrap_or_default();
            Candidate {
                as_written: tokens[start..end].join(" "),
                bare,
                key,
                text_only: false,
            }
        })
        .collect()
}

/// Split a segment into tokens at whitespace. A `(` with a `)` after it starts a group that runs to
/// that `)`; the group is one token, and ends the token before it.
///
/// Each search for a `)` starts after the group before it, and a `(` after the last `)` is not
/// searched from at all, so the time taken grows linearly with the segment.
fn tokens(segment: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    let last_close = segment.rfind(')');
    let mut chars = segment.char_indices();
    while let Some((index, c)) = chars.next() {
        let opens_group = c == '(' && last_close.is_some_and(|close| close > index);
        if c.is_whitespace() {
            if let Some(begin) = start.take() {
                tokens.push(&segment[begin..index]);
            }
        } else if let Some(close) = opens_group.then(|| segment[index..].find(')')).flatten() {
            if let Some(begin) = start.take() {
                tokens.push(&segment[begin..index]);
            }
            let end = index + close + 1;
            tokens.push(&segment[index..end]);
            // Skip the rest of the group; `)` is one byte, so `end` is a character boundary.
            while chars.as_str().len() > segment.len() - end {
                chars.next();
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start {
        tokens.push(&segment[begin..]);
    }
    tokens
}

/// The bare identifier and digit key of `token`, if it is an identifier rather than a label.
fn identifier(token: &str) -> Option<(String, String)> {
    if token.starts_with('(') && token.ends_with(')') {
        return None;
    }
    let end = token.rfind(|c: char| c.is_ascii_digit() || c == 'X' || c == 'x')? + 1;
    // A label glued on with a `:`, such as `ISBN-13:`, ends at the last `:`.
    let from = token[..end].rfind(':').map_or(0, |colon| colon + 1);
    let start = from + token[from..end].find(|c: char| c.is_ascii_digit())?;
    let bare = &token[start..end];
    let key: String = bare
        .chars()
        .filter_map(|c| match c {
            '0'..='9' => Some(c),
            'X' | 'x' => Some('X'),
            _ => None,
        })
        .collect();
    (key.len() >= MIN_KEY_LEN).then(|| (bare.to_owned(), key))
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::select;
    use test_strategy::{Arbitrary, proptest};

    use super::*;

    /// The result of [`choose`] for `values`, with `chosen` expected and `rest` as listed.
    fn expected(chosen: &str, rest: &[&str]) -> Identifiers {
        Identifiers {
            chosen: Some(chosen.to_owned()),
            rest: rest.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    /// An `NBIB` record's print, electronic and linking ISSNs give the lowest, and the others stay
    /// as written in input order. The linking ISSN repeats the print one, and the tie goes to the
    /// lower text as written, so the print ISSN stays in the rest.
    #[test]
    fn labelled_issns_give_the_lowest() {
        let identifiers = choose([
            "1939-1676 (Electronic)",
            "0891-6640 (Print)",
            "0891-6640 (Linking)",
        ]);
        assert_eq!(
            identifiers,
            expected(
                "0891-6640",
                &["1939-1676 (Electronic)", "0891-6640 (Print)"]
            )
        );
    }

    /// A label glued to the next identifier, `(Print)0964-6906`, ends at its `)`.
    #[test]
    fn label_glued_to_next_identifier_is_split() {
        assert_eq!(
            choose(["0964-6906 (Print)0964-6906"]),
            expected("0964-6906", &["0964-6906 (Print)"])
        );
    }

    /// Two labelled ISBNs in one value, separated by `;`, give the lower.
    #[test]
    fn isbns_separated_by_semicolon() {
        assert_eq!(
            choose(["978-131750096-4 (ISBN); 978-113888701-5 (ISBN)"]),
            expected("978-113888701-5", &["978-131750096-4 (ISBN)"])
        );
    }

    /// A literal backslash and `r` before an identifier, as one export writes a lost line break,
    /// are not part of it; the identifier written that way stays in the rest as written.
    #[test]
    fn literal_backslash_r_is_not_part_of_the_identifier() {
        assert_eq!(
            choose([r"1757-4676 (Print)\r1757-4676"]),
            expected("1757-4676", &[r"\r1757-4676"])
        );
    }

    /// A carriage return separates identifiers, as `EndNote` writes them.
    #[test]
    fn carriage_return_separates() {
        assert_eq!(
            choose(["1939-1676 (Electronic)\r0891-6640 (Print)"]),
            expected("0891-6640", &["1939-1676 (Electronic)"])
        );
    }

    /// A line feed separates identifiers, as in a CSV cell.
    #[test]
    fn line_feed_separates() {
        assert_eq!(
            choose(["1939-1676\n0891-6640"]),
            expected("0891-6640", &["1939-1676"])
        );
    }

    /// Whitespace separates identifiers, as Zotero and `EndNote` list ISSNs.
    #[test]
    fn whitespace_separates() {
        assert_eq!(
            choose(["1939-1676 0891-6640"]),
            expected("0891-6640", &["1939-1676"])
        );
    }

    /// A label before an identifier belongs to it, not to the identifier of the segment before.
    #[test]
    fn leading_labels_stay_in_their_segment() {
        assert_eq!(
            split("ISBN 978-3-16-148410-0; ISBN 978-0-306-40615-7"),
            ["ISBN 978-3-16-148410-0", "ISBN 978-0-306-40615-7"]
        );
        assert_eq!(
            choose(["ISBN 978-3-16-148410-0; ISBN 978-0-306-40615-7"]),
            expected("978-0-306-40615-7", &["ISBN 978-3-16-148410-0"])
        );
    }

    /// A label with digits glued to the identifier by a `:`, such as `ISBN-13:978…`, is not part of
    /// it. Regression test: the `13:` was kept, and its digits made the identifier sort first.
    #[test]
    fn glued_label_with_digits_is_stripped() {
        assert_eq!(
            choose(["ISBN-13:978-3-16-148410-0, 978-0-306-40615-7"]),
            expected("978-0-306-40615-7", &["ISBN-13:978-3-16-148410-0"])
        );
    }

    /// A long run of `(` without a `)` is read in linear time. Regression test: every `(` searched
    /// the rest of the segment for a `)`.
    #[test]
    fn unclosed_parentheses_read_in_linear_time() {
        let value = "(".repeat(1_000_000);
        let start = std::time::Instant::now();
        assert_eq!(choose([value.as_str()]), expected(&value, &[]));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    /// A label with a few digits, such as `ISBN-13:`, is not an identifier.
    #[test]
    fn short_digit_runs_are_labels() {
        assert_eq!(
            choose(["ISBN-13: 978-3-16-148410-0"]),
            expected("978-3-16-148410-0", &[])
        );
    }

    /// A label glued to the identifier, `ISBN:978…`, is not part of the chosen identifier.
    #[test]
    fn glued_prefix_is_stripped() {
        assert_eq!(
            choose(["ISBN:978-3-16-148410-0."]),
            expected("978-3-16-148410-0", &[])
        );
    }

    /// Text in parentheses is a label even when it holds digits, however many.
    #[test]
    fn parenthesised_digits_are_labels() {
        assert_eq!(
            choose(["0891-6640 (v. 2) (2nd ed.) (12345678)"]),
            expected("0891-6640", &[])
        );
    }

    /// A value with no identifier, such as a placeholder ISBN with a single digit, is kept whole.
    #[test]
    fn value_without_identifier_is_kept_whole() {
        assert_eq!(choose(["ISBN 1"]), expected("ISBN 1", &[]));
        assert_eq!(choose(["n/a"]), expected("n/a", &[]));
    }

    /// An ISBN written with spaces between its groups is kept whole when it stands alone.
    #[test]
    fn spaced_isbn_alone_is_kept_whole() {
        assert_eq!(
            choose(["978 3 16 148410 0"]),
            expected("978 3 16 148410 0", &[])
        );
    }

    /// A value without an identifier loses to one with an identifier, and is kept in the rest.
    #[test]
    fn identifier_wins_over_text() {
        assert_eq!(
            choose(["n/a", "1939-1676"]),
            expected("1939-1676", &["n/a"])
        );
    }

    /// A `(` without a `)` after it is an ordinary character, so the text after it is not one
    /// label.
    #[test]
    fn unclosed_parenthesis_is_ordinary() {
        assert_eq!(
            choose(["(Print 1939-1676 0891-6640"]),
            expected("0891-6640", &["(Print 1939-1676"])
        );
    }

    /// Of two candidates with the same identifier, the one written without labels is chosen.
    #[test]
    fn identifier_alone_beats_labelled() {
        assert_eq!(
            choose(["0891-6640 (Print)", "0891-6640"]),
            expected("0891-6640", &["0891-6640 (Print)"])
        );
    }

    /// A check digit `X` and `x` compare equal, and the identifiers then compare as written.
    #[test]
    fn check_digit_case_compares_equal() {
        assert_eq!(
            choose(["0044-605x", "0044-605X"]),
            expected("0044-605X", &["0044-605x"])
        );
    }

    /// Values that are only blank give an empty identifier, and no value gives none.
    #[test]
    fn blank_and_missing_values() {
        assert_eq!(choose(["", " ; "]), expected("", &[]));
        assert_eq!(
            choose(Vec::<&str>::new()),
            Identifiers {
                chosen: None,
                rest: Vec::new()
            }
        );
    }

    /// `split` gives every candidate, text-only ones included, as written.
    #[test]
    fn split_gives_every_candidate() {
        assert_eq!(
            split("0891-6640 (Print)  1939-1676; n/a, ,"),
            ["0891-6640 (Print)", "1939-1676", "n/a"]
        );
    }

    /// An identifier of eight to thirteen digits, with hyphens between some of them and a check
    /// digit that may be `X`.
    fn arb_identifier() -> impl Strategy<Value = String> {
        "[0-9]{4}-?[0-9]{3,5}-?[0-9]{0,3}[0-9Xx]"
    }

    /// A label that may stand before or after an identifier.
    fn arb_label() -> impl Strategy<Value = &'static str> {
        select(vec![
            "(Print)",
            "(Electronic)",
            "(Linking)",
            "(v. 2)",
            "(2nd ed.)",
            "(12345678)",
            "ISBN",
            "ISSN:",
            "ISBN-13:",
            "pbk.",
        ])
    }

    /// Text that separates two identifiers in a value.
    fn arb_separator() -> impl Strategy<Value = &'static str> {
        select(vec![
            " ", "  ", ", ", "; ", ",", ";", "\r", "\n", "\r\n", " ;\t",
        ])
    }

    /// One identifier with what may be written around it.
    #[derive(Arbitrary, Debug, Clone)]
    struct LabelledIdentifier {
        /// Labels before the identifier.
        #[strategy(vec(arb_label(), 0..3))]
        before: Vec<&'static str>,
        /// Text glued to the front of the identifier.
        #[strategy(select(vec!["", r"\r", "ISBN:"]))]
        glued: &'static str,
        /// The identifier.
        #[strategy(arb_identifier())]
        identifier: String,
        /// Labels after the identifier.
        #[strategy(vec(arb_label(), 0..3))]
        after: Vec<&'static str>,
    }

    impl LabelledIdentifier {
        /// The identifier with its labels, separated by spaces.
        fn render(&self) -> String {
            let identifier = format!("{}{}", self.glued, self.identifier);
            self.before
                .iter()
                .copied()
                .chain([identifier.as_str()])
                .chain(self.after.iter().copied())
                .collect::<Vec<_>>()
                .join(" ")
        }
    }

    /// Several values, each listing one or more identifiers with labels and separators.
    #[derive(Arbitrary, Debug, Clone)]
    struct Values {
        /// The identifiers of each value.
        #[strategy(vec(vec(any::<LabelledIdentifier>(), 1..4), 1..4))]
        identifiers: Vec<Vec<LabelledIdentifier>>,
        /// The separators between identifiers, used in turn.
        #[strategy(vec(arb_separator(), 1..4))]
        separators: Vec<&'static str>,
    }

    impl Values {
        /// Each value as text.
        fn render(&self) -> Vec<String> {
            self.identifiers
                .iter()
                .map(|identifiers| {
                    let mut value = String::new();
                    for (index, identifier) in identifiers.iter().enumerate() {
                        if index > 0 {
                            value.push_str(self.separators[index % self.separators.len()]);
                        }
                        value.push_str(&identifier.render());
                    }
                    value
                })
                .collect()
        }

        /// The digit keys of every identifier, sorted.
        fn keys(&self) -> Vec<String> {
            let mut keys: Vec<String> = self
                .identifiers
                .iter()
                .flatten()
                .map(|identifier| digit_key(&identifier.identifier))
                .collect();
            keys.sort();
            keys
        }
    }

    /// The digits of `identifier`, with `x` upper-cased: a reference digit key.
    fn digit_key(identifier: &str) -> String {
        identifier
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
            .map(|c| c.to_ascii_uppercase())
            .collect()
    }

    /// The digit keys of the identifiers in `chosen` and `rest`, sorted.
    fn keys_of(identifiers: &Identifiers) -> Vec<String> {
        let mut keys: Vec<String> = identifiers
            .chosen
            .iter()
            .chain(&identifiers.rest)
            .flat_map(|value| candidates(value))
            .filter(|candidate| !candidate.text_only)
            .map(|candidate| candidate.key)
            .collect();
        keys.sort();
        keys
    }

    /// The order of the values does not change the identifier chosen.
    #[proptest]
    fn choice_does_not_depend_on_order(
        values: Values,
        #[strategy(Just(#values.render()).prop_shuffle())] shuffled: Vec<String>,
    ) {
        let chosen = choose(values.render().iter().map(String::as_str)).chosen;
        let reordered = choose(shuffled.iter().map(String::as_str)).chosen;
        prop_assert_eq!(chosen, reordered);
    }

    /// Every identifier given is either chosen or in the rest.
    #[proptest]
    fn no_identifier_is_lost(values: Values) {
        let identifiers = choose(values.render().iter().map(String::as_str));
        prop_assert_eq!(keys_of(&identifiers), values.keys());
    }

    /// Without labels, the chosen identifier is the one with the lowest digit key, as found by
    /// splitting at every separator.
    #[proptest]
    fn choice_matches_reference_without_labels(
        #[strategy(vec(arb_identifier(), 1..6))] identifiers: Vec<String>,
        #[strategy(vec(arb_separator(), #identifiers.len()))] separators: Vec<&'static str>,
    ) {
        let mut value = String::new();
        for (identifier, separator) in identifiers.iter().zip(&separators) {
            value.push_str(identifier);
            value.push_str(separator);
        }
        let reference = value
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|part| !part.is_empty())
            .min_by(|a, b| digit_key(a).cmp(&digit_key(b)).then(a.cmp(b)))
            .map(str::to_owned);
        prop_assert_eq!(choose([value.as_str()]).chosen, reference);
    }

    /// Choosing again from the chosen identifier and the rest gives the same result.
    #[proptest]
    fn choosing_again_changes_nothing(values: Values) {
        let first = choose(values.render().iter().map(String::as_str));
        let again = choose(first.chosen.iter().chain(&first.rest).map(String::as_str));
        let sorted = |rest: &[String]| {
            let mut rest = rest.to_vec();
            rest.sort();
            rest
        };
        prop_assert_eq!(&again.chosen, &first.chosen);
        prop_assert_eq!(sorted(&again.rest), sorted(&first.rest));
    }

    /// The rest, joined with either separator the serialisers write, splits back into itself.
    #[proptest]
    fn rest_joined_splits_back(
        values: Values,
        #[strategy(select(vec!["; ", ", "]))] joiner: &'static str,
    ) {
        let rest = choose(values.render().iter().map(String::as_str)).rest;
        prop_assert_eq!(split(&rest.join(joiner)), rest);
    }

    /// Any text, including unbalanced parentheses and digits that are not ASCII, is read without
    /// panicking; every candidate is either chosen or in the rest; and the candidates of each
    /// value, joined with `; `, split back into themselves.
    #[proptest]
    fn any_text_is_read(
        #[strategy(vec(r"[()\[\] ,;\r\n0-9Xx٣\\a-]{0,24}|\PC{0,24}", 0..4))] values: Vec<String>,
    ) {
        let identifiers = choose(values.iter().map(String::as_str));
        let candidates: usize = values.iter().map(|value| split(value).len()).sum();
        prop_assert_eq!(
            identifiers.rest.len() + usize::from(candidates > 0),
            candidates
        );
        for value in &values {
            let split_once = split(value);
            prop_assert_eq!(split(&split_once.join("; ")), split_once);
        }
    }
}
