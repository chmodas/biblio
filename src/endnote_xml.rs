//! Parser and serializer for the `EndNote XML` bibliographic format.
//!
//! # References
//!
//! - [`EndNote XML` DTD](https://support.clarivate.com/Endnote/s/article/EndNote-XML-Document-Type-Definition)
//! - [Zotero `EndNote XML` translator](https://github.com/zotero/translators/blob/master/Endnote%20XML.js)

use std::collections::HashMap;
use std::ops::ControlFlow;

use quick_xml::events::{BytesCData, BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::parse_util::{check_empty, parsed, report_missing_title, strip_byte_order_mark};
use crate::{Diagnostic, Error, Parsed, PublicationDate, Record};

/// Parse `EndNote XML` text into zero or more [`Record`]s.
///
/// Handles both `<xml><records>...</records></xml>` and bare `<records>...</records>` roots. Strips
/// presentational `<style>` tags before parsing.
///
/// Every record is kept, even one without a title. Problems are reported as diagnostics:
///
/// - [`Diagnostic::MissingTitle`]: a record has no title, or an empty one.
/// - [`Diagnostic::Truncated`]: an XML syntax error, or the end of the input inside a `<record>`.
///   The records before it are kept, the record in progress is dropped, and parsing stops.
///
/// Unknown entities such as `&foo;` are dropped without a diagnostic, and a `<record>` nested
/// inside another replaces the outer one without a diagnostic.
///
/// A leading byte-order mark is ignored.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input, after any leading byte-order mark, is empty or
/// whitespace-only.
/// Returns [`Error::MalformedSyntax`] if the XML breaks off, as for [`Diagnostic::Truncated`],
/// before any record is complete.
pub fn parse(input: &str) -> Result<Parsed, Error> {
    // quick-xml would also skip the byte-order mark, but stripping it here first makes a mark
    // followed only by whitespace count as empty input, as in the other formats.
    let input = strip_byte_order_mark(input);
    check_empty(input)?;

    let (cleaned, removed_newlines) = strip_style_tags(input);
    let mut reader = Reader::from_str(&cleaned);
    let mut walker = Walker::new(&cleaned, &removed_newlines);

    loop {
        // The position before an event is where that event starts.
        let position = reader.buffer_position();
        let flow = match reader.read_event() {
            Ok(event) => walker.on_event(&event, position),
            Err(e) => walker.fail(reader.error_position(), e.to_string()),
        };
        if flow.is_break() {
            break;
        }
    }

    walker.finish()
}

/// Walks the XML events of one input, collecting records and diagnostics.
struct Walker<'a> {
    /// The input after style tags are stripped, used to turn byte positions into line numbers.
    text: &'a str,
    /// Offsets in `text` of newlines removed with style tags, in ascending order.
    removed_newlines: &'a [usize],
    /// Byte position up to which newlines have been counted.
    counted: usize,
    /// One-indexed line at `counted`.
    line: usize,
    /// Completed records.
    records: Vec<Record>,
    /// Problems found so far.
    diagnostics: Vec<Diagnostic>,
    /// The record being read, if inside a `<record>`.
    pending: Option<PendingRecord>,
    /// The current element path, to know which field is being filled.
    path: Vec<String>,
    /// Text collected for the current element.
    buf: String,
    /// The line and message of the problem that stopped parsing, if any.
    failure: Option<(usize, String)>,
}

impl<'a> Walker<'a> {
    /// Start walking `text`, whose style tags removed the newlines at `removed_newlines`.
    fn new(text: &'a str, removed_newlines: &'a [usize]) -> Self {
        Self {
            text,
            removed_newlines,
            counted: 0,
            line: 1,
            records: Vec::new(),
            diagnostics: Vec::new(),
            pending: None,
            path: Vec::new(),
            buf: String::new(),
            failure: None,
        }
    }

