use std::borrow::Cow;

use crate::Error;

/// Return [`Error::EmptyInput`] if `input` is empty or whitespace-only.
pub(crate) fn check_empty(input: &str) -> Result<(), Error> {
    if input.trim().is_empty() {
        return Err(Error::EmptyInput);
    }
    Ok(())
}

/// Normalise `\r\n` → `\n`. Returns a borrowed view when no `\r` is present
/// (the common case), avoiding allocation.
pub(crate) fn normalize_line_endings(input: &str) -> Cow<'_, str> {
    if input.contains('\r') {
        Cow::Owned(input.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(input)
    }
}

/// Append `text` to an existing `Option<String>`, separated by a space unless the existing value is
/// empty. Tag values are trimmed at the end and continuation lines are fully trimmed, so a blank
/// value is always empty, and a blank field continued on the next line becomes exactly `text`.
pub(crate) fn append_to_opt(field: &mut Option<String>, text: &str) {
    if let Some(existing) = field {
        push_continuation(existing, text);
    }
}

/// Append `text` to the last element of a slice, separated by a space unless that element is empty.
/// See [`append_to_opt`].
pub(crate) fn append_to_last(vec: &mut [String], text: &str) {
    if let Some(last) = vec.last_mut() {
        push_continuation(last, text);
    }
}

/// Append `text` to `existing`, with a separating space only when `existing` is not empty.
fn push_continuation(existing: &mut String, text: &str) {
    if !existing.is_empty() {
        existing.push(' ');
    }
    existing.push_str(text);
}
