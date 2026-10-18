//! Guards that keep hostile `BibTeX` input from overflowing the stack or running without end.
//!
//! The underlying parser resolves LaTeX commands by recursion, with no limit on depth. A stack
//! overflow aborts the process and cannot be caught, so the guards run on the raw bibliography,
//! before anything is resolved, and drop what would recurse too deeply. Each guard is iterative, so
//! the guard itself cannot overflow.

use std::collections::HashSet;

use biblatex::{Pair, RawChunk, RawEntry, Spanned};

use crate::Diagnostic;

/// The deepest brace nesting allowed in a field value or `@string` definition.
///
/// The parser recurses for each command argument (`\x{…}`), using a few stack frames per level. A
/// 2 MiB thread in a debug build survives about 700 levels, even at the end of a long chain of
/// `@string` definitions, so this leaves a wide margin.
pub(super) const MAX_NESTING: usize = 128;

/// The deepest brace nesting in `text`, counting every `{` and `}` and never going below zero.
///
/// Backslashes are not treated as escapes, because the parser does not treat them as escapes when
/// it finds a command's argument. This counts plain groups and escaped braces too, so it is an
/// upper bound on how deeply the parser recurses.
fn nesting(text: &str) -> usize {
    let mut depth = 0_usize;
    let mut deepest = 0;
    for byte in text.bytes() {
        match byte {
            b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// Whether any literal part of a field value nests deeper than [`MAX_NESTING`]. Each part is parsed
/// on its own, so their depths do not add up; references to `@string`s are judged where the
/// `@string` is defined.
fn nests_too_deeply(value: &[Spanned<RawChunk<'_>>]) -> bool {
    value.iter().any(|chunk| match chunk.v {
        RawChunk::Normal(text) => nesting(text) > MAX_NESTING,
        RawChunk::Abbreviation(_) => false,
    })
}

/// Remove `@string` definitions that would make the parser recurse too deeply, reporting each one.
///
/// `lines` holds the line of each definition and is kept aligned with `strings`. The parser uses a
/// name's first definition, so only that one is judged; when it nests too deeply, every definition
/// of the name is removed, so that a later one cannot silently take its place. Fields that use the
/// name then fail as an unknown abbreviation, unless it is a month name, which falls back to the
/// month.
pub(super) fn strings(
    strings: &mut Vec<Pair<'_>>,
    lines: &mut Vec<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut removed: HashSet<&str> = HashSet::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for (pair, &line) in strings.iter().zip(lines.iter()) {
        let name = pair.key.v;
        if !seen.insert(name) {
            continue;
        }
        if nests_too_deeply(&pair.value.v) {
            removed.insert(name);
            diagnostics.push(Diagnostic::SkippedEntry {
                line,
                message: format!("@string {name} is nested too deeply"),
            });
        }
    }
    if removed.is_empty() {
        return;
    }
    let keep: Vec<bool> = strings
        .iter()
        .map(|pair| !removed.contains(&pair.key.v))
        .collect();
    retain_aligned(strings, lines, &keep);
}

/// Remove entries that would make the parser recurse too deeply, reporting each one.
///
/// `lines` holds the line of each entry and is kept aligned with `entries`.
pub(super) fn entries(
    entries: &mut Vec<Spanned<RawEntry<'_>>>,
    lines: &mut Vec<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let keep: Vec<bool> = entries
        .iter()
        .zip(lines.iter())
        .map(|(entry, &line)| {
            let deep = entry
                .v
                .fields
                .iter()
                .any(|field| nests_too_deeply(&field.value.v));
            if deep {
                diagnostics.push(Diagnostic::SkippedEntry {
                    line,
                    message: "fields nested too deeply".to_owned(),
                });
            }
            !deep
        })
        .collect();
    retain_aligned(entries, lines, &keep);
}

/// Keep the items of `items`, and the matching items of `lines`, for which `keep` is true.
fn retain_aligned<T>(items: &mut Vec<T>, lines: &mut Vec<usize>, keep: &[bool]) {
    let mut flags = keep.iter();
    items.retain(|_| *flags.next().unwrap_or(&true));
    let mut flags = keep.iter();
    lines.retain(|_| *flags.next().unwrap_or(&true));
}
