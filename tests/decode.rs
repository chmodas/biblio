use std::borrow::Cow;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use test_strategy::{Arbitrary, proptest};

use biblio::{Decoded, Encoding, decode};

/// Whether the decoded text borrows the input rather than copying it.
fn is_borrowed(decoded: &Decoded<'_>) -> bool {
    matches!(decoded.text, Cow::Borrowed(_))
}

/// `text` encoded as UTF-16 in the given byte order, behind its byte-order mark.
fn utf16_with_mark(text: &str, big_endian: bool) -> Vec<u8> {
    let mut bytes = if big_endian {
        vec![0xFE, 0xFF]
    } else {
        vec![0xFF, 0xFE]
    };
    for unit in text.encode_utf16() {
        let pair = if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        };
        bytes.extend_from_slice(&pair);
    }
    bytes
}

/// An XML declaration naming `label`, followed by `body`.
fn declared(label: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("<?xml version=\"1.0\" encoding=\"{label}\"?>").into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

// -- UTF-8 --

/// Valid UTF-8 is kept as it is, without copying.
#[test]
fn utf8_borrowed_unchanged() {
    let decoded = decode("TI  - Café".as_bytes());
    assert_eq!(decoded.text, "TI  - Café");
    assert_eq!(decoded.encoding, Encoding::Utf8);
    assert!(!decoded.replaced);
    assert!(is_borrowed(&decoded));
}

/// Empty input gives empty text.
#[test]
fn empty_input_gives_empty_text() {
    let decoded = decode(b"");
    assert_eq!(decoded.text, "");
    assert_eq!(decoded.encoding, Encoding::Utf8);
    assert!(is_borrowed(&decoded));
}

/// A UTF-8 byte-order mark is removed, and the rest is still borrowed.
#[test]
fn utf8_mark_removed() {
    let decoded = decode(b"\xEF\xBB\xBFTY  - JOUR");
    assert_eq!(decoded.text, "TY  - JOUR");
    assert_eq!(decoded.encoding, Encoding::Utf8);
    assert!(is_borrowed(&decoded));

    let mark_only = decode(b"\xEF\xBB\xBF");
    assert_eq!(mark_only.text, "");
    assert!(is_borrowed(&mark_only));
}

/// After a UTF-8 byte-order mark, malformed bytes are replaced rather than read as Windows-1252.
#[test]
fn malformed_utf8_after_mark_replaced() {
    let decoded = decode(b"\xEF\xBB\xBFa\xFFb");
    assert_eq!(decoded.text, "a\u{FFFD}b");
    assert_eq!(decoded.encoding, Encoding::Utf8);
    assert!(decoded.replaced);
}

// -- UTF-16 --

/// UTF-16 in either byte order is read from its byte-order mark.
#[test]
fn utf16_read_from_mark() {
    let little_bytes = utf16_with_mark("Café ✓", false);
    let little = decode(&little_bytes);
    assert_eq!(little.text, "Café ✓");
    assert_eq!(little.encoding, Encoding::Utf16Le);
    assert!(!little.replaced);

    let big_bytes = utf16_with_mark("Café ✓", true);
    let big = decode(&big_bytes);
    assert_eq!(big.text, "Café ✓");
    assert_eq!(big.encoding, Encoding::Utf16Be);
}

/// A UTF-16 byte-order mark alone gives empty text.
#[test]
fn utf16_mark_alone_gives_empty_text() {
    assert_eq!(decode(b"\xFF\xFE").text, "");
    assert_eq!(decode(b"\xFE\xFF").text, "");
}

/// UTF-16 with an odd number of bytes, or an unpaired surrogate, is replaced in part.
#[test]
fn malformed_utf16_replaced() {
    let odd = decode(b"\xFF\xFEa\x00b");
    assert_eq!(odd.text, "a\u{FFFD}");
    assert!(odd.replaced);

    let surrogate = decode(b"\xFF\xFE\x00\xD8a\x00");
    assert_eq!(surrogate.text, "\u{FFFD}a");
    assert!(surrogate.replaced);
}

/// The UTF-32 little-endian mark starts with the UTF-16 one, so it is read as UTF-16 with a NUL.
#[test]
fn utf32_mark_read_as_utf16() {
    let decoded = decode(b"\xFF\xFE\x00\x00a\x00");
    assert_eq!(decoded.encoding, Encoding::Utf16Le);
    assert_eq!(decoded.text, "\0a");
}

/// UTF-16 without a byte-order mark is not detected: ASCII text in it is valid UTF-8 with a NUL
/// after each character, and is read that way.
#[test]
fn utf16_without_mark_read_as_utf8() {
    let decoded = decode(b"T\x00Y\x00");
    assert_eq!(decoded.encoding, Encoding::Utf8);
    assert_eq!(decoded.text, "T\0Y\0");
}

// -- Windows-1252 --

/// Input that is not UTF-8 and names no encoding is read as Windows-1252, a superset of Latin-1.
#[test]
fn latin1_read_as_windows_1252() {
    let decoded = decode(b"\xA9 2001 Elsevier");
    assert_eq!(decoded.text, "© 2001 Elsevier");
    assert_eq!(decoded.encoding, Encoding::Windows1252);
    assert!(!decoded.replaced);
}

/// Windows-1252 characters outside Latin-1, such as curly quotes, are read too.
#[test]
fn windows_1252_curly_quotes() {
    assert_eq!(decode(b"\x93quoted\x94").text, "\u{201C}quoted\u{201D}");
}

/// Every byte has a character in Windows-1252, so nothing is replaced.
#[test]
fn windows_1252_never_replaces() {
    assert_eq!(decode(b"\xFF").text, "\u{FF}");
    let undefined = decode(b"\x81");
    assert_eq!(undefined.text, "\u{81}");
    assert!(!undefined.replaced);
}

// -- XML declarations --

/// The encoding named in an XML declaration is used for input that is not UTF-8.
#[test]
fn declared_windows_1252_used() {
    let input = declared("windows-1252", b"<title>Caf\xE9</title>");
    let decoded = decode(&input);
    assert!(decoded.text.ends_with("<title>Café</title>"));
    assert_eq!(decoded.encoding, Encoding::Windows1252);
}

/// Labels that name Latin-1 are read as Windows-1252, its superset.
#[test]
fn latin1_labels_give_windows_1252() {
    for label in ["ISO-8859-1", "latin1", "us-ascii"] {
        assert_eq!(
            decode(&declared(label, b"\xE9")).encoding,
            Encoding::Windows1252,
            "{label}"
        );
    }
}

/// A double-byte encoding without a variant of its own is reported by name.
#[test]
fn declared_shift_jis_reported_as_other() {
    let input = declared("Shift_JIS", b"\x93\xFA\x96\x7B");
    let decoded = decode(&input);
    assert!(decoded.text.ends_with("日本"));
    assert_eq!(decoded.encoding, Encoding::Other("Shift_JIS"));
    assert!(!decoded.replaced);

    // A lead byte without a valid second byte is replaced.
    let malformed_input = declared("Shift_JIS", b"\x81\x20");
    let malformed = decode(&malformed_input);
    assert_eq!(malformed.encoding, Encoding::Other("Shift_JIS"));
    assert!(malformed.replaced);
}

/// `x-user-defined` maps every high byte to a private-use character, so nothing is replaced.
#[test]
fn declared_x_user_defined_used() {
    let input = declared("x-user-defined", b"\xE9");
    let decoded = decode(&input);
    assert_eq!(decoded.encoding, Encoding::Other("x-user-defined"));
    assert!(!decoded.replaced);
}

/// A declaration that names UTF-8 on input that is not UTF-8 is wrong, and Windows-1252 is used.
#[test]
fn declared_utf8_on_invalid_bytes_falls_back() {
    let input = declared("UTF-8", b"\xE9");
    let decoded = decode(&input);
    assert_eq!(decoded.encoding, Encoding::Windows1252);
    assert!(decoded.text.ends_with('é'));
}

/// A declaration readable as ASCII cannot be in an encoding that does not read ASCII as ASCII, so
/// such a label is ignored.
#[test]
fn declared_non_ascii_encodings_ignored() {
    for label in ["UTF-16", "iso-2022-kr", "iso-2022-jp"] {
        assert_eq!(
            decode(&declared(label, b"\xE9")).encoding,
            Encoding::Windows1252,
            "{label}"
        );
    }
}

/// The declaration may use single quotes, whitespace around `=`, other attributes before
/// `encoding`, and any case in the label.
#[test]
fn declaration_syntax_variants_read() {
    for declaration in [
        "<?xml version='1.0' encoding='Shift_JIS'?>",
        "<?xml version=\"1.0\" encoding = \"Shift_JIS\" ?>",
        "<?xml version=\"1.0\" standalone=\"yes\" encoding=\"Shift_JIS\"?>",
        "<?xml\tencoding=\"SHIFT_JIS\"?>",
        "<?xml version=\"1.0\"\r\n  encoding\t=\t'shift_jis'?>",
    ] {
        let mut bytes = declaration.as_bytes().to_vec();
        bytes.extend_from_slice(b"\x93\xFA");
        assert_eq!(
            decode(&bytes).encoding,
            Encoding::Other("Shift_JIS"),
            "{declaration}"
        );
    }
}

/// Text that only looks like a declaration, or a declaration whose encoding cannot be read, names
/// no encoding.
#[test]
fn malformed_declarations_ignored() {
    for declaration in [
        "<?xml-stylesheet href=\"a\" encoding=\"Shift_JIS\"?>",
        "<?xml version=\"1.0\" xencoding=\"Shift_JIS\"?>",
        "<?xml version=\"1.0\" encoding=\"\"?>",
        "<?xml version=\"1.0\" encoding=\"Shift_JIS?>",
        "<?xml version=\"1.0\" encoding=\"Shift_JIS'?>",
        "<?xml version=\"1.0\" encoding=Shift_JIS?>",
        "<?xml version=\"1.0\" encoding\"Shift_JIS\"?>",
        "<?xml version=\"1.0\" encoding=\"no-such-encoding\"?>",
        "<?xml version=\"1.0\"?> encoding=\"Shift_JIS\"",
        "<?xml version=\"1.0\" encoding=\"Shift_JIS\"",
        " <?xml version=\"1.0\" encoding=\"Shift_JIS\"?>",
        "<?xml",
    ] {
        let mut bytes = declaration.as_bytes().to_vec();
        bytes.extend_from_slice(b"\x93\xFA");
        assert_eq!(
            decode(&bytes).encoding,
            Encoding::Windows1252,
            "{declaration}"
        );
    }
}

/// `encoding` right after a rejected match is still checked, without reading before the start of
/// the declaration. Regression test: `xencodingencoding` used to panic.
#[test]
fn repeated_encoding_tokens_do_not_panic() {
    let cases = [
        (
            "<?xml xencodingencoding=\"Shift_JIS\"?>",
            Encoding::Windows1252,
        ),
        (
            "<?xml xencodingencodingencoding=\"Shift_JIS\"?>",
            Encoding::Windows1252,
        ),
        (
            "<?xml version=\"1.0\" xencoding encoding=\"Shift_JIS\"?>",
            Encoding::Other("Shift_JIS"),
        ),
        (
            "<?xml version=\"xencoding\" encoding=\"Shift_JIS\"?>",
            Encoding::Other("Shift_JIS"),
        ),
    ];
    for (declaration, expected) in cases {
        let mut bytes = declaration.as_bytes().to_vec();
        bytes.extend_from_slice(b"\x93\xFA");
        assert_eq!(decode(&bytes).encoding, expected, "{declaration}");
    }
}

/// A declaration must end within the first 1024 bytes.
#[test]
fn declaration_must_end_within_1024_bytes() {
    let with_end_at = |end: usize| {
        let head = "<?xml version=\"1.0\" encoding=\"Shift_JIS\"";
        let padding = " ".repeat(end - head.len() - 2);
        let mut bytes = format!("{head}{padding}?>").into_bytes();
        assert_eq!(bytes.len(), end);
        bytes.extend_from_slice(b"\x93\xFA");
        decode(&bytes).encoding
    };
    assert_eq!(with_end_at(1024), Encoding::Other("Shift_JIS"));
    assert_eq!(with_end_at(1025), Encoding::Windows1252);
}

/// Valid UTF-8 is checked before the declaration, so ASCII input is reported as UTF-8 whatever it
/// declares; the text is the same either way.
#[test]
fn ascii_with_declaration_reported_as_utf8() {
    let input = declared("windows-1252", b"<xml/>");
    let decoded = decode(&input);
    assert_eq!(decoded.encoding, Encoding::Utf8);
    assert!(is_borrowed(&decoded));
}

// -- Encoding --

/// Every encoding has its WHATWG name, which is also how it is displayed.
#[test]
fn encoding_names() {
    let cases = [
        (Encoding::Utf8, "UTF-8"),
        (Encoding::Utf16Le, "UTF-16LE"),
        (Encoding::Utf16Be, "UTF-16BE"),
        (Encoding::Windows1252, "windows-1252"),
        (Encoding::Other("Shift_JIS"), "Shift_JIS"),
    ];
    for (encoding, name) in cases {
        assert_eq!(encoding.name(), name);
        assert_eq!(encoding.to_string(), name);
    }
}

// -- End to end --

/// A Windows-1252 `RIS` export parses with its accented characters once decoded.
#[test]
fn windows_1252_ris_parses_after_decoding() {
    let decoded =
        decode(b"TY  - JOUR\r\nTI  - Caf\xE9 \x93society\x94\r\nAU  - M\xFCller, J\r\nER  - \r\n");
    let record = biblio::ris::parse(&decoded.text).unwrap().records.remove(0);
    assert_eq!(record.title.as_deref(), Some("Café “society”"));
    assert_eq!(record.authors, vec!["Müller, J"]);
}

/// `EndNote XML` that declares Latin-1 parses once decoded, although its declaration still names
/// Latin-1.
#[test]
fn latin1_endnote_xml_parses_after_decoding() {
    let input = declared(
        "ISO-8859-1",
        b"<xml><records><record><titles><title>Caf\xE9</title></titles></record></records></xml>",
    );
    let decoded = decode(&input);
    let record = biblio::endnote_xml::parse(&decoded.text)
        .unwrap()
        .records
        .remove(0);
    assert_eq!(record.title.as_deref(), Some("Café"));
}

/// `EndNote XML` that declares a double-byte encoding parses once decoded, although its
/// declaration still names that encoding.
#[test]
fn shift_jis_endnote_xml_parses_after_decoding() {
    let input = declared(
        "Shift_JIS",
        b"<xml><records><record><titles><title>\x93\xFA\x96\x7B</title></titles></record></records></xml>",
    );
    let decoded = decode(&input);
    let record = biblio::endnote_xml::parse(&decoded.text)
        .unwrap()
        .records
        .remove(0);
    assert_eq!(record.title.as_deref(), Some("日本"));
}

// -- Properties --

/// Any UTF-8 text not starting with a byte-order mark decodes to itself, without copying.
#[proptest]
fn utf8_decodes_to_itself(#[filter(!#text.starts_with('\u{feff}'))] text: String) {
    let decoded = decode(text.as_bytes());
    prop_assert_eq!(&decoded.text, &text);
    prop_assert_eq!(decoded.encoding, Encoding::Utf8);
    prop_assert!(!decoded.replaced);
    prop_assert!(is_borrowed(&decoded));
}

/// Any text written as UTF-16 behind a byte-order mark decodes to itself.
#[proptest]
fn utf16_decodes_to_itself(text: String, big_endian: bool) {
    let bytes = utf16_with_mark(&text, big_endian);
    let decoded = decode(&bytes);
    prop_assert_eq!(&decoded.text, &text);
    let expected = if big_endian {
        Encoding::Utf16Be
    } else {
        Encoding::Utf16Le
    };
    prop_assert_eq!(decoded.encoding, expected);
    prop_assert!(!decoded.replaced);
}

/// An XML declaration in one of its many spellings, followed by a body that is not UTF-8.
#[derive(Arbitrary, Debug)]
struct XmlDeclaration {
    /// An encoding label that reads ASCII as ASCII and is not UTF-8.
    #[strategy(select(vec![
        "windows-1252", "iso-8859-2", "iso-8859-15", "koi8-r", "windows-1251", "macintosh",
        "shift_jis", "euc-jp", "euc-kr", "gbk", "gb18030", "big5",
    ]))]
    label: &'static str,
    /// Whether the label is written in upper case.
    upper_case: bool,
    /// The quote around the attribute values.
    #[strategy(select(vec!['"', '\'']))]
    quote: char,
    /// Whitespace before `=`.
    #[strategy(select(vec!["", " ", "\t", "\r\n "]))]
    before_equals: &'static str,
    /// Whitespace after `=`.
    #[strategy(select(vec!["", " ", "\n"]))]
    after_equals: &'static str,
    /// Whether a `version` attribute comes first.
    version: bool,
    /// Whether a `standalone` attribute comes before `encoding`, rather than after it.
    standalone_first: bool,
    /// The body after the declaration. A `0xFF` byte is added in front, which never occurs in
    /// UTF-8, so the input is not valid UTF-8.
    #[strategy(vec(any::<u8>(), 0..64))]
    body: Vec<u8>,
}