    /// The one-indexed line of byte `position`. Positions must not decrease between calls.
    fn line_at(&mut self, position: u64) -> usize {
        let position = usize::try_from(position)
            .unwrap_or(usize::MAX)
            .min(self.text.len());
        if position > self.counted {
            // Splitting on newlines yields one more piece than there are newlines. Bytes, not
            // chars, so a position inside a multi-byte character cannot panic.
            let newlines = self.text.as_bytes()[self.counted..position]
                .split(|&byte| byte == b'\n')
                .count()
                - 1;
            self.line += newlines;
            self.counted = position;
        }
        // Newlines removed with a style tag at or before `position` come before it.
        let removed = self
            .removed_newlines
            .iter()
            .take_while(|&&offset| offset <= position)
            .count();
        self.line += removed;
        self.removed_newlines = &self.removed_newlines[removed..];
        self.line
    }

    /// Handle one event starting at byte `position`.
    fn on_event(&mut self, event: &Event<'_>, position: u64) -> ControlFlow<()> {
        match event {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "record" {
                    self.pending = Some(PendingRecord {
                        line_start: self.line_at(position),
                        ..PendingRecord::default()
                    });
                }
                self.path.push(name);
                self.buf.clear();
            }
            Event::End(e) => {
                if e.name().as_ref() == b"record" {
                    if let Some(pending) = self.pending.take() {
                        let record = pending.finish(&mut self.diagnostics);
                        self.records.push(record);
                    }
                } else if let Some(pending) = self.pending.as_mut() {
                    pending.set_field(&self.path, &self.buf);
                }
                self.path.pop();
                self.buf.clear();
            }
            Event::Text(e) => {
                if self.pending.is_some() {
                    // quick-xml guarantees valid UTF-8 for str-based readers.
                    self.buf.push_str(std::str::from_utf8(e).unwrap_or(""));
                }
            }
            // In quick-xml 0.39+, entity references like &amp; &lt; arrive as separate GeneralRef
            // events rather than being inlined into Text.
            Event::GeneralRef(e) => {
                if self.pending.is_some() {
                    if let Ok(Some(ch)) = e.resolve_char_ref() {
                        self.buf.push(ch);
                    } else {
                        match std::str::from_utf8(e).unwrap_or("") {
                            "amp" => self.buf.push('&'),
                            "lt" => self.buf.push('<'),
                            "gt" => self.buf.push('>'),
                            "apos" => self.buf.push('\''),
                            "quot" => self.buf.push('"'),
                            _ => {}
                        }
                    }
                }
            }
            Event::Eof => {
                // Unclosed <records> or <xml> are harmless; an unclosed <record> is not.
                if let Some(pending) = &self.pending {
                    let line = pending.line_start;
                    self.failure = Some((line, "input ended inside a record".to_owned()));
                }
                return ControlFlow::Break(());
            }
            // Self-closing tags (<pages/>), comments, PIs – nothing to capture.
            _ => {}
        }
        ControlFlow::Continue(())
    }

    /// Record an XML syntax error at byte `position` and stop.
    fn fail(&mut self, position: u64, message: String) -> ControlFlow<()> {
        let line = self.line_at(position);
        self.failure = Some((line, message));
        ControlFlow::Break(())
    }

    /// Return the records and diagnostics, or an error if parsing failed before any record was
    /// complete.
    fn finish(mut self) -> Result<Parsed, Error> {
        if let Some((line, message)) = self.failure {
            if self.records.is_empty() {
                return Err(Error::MalformedSyntax { line, message });
            }
            self.diagnostics
                .push(Diagnostic::Truncated { line, message });
        }
        Ok(parsed(self.records, self.diagnostics))
    }
}

