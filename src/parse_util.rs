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

/// Append `text` (with a space separator) to an existing `Option<String>`.
pub(crate) fn append_to_opt(field: &mut Option<String>, text: &str) {
    if let Some(existing) = field {
        existing.push(' ');
        existing.push_str(text);
    }
}

/// Append `text` (with a space separator) to the last element of a slice.
pub(crate) fn append_to_last(vec: &mut [String], text: &str) {
    if let Some(last) = vec.last_mut() {
        last.push(' ');
        last.push_str(text);
    }
}