impl XmlDeclaration {
    /// The bytes of the declaration and its body.
    fn bytes(&self) -> Vec<u8> {
        let q = self.quote;
        let label = if self.upper_case {
            self.label.to_ascii_uppercase()
        } else {
            self.label.to_owned()
        };
        let version = if self.version {
            format!(" version={q}1.0{q}")
        } else {
            String::new()
        };
        let standalone = format!(" standalone={q}no{q}");
        let encoding = format!(
            " encoding{}={}{q}{label}{q}",
            self.before_equals, self.after_equals
        );
        let attributes = if self.standalone_first {
            format!("{version}{standalone}{encoding}")
        } else {
            format!("{version}{encoding}{standalone}")
        };
        let mut bytes = format!("<?xml{attributes}?>").into_bytes();
        bytes.push(0xFF);
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

/// Input that is not UTF-8 is decoded in the encoding its declaration names, exactly as that
/// encoding decodes it.
#[proptest]
fn declared_encoding_matches_reference(input: XmlDeclaration) {
    let bytes = input.bytes();
    let charset = encoding_rs::Encoding::for_label(input.label.as_bytes()).unwrap();
    let (expected, had_errors) = charset.decode_without_bom_handling(&bytes);

    let decoded = decode(&bytes);
    prop_assert_eq!(decoded.encoding.name(), charset.name());
    prop_assert_eq!(decoded.text, expected);
    prop_assert_eq!(decoded.replaced, had_errors);
}

/// Arbitrary bytes, behind a prefix that steers them into each way of decoding.
#[derive(Arbitrary, Debug)]
struct ArbitraryInput {
    /// Nothing, a byte-order mark, or an XML declaration.
    #[strategy(select(vec![
        &b""[..],
        b"\xEF\xBB\xBF",
        b"\xFF\xFE",
        b"\xFE\xFF",
        b"<?xml version=\"1.0\" encoding=\"shift_jis\"?>",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    ]))]
    prefix: &'static [u8],
    /// The bytes after the prefix.
    #[strategy(vec(any::<u8>(), 0..200))]
    body: Vec<u8>,
}