/// Serialize [`Record`]s to `EndNote XML` format.
///
/// Produces a well-formed XML document with `<xml><records>...</records></xml>` structure. Text
/// values are CDATA-escaped.
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    let mut writer = Writer::new(Vec::new());

    // XML declaration.
    let _ = writer.write_event(Event::Decl(quick_xml::events::BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        None,
    )));

    // <xml>
    let _ = writer.write_event(Event::Start(BytesStart::new("xml")));

    // <records>
    let _ = writer.write_event(Event::Start(BytesStart::new("records")));

    for record in records {
        write_record(&mut writer, record);
    }

    // </records>
    let _ = writer.write_event(Event::End(BytesEnd::new("records")));
    // </xml>
    let _ = writer.write_event(Event::End(BytesEnd::new("xml")));

    String::from_utf8(writer.into_inner()).unwrap_or_default()
}

fn write_record(writer: &mut Writer<Vec<u8>>, record: &Record) {
    let _ = writer.write_event(Event::Start(BytesStart::new("record")));

    if let Some(rn) = record.extras.get("rec-number") {
        write_simple_element(writer, "rec-number", rn);
    }

    // <titles><title>...</title></titles>
    if let Some(title) = &record.title {
        let _ = writer.write_event(Event::Start(BytesStart::new("titles")));
        write_simple_element(writer, "title", title);
        let _ = writer.write_event(Event::End(BytesEnd::new("titles")));
    }

    if !record.authors.is_empty() {
        let _ = writer.write_event(Event::Start(BytesStart::new("contributors")));
        let _ = writer.write_event(Event::Start(BytesStart::new("authors")));
        for author in &record.authors {
            write_simple_element(writer, "author", author);
        }
        let _ = writer.write_event(Event::End(BytesEnd::new("authors")));
        let _ = writer.write_event(Event::End(BytesEnd::new("contributors")));
    }

    if let Some(date) = record.date {
        let _ = writer.write_event(Event::Start(BytesStart::new("dates")));
        write_simple_element(writer, "year", &date.year.to_string());
        let _ = writer.write_event(Event::End(BytesEnd::new("dates")));
    }

    if let Some(journal) = &record.journal {
        let _ = writer.write_event(Event::Start(BytesStart::new("periodical")));
        write_simple_element(writer, "full-title", journal);
        let _ = writer.write_event(Event::End(BytesEnd::new("periodical")));
    }

    if let Some(doi) = &record.doi {
        write_simple_element(writer, "electronic-resource-num", doi);
    }

    if let Some(pages) = &record.pages {
        write_simple_element(writer, "pages", pages);
    }

    if let Some(volume) = &record.volume {
        write_simple_element(writer, "volume", volume);
    }

    if let Some(number) = &record.number {
        write_simple_element(writer, "number", number);
    }

    if let Some(abstract_text) = &record.abstract_text {
        write_simple_element(writer, "abstract", abstract_text);
    }

    if let Some(isbn) = &record.isbn {
        write_simple_element(writer, "isbn", isbn);
    }

    if let Some(label) = record.extras.get("label") {
        write_simple_element(writer, "label", label);
    }

    let _ = writer.write_event(Event::End(BytesEnd::new("record")));
}

fn write_simple_element(writer: &mut Writer<Vec<u8>>, tag: &str, value: &str) {
    let _ = writer.write_event(Event::Start(BytesStart::new(tag)));
    if let Ok(escaped) = BytesCData::new(value).escape() {
        let _ = writer.write_event(Event::Text(escaped));
    }
    let _ = writer.write_event(Event::End(BytesEnd::new(tag)));
}

// -- Style tag stripping --

