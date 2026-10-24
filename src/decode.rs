//! Decoding the bytes of an export into text for the parsers. See [`decode`] for how the encoding
//! is chosen.

use std::borrow::Cow;
use std::fmt;

use encoding_rs::Encoding as Charset;

/// The longest prefix of the input searched for an XML declaration. A declaration must end, with
/// `?>`, within it.
const DECLARATION_LIMIT: usize = 1024;

/// The character encoding that [`decode`] read its input in.
///
/// New variants may be added. [`Encoding::Other`] holds an encoding named by an XML declaration
/// that has no variant of its own, and a later release may give it one, so to recognise such an
/// encoding compare [`Encoding::name`] rather than matching on `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Encoding {
    /// UTF-8, with or without a byte-order mark.
    Utf8,
    /// UTF-16, little-endian, read from its byte-order mark.
    Utf16Le,
    /// UTF-16, big-endian, read from its byte-order mark.
    Utf16Be,
    /// Windows-1252, a superset of Latin-1 (ISO-8859-1): named in an XML declaration, or the
    /// fallback for input that is not UTF-8.
    Windows1252,
    /// Another encoding named in an XML declaration, by its WHATWG name (e.g. `"Shift_JIS"`).
    /// Only [`decode`] produces it.
    Other(&'static str),
}

impl Encoding {
    /// The WHATWG name of the encoding: `"UTF-8"`, `"UTF-16LE"`, `"UTF-16BE"`, `"windows-1252"`,
    /// or the name held by [`Encoding::Other`].
    ///
    /// ```
    /// assert_eq!(biblio::Encoding::Windows1252.name(), "windows-1252");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Windows1252 => "windows-1252",
            Self::Other(name) => name,
        }
    }

    /// The variant for `charset`, or [`Encoding::Other`] with its name.
    fn from_charset(charset: &'static Charset) -> Self {
        if charset == encoding_rs::UTF_8 {
            Self::Utf8
        } else if charset == encoding_rs::UTF_16LE {
            Self::Utf16Le
        } else if charset == encoding_rs::UTF_16BE {
            Self::Utf16Be
        } else if charset == encoding_rs::WINDOWS_1252 {
            Self::Windows1252
        } else {
            Self::Other(charset.name())
        }
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The text of an input, as returned by [`decode`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Decoded<'a> {
    /// The decoded text, without a byte-order mark. It borrows the input when the input is valid
    /// UTF-8, and is a new string otherwise. An XML declaration in it still names the original encoding,
    /// which the parsers ignore.
    pub text: Cow<'a, str>,

    /// The encoding the input was read in.
    pub encoding: Encoding,

    /// Whether some bytes were not valid in [`Decoded::encoding`] and were replaced with U+FFFD,
    /// so characters were lost: for example, UTF-16 with an odd number of bytes.
    pub replaced: bool,
}

/// Decode the bytes of an export into text for the parsers. Never fails.
///
/// Exports are not always UTF-8: older Windows exporters write Windows-1252, and an `EndNote XML`
/// file may name its encoding in its XML declaration. The encoding is chosen by trying, in order:
///
/// 1. a byte-order mark for UTF-8, UTF-16LE or UTF-16BE, which is removed from the text;
/// 2. valid UTF-8, which is kept as it is without copying;
/// 3. the encoding named in an XML declaration at the start of the input, if it reads ASCII as
///    ASCII and is not UTF-8;
/// 4. otherwise, Windows-1252, which gives a character for every byte, so nothing is replaced.
///
/// Valid UTF-8 is checked before the declaration because exporters sometimes declare the wrong
/// encoding, while text in another encoding is almost never valid UTF-8 by chance. As a result,
/// input of only ASCII is reported as UTF-8 whatever its declaration says, which gives the same
/// text, and very short text in a double-byte encoding that happens to be valid UTF-8 is read as
/// UTF-8. UTF-16 without a byte-order mark, UTF-32 and ISO-2022-JP are not detected: UTF-16 text
/// without a mark is usually valid UTF-8 with a NUL after each character, and is read that way,
/// and the UTF-32 little-endian mark is read as UTF-16 followed by a NUL.
///
/// ```
/// // "Café" in Windows-1252, as an older Windows exporter writes it.
/// let decoded = biblio::decode(b"TY  - JOUR\nTI  - Caf\xe9\nER  - \n");
/// assert_eq!(decoded.encoding, biblio::Encoding::Windows1252);
/// let parsed = biblio::ris::parse(&decoded.text).unwrap();
/// assert_eq!(parsed.records[0].title.as_deref(), Some("Café"));
/// ```
#[must_use]
pub fn decode(bytes: &[u8]) -> Decoded<'_> {
    if let Some((charset, mark_len)) = Charset::for_bom(bytes) {
        return decode_with(charset, &bytes[mark_len..]);
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return Decoded {
            text: Cow::Borrowed(text),
            encoding: Encoding::Utf8,
            replaced: false,
        };
    }

    // UTF-16 cannot be named by a declaration readable as ASCII, and UTF-8 has failed already.
    if let Some(charset) = declared_encoding(bytes)
        .filter(|charset| charset.is_ascii_compatible() && *charset != encoding_rs::UTF_8)
    {
        return decode_with(charset, bytes);
    }

    let decoded = decode_with(encoding_rs::WINDOWS_1252, bytes);
    debug_assert!(!decoded.replaced, "Windows-1252 maps every byte");
    decoded
}

/// Decode `bytes`, which carry no byte-order mark, as `charset`.
fn decode_with<'a>(charset: &'static Charset, bytes: &'a [u8]) -> Decoded<'a> {
    let (text, replaced) = charset.decode_without_bom_handling(bytes);
    Decoded {
        text,
        encoding: Encoding::from_charset(charset),
        replaced,
    }
}

/// The encoding named by the XML declaration at the start of `bytes`, if there is one.
///
/// The input must start with `<?xml` and whitespace, and the declaration must end with `?>` within
/// the first [`DECLARATION_LIMIT`] bytes. Its `encoding` attribute, preceded by whitespace, may
/// have whitespace around `=` and a value in single or double quotes. An empty, unterminated or
/// unknown value gives `None`.
fn declared_encoding(bytes: &[u8]) -> Option<&'static Charset> {
    let window = &bytes[..bytes.len().min(DECLARATION_LIMIT)];
    let rest = window.strip_prefix(b"<?xml")?;
    if !rest.first().copied().is_some_and(is_xml_space) {
        return None;
    }
    let declaration = &rest[..find(rest, b"?>")?];

    // Positions are within the whole declaration, which starts with whitespace, so a match is
    // never at index 0 and the byte before it always exists.
    let mut from = 0;
    while let Some(offset) = find(&declaration[from..], b"encoding") {
        let at = from + offset;
        let end = at + b"encoding".len();
        if is_xml_space(declaration[at - 1]) {
            return attribute_value(&declaration[end..]).and_then(Charset::for_label);
        }
        from = end;
    }
    None
}

/// The quoted value of an attribute, given the text after its name: optional whitespace, `=`,
/// optional whitespace, then a value in matching single or double quotes.
fn attribute_value(after_name: &[u8]) -> Option<&[u8]> {
    let rest = trim_xml_space(after_name).strip_prefix(b"=")?;
    let (&quote, rest) = trim_xml_space(rest).split_first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let end = rest.iter().position(|&b| b == quote)?;
    Some(&rest[..end])
}

/// `bytes` without leading XML whitespace.
fn trim_xml_space(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&b| !is_xml_space(b))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

/// Whether `byte` is XML whitespace: space, tab, carriage return or line feed.
fn is_xml_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// The position of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
