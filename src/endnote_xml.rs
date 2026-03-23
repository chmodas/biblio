//! Parser and serializer for the `EndNote XML` bibliographic format.
//!
//! # References
//!
//! - [`EndNote XML` DTD](https://support.clarivate.com/Endnote/s/article/EndNote-XML-Document-Type-Definition)
//! - [Zotero `EndNote XML` translator](https://github.com/zotero/translators/blob/master/Endnote%20XML.js)

use std::collections::HashMap;

use quick_xml::events::{BytesCData, BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::parse_util::check_empty;
use crate::{Error, PublicationDate, Record};

/// Parse `EndNote XML` text into zero or more [`Record`]s.
///
/// Handles both `<xml><records>...</records></xml>` and bare
/// `<records>...</records>` roots. Strips presentational `<style>` tags
/// before parsing.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input is empty or whitespace-only.
/// Returns [`Error::MalformedSyntax`] if the XML cannot be parsed.
/// Returns [`Error::MissingRequiredField`] if a record has no title.
pub fn parse(input: &str) -> Result<Vec<Record>, Error> {
    check_empty(input)?;

    let cleaned = strip_style_tags(input);
    let mut reader = Reader::from_str(&cleaned);

    let mut records = Vec::new();
    let mut builder: Option<RecordBuilder> = None;
    // Track the current element path to know which field we're filling.
    let mut path: Vec<String> = Vec::new();
    let mut buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "record" {
                    builder = Some(RecordBuilder::default());
                }
                path.push(name);
                buf.clear();
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "record" {
                    if let Some(b) = builder.take() {
                        records.push(b.finish()?);
                    }
                } else if let Some(b) = builder.as_mut() {
                    b.set_field(&path, &buf);
                }
                path.pop();
                buf.clear();
            }
            Ok(Event::Text(e)) => {
                if builder.is_some() {
                    // quick-xml guarantees valid UTF-8 for str-based readers.
                    let raw = std::str::from_utf8(&e).unwrap_or("");
                    buf.push_str(raw);
                }
            }
            // In quick-xml 0.39+, entity references like &amp; &lt; arrive as
            // separate GeneralRef events rather than being inlined into Text.
            Ok(Event::GeneralRef(e)) => {
                if builder.is_some() {
                    if let Ok(Some(ch)) = e.resolve_char_ref() {
                        buf.push(ch);
                    } else {
                        let name = std::str::from_utf8(&e).unwrap_or("");
                        match name {
                            "amp" => buf.push('&'),
                            "lt" => buf.push('<'),
                            "gt" => buf.push('>'),
                            "apos" => buf.push('\''),
                            "quot" => buf.push('"'),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            // Self-closing tags (<pages/>), comments, PIs – nothing to capture.
            Ok(_) => {}
            Err(e) => {
                return Err(Error::MalformedSyntax {
                    line: 0,
                    message: e.to_string(),
                });
            }
        }
    }

    Ok(records)
}

/// Serialize [`Record`]s to `EndNote XML` format.
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
    let _ = writer.write_event(Event::Start(BytesStart::new("titles")));
    write_simple_element(writer, "title", &record.title);
    let _ = writer.write_event(Event::End(BytesEnd::new("titles")));

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

/// Strip `<style ...>` opening tags and `</style>` closing tags from the XML.
fn strip_style_tags(xml: &str) -> String {
    let mut result = String::with_capacity(xml.len());
    let mut remaining = xml;

    // Strip opening <style ...> tags. Only match `<style>` or `<style ` (with
    // attributes), not tags that merely start with "style" (e.g. `<stylesheet>`).
    while let Some(start) = remaining.find("<style") {
        let after = start + "<style".len();
        if after < remaining.len() {
            let next = remaining.as_bytes()[after];
            if next != b'>' && !next.is_ascii_whitespace() && next != b'/' {
                result.push_str(&remaining[..=after]);
                remaining = &remaining[after + 1..];
                continue;
            }
        }
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        if let Some(end) = remaining.find('>') {
            remaining = &remaining[end + 1..];
        } else {
            break;
        }
    }
    result.push_str(remaining);

    result.replace("</style>", "")
}

// -- Record builder --

#[derive(Default)]
struct RecordBuilder {
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

impl RecordBuilder {
    /// Map the current element path plus text content to the appropriate field.
    ///
    /// Paths are relative to the `<record>` element. For example, the title arrives as a path
    /// `["record", "titles", "title"]` with the title text.
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

    fn finish(self) -> Result<Record, Error> {
        let title = self.title.ok_or(Error::MissingRequiredField {
            tag: "title",
            line: 0,
        })?;

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

        Ok(Record {
            title,
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
        })
    }
}