/// Strip `<style ...>` and `</style>` tags from the XML.
///
/// `EndNote` wraps text content in presentational `<style face="bold" ...>` tags that carry no
/// bibliographic meaning. Removing them before parsing keeps the XML walker simple and the
/// extracted text clean.
///
/// Returns the stripped text and, for each newline removed with a tag, the offset in the stripped
/// text where the tag was, so that line numbers can still be given for the original input. After an
/// opening tag with no closing `>`, the rest of the input is kept as it is, apart from `</style>`.
fn strip_style_tags(xml: &str) -> (String, Vec<usize>) {
    let mut result = String::with_capacity(xml.len());
    let mut removed_newlines = Vec::new();
    let mut remaining = xml;
    let mut strip_opening = true;

    while let Some(start) = remaining.find('<') {
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        if let Some(rest) = remaining.strip_prefix("</style>") {
            remaining = rest;
            continue;
        }
        if strip_opening && is_style_opening(remaining) {
            if let Some(end) = remaining.find('>') {
                let newlines = remaining[..end].matches('\n').count();
                removed_newlines.extend(std::iter::repeat_n(result.len(), newlines));
                remaining = &remaining[end + 1..];
                continue;
            }
            strip_opening = false;
        }
        result.push('<');
        remaining = &remaining[1..];
    }
    result.push_str(remaining);

    (result, removed_newlines)
}

/// Whether `text` starts with an opening `<style>` tag: `<style` followed by `>`, `/`, whitespace
/// or the end of the input, but not a longer name such as `<stylesheet>`.
fn is_style_opening(text: &str) -> bool {
    text.strip_prefix("<style").is_some_and(|after| {
        after
            .bytes()
            .next()
            .is_none_or(|next| next == b'>' || next == b'/' || next.is_ascii_whitespace())
    })
}

// -- Record builder --

/// A record being read, before its fields are combined into a [`Record`].
#[derive(Default)]
struct PendingRecord {
    /// Line of the record's `<record>` tag.
    line_start: usize,
    rec_number: Option<String>,
    title: Option<String>,
    authors: Vec<String>,
    year: Option<String>,
    journal: Option<String>,
    doi: Option<String>,
    pages: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    abstract_text: Option<String>,
    isbn: Option<String>,
    label: Option<String>,
}

impl PendingRecord {
    /// Map the current element path plus text content to the appropriate field.
    ///
    /// Paths are relative to the `<record>` element. For example, the title arrives as a path
    /// `["record", "titles", "title"]` with the title text as content.
    fn set_field(&mut self, path: &[String], text: &str) {
        if text.is_empty() {
            return;
        }
        match path
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [.., "titles", "title"] => self.title = Some(text.to_owned()),
            [.., "contributors", "authors", "author"] => self.authors.push(text.to_owned()),
            [.., "dates", "year"] => self.year = Some(text.to_owned()),
            [.., "periodical", "full-title"] => self.journal = Some(text.to_owned()),
            [.., "electronic-resource-num"] => self.doi = Some(text.to_owned()),
            [.., "pages"] => self.pages = Some(text.to_owned()),
            [.., "volume"] => self.volume = Some(text.to_owned()),
            [.., "number"] => self.number = Some(text.to_owned()),
            [.., "abstract"] => self.abstract_text = Some(text.to_owned()),
            [.., "isbn"] => self.isbn = Some(text.to_owned()),
            [.., "label"] => self.label = Some(text.to_owned()),
            [.., "rec-number"] => self.rec_number = Some(text.to_owned()),
            _ => {}
        }
    }

    /// Combine the fields into a [`Record`], reporting a missing title.
    fn finish(self, diagnostics: &mut Vec<Diagnostic>) -> Record {
        report_missing_title(self.title.as_deref(), self.line_start, diagnostics);

        let date = self
            .year
            .and_then(|y| y.trim().parse::<i32>().ok())
            .map(|year| PublicationDate {
                year,
                month: None,
                day: None,
            });

        let mut extras = HashMap::new();
        if let Some(label) = self.label {
            extras.insert("label".into(), label);
        }
        if let Some(rn) = self.rec_number {
            extras.insert("rec-number".into(), rn);
        }

        Record {
            title: self.title,
            authors: self.authors,
            date,
            journal: self.journal,
            doi: self.doi,
            pages: self.pages,
            volume: self.volume,
            number: self.number,
            abstract_text: self.abstract_text,
            isbn: self.isbn,
            extras,
        }
    }
}