/// Whatever the input, decoding never panics, and what it reports holds: replaced bytes leave a
/// U+FFFD, Windows-1252 maps back to the input byte for byte, and borrowed text is the input or its
/// end.
#[proptest]
fn decoding_invariants(input: ArbitraryInput) {
    let bytes = [input.prefix, &input.body].concat();
    let decoded = decode(&bytes);

    if decoded.replaced {
        prop_assert!(decoded.text.contains(char::REPLACEMENT_CHARACTER));
    }
    if decoded.encoding == Encoding::Windows1252 {
        prop_assert!(!decoded.replaced);
        let (encoded, _, unmappable) = encoding_rs::WINDOWS_1252.encode(&decoded.text);
        prop_assert!(!unmappable);
        prop_assert_eq!(&encoded[..], &bytes[..]);
    }
    if is_borrowed(&decoded) {
        prop_assert!(bytes.ends_with(decoded.text.as_bytes()));
    }
    let has_mark = encoding_rs::Encoding::for_bom(&bytes).is_some();
    if decoded.encoding == Encoding::Utf8 && !has_mark && !decoded.replaced {
        prop_assert_eq!(decoded.text.as_bytes(), &bytes[..]);
    }
}

/// The inside of an XML declaration made of pieces of its syntax in any order, so that names,
/// `=`, quotes and labels appear where a well-formed declaration never has them.
#[derive(Arbitrary, Debug)]
struct ScrambledDeclaration {
    /// The pieces between `<?xml ` and `?>`.
    #[strategy(vec(select(vec![
        &b"encoding"[..], b"xencoding", b"=", b"\"", b"'", b" ", b"\t", b"\r\n", b"Shift_JIS",
        b"latin1", b"UTF-8", b"utf-16", b"version", b"1.0", b"?", b">", b"\x00", b"\xE9",
    ]), 0..16))]
    pieces: Vec<&'static [u8]>,
    /// The bytes after the declaration, behind a `0xFF` that makes the input invalid UTF-8.
    #[strategy(vec(any::<u8>(), 0..32))]
    body: Vec<u8>,
}

/// However the inside of a declaration is scrambled, decoding never panics, and reports either an
/// encoding it names or the Windows-1252 fallback.
#[proptest]
fn scrambled_declarations_never_panic(input: ScrambledDeclaration) {
    let bytes = [
        &b"<?xml "[..],
        &input.pieces.concat(),
        b"?>\xFF",
        &input.body,
    ]
    .concat();
    let decoded = decode(&bytes);
    if decoded.encoding == Encoding::Windows1252 {
        prop_assert!(!decoded.replaced);
    }
    prop_assert_ne!(decoded.encoding, Encoding::Utf8);
}

/// Decoding the bytes of decoded text again gives the same text, unless it starts with what reads
/// as a byte-order mark.
#[proptest]
fn decoding_is_idempotent(input: ArbitraryInput) {
    let bytes = [input.prefix, &input.body].concat();
    let decoded = decode(&bytes);
    if !decoded.text.starts_with('\u{feff}') {
        let again = decode(decoded.text.as_bytes());
        prop_assert_eq!(&again.text, &decoded.text);
    }
}
