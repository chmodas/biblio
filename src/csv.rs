//! Parser and serializer for bibliographic records in CSV.
//!
//! A CSV export has a header row naming its columns and one row per record. Three column layouts
//! are read and written; see [`Layout`] for their columns. [`parse`] detects the layout from the
//! header row, and [`parse_as`] reads a given one.
//!
//! # References
//!
//! - [RFC 4180: Common Format and MIME Type for CSV Files](https://www.rfc-editor.org/rfc/rfc4180)
//! - [Zotero: exporting to CSV](https://www.zotero.org/support/kb/exporting)

use std::collections::{BTreeSet, HashMap};

use crate::parse_util::{
    check_empty, non_blank, non_blank_values, own_extras, parsed, push_extra, report_missing_title,
    strip_byte_order_mark,
};
use crate::{Diagnostic, Error, Parsed, PublicationDate, Record};

/// The column layout of a CSV file.
///
/// Values within one cell of a list column (authors, keywords, and, in the generic layout, links)
/// are separated by `;`. Every column without a field is kept in [`Record::extras`] as
/// `"csv:{header}"`, with the header as written.
///
/// New layouts may be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Layout {
    /// Columns named after the record's fields, matched in any case and ignoring surrounding
    /// whitespace: `title`, `author` (or `authors`), `year`, `date` (`YYYY-MM-DD`, preferred over
    /// `year`), `journal`, `doi`, `pages`, `volume`, `number`, `abstract` (or `abstract_text`),
    /// `isbn`, `source` (or `database`), `label`, `keywords` and `url` (or `urls`). Links are a
    /// list.
    Generic,
    /// Zotero's CSV export: `Title`, `Author`, `Date` (preferred over `Publication Year`),
    /// `Publication Title`, `DOI`, `Pages`, `Volume`, `Issue`, `Abstract Note`, `ISBN` (preferred
    /// over `ISSN`), `Manual Tags` and `Url`, which holds one link. `Key` and `Item Type` are kept
    /// in extras and written back in their own columns.
    Zotero,
    /// The `SyRF` import layout: `Title`, `Authors`, `Year`, `PublicationName`, `DOI`, `Abstract`,
    /// `Keywords` and `Url`, which holds one link. `AlternateName`, `AuthorAddress`,
    /// `ReferenceType`, `PdfRelativePath` and `CustomId` are kept in extras and written back in
    /// their own columns. There is no column for the month or day.
    Syrf,
}

/// A record field that a column is read into or written from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    /// A list of authors.
    Authors,
    /// A date in the form `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, preferred over [`Field::Year`].
    IsoDate,
    /// A year alone.
    Year,
    Journal,
    Doi,
    Pages,
    Volume,
    Number,
    Abstract,
    /// The identifier, preferred over [`Field::Issn`].
    Isbn,
    /// Zotero's `ISSN` column, read when `ISBN` is blank.
    Issn,
    Database,
    Label,
    /// A list of keywords.
    Keywords,
    /// A list of links.
    Urls,
    /// A single link.
    Link,
}

impl Field {
    /// The number of fields for arrays indexed by field.
    const COUNT: usize = Self::Link as usize + 1;
}

/// What a column of a layout writes: a field, or the value of the extra with its name.
#[derive(Clone, Copy)]
enum Source {
    Field(Field),
    PassThrough,
}

/// The columns the generic layout writes, in order.
const GENERIC_COLUMNS: [(&str, Source); 15] = [
    ("title", Source::Field(Field::Title)),
    ("author", Source::Field(Field::Authors)),
    ("year", Source::Field(Field::Year)),
    ("date", Source::Field(Field::IsoDate)),
    ("journal", Source::Field(Field::Journal)),
    ("doi", Source::Field(Field::Doi)),
    ("pages", Source::Field(Field::Pages)),
    ("volume", Source::Field(Field::Volume)),
    ("number", Source::Field(Field::Number)),
    ("abstract", Source::Field(Field::Abstract)),
    ("isbn", Source::Field(Field::Isbn)),
    ("source", Source::Field(Field::Database)),
    ("label", Source::Field(Field::Label)),
    ("keywords", Source::Field(Field::Keywords)),
    ("url", Source::Field(Field::Urls)),
];

/// The columns the Zotero layout reads and writes, in the order it writes them.
const ZOTERO_COLUMNS: [(&str, Source); 16] = [
    ("Key", Source::PassThrough),
    ("Item Type", Source::PassThrough),
    ("Publication Year", Source::Field(Field::Year)),
    ("Author", Source::Field(Field::Authors)),
    ("Title", Source::Field(Field::Title)),
    ("Publication Title", Source::Field(Field::Journal)),
    ("ISBN", Source::Field(Field::Isbn)),
    ("ISSN", Source::Field(Field::Issn)),
    ("DOI", Source::Field(Field::Doi)),
    ("Url", Source::Field(Field::Link)),
    ("Abstract Note", Source::Field(Field::Abstract)),
    ("Date", Source::Field(Field::IsoDate)),
    ("Pages", Source::Field(Field::Pages)),
    ("Issue", Source::Field(Field::Number)),
    ("Volume", Source::Field(Field::Volume)),
    ("Manual Tags", Source::Field(Field::Keywords)),
];

/// The columns the `SyRF` layout reads and writes, in the order it writes them.
const SYRF_COLUMNS: [(&str, Source); 13] = [
    ("Title", Source::Field(Field::Title)),
    ("Authors", Source::Field(Field::Authors)),
    ("PublicationName", Source::Field(Field::Journal)),
    ("AlternateName", Source::PassThrough),
    ("Abstract", Source::Field(Field::Abstract)),
    ("Url", Source::Field(Field::Link)),
    ("AuthorAddress", Source::PassThrough),
    ("Year", Source::Field(Field::Year)),
    ("DOI", Source::Field(Field::Doi)),
    ("ReferenceType", Source::PassThrough),
    ("Keywords", Source::Field(Field::Keywords)),
    ("PdfRelativePath", Source::PassThrough),
    ("CustomId", Source::PassThrough),
];

impl Layout {
    /// The columns this layout writes, in order.
    fn columns(self) -> &'static [(&'static str, Source)] {
        match self {
            Self::Generic => &GENERIC_COLUMNS,
            Self::Zotero => &ZOTERO_COLUMNS,
            Self::Syrf => &SYRF_COLUMNS,
        }
    }

    /// The field a column named `header` is read into, if any. Generic names are matched in any
    /// case and without surrounding whitespace and accept a few aliases; the others are matched
    /// exactly.
    fn field_for(self, header: &str) -> Option<Field> {
        if self != Self::Generic {
            return self
                .columns()
                .iter()
                .find(|(name, _)| *name == header)
                .and_then(|(_, source)| match source {
                    Source::Field(field) => Some(*field),
                    Source::PassThrough => None,
                });
        }
        Some(match header.trim().to_ascii_lowercase().as_str() {
            "title" => Field::Title,
            "author" | "authors" => Field::Authors,
            "date" => Field::IsoDate,
            "year" => Field::Year,
            "journal" => Field::Journal,
            "doi" => Field::Doi,
            "pages" => Field::Pages,
            "volume" => Field::Volume,
            "number" => Field::Number,
            "abstract" | "abstract_text" => Field::Abstract,
            "isbn" => Field::Isbn,
            "source" | "database" => Field::Database,
            "label" => Field::Label,
            "keywords" => Field::Keywords,
            "url" | "urls" => Field::Urls,
            _ => return None,
        })
    }

    /// Whether an extra named `name` would be written in, or read back into, one of this layout's
    /// own columns, so that it must not get a column of its own.
    fn is_own_column(self, name: &str) -> bool {
        self.field_for(name).is_some() || self.columns().iter().any(|(column, _)| *column == name)
    }
}

/// Detect the layout of a CSV file from its header row.
///
/// - [`Layout::Zotero`] if the header has `Key`, `Item Type` and `Publication Year`.
/// - [`Layout::Syrf`] if it has `PublicationName` and `Authors`.
/// - [`Layout::Generic`] otherwise, including for empty input.
///
/// A file that does not follow its layout's signature, such as records from several files merged
/// into one, can be read with [`parse_as`] instead.
///
/// ```
/// use biblio::csv::{Layout, detect_layout};
///
/// assert_eq!(
///     detect_layout("Key,Item Type,Publication Year,Title\n"),
///     Layout::Zotero
/// );
/// assert_eq!(detect_layout("title,author,year\n"), Layout::Generic);
/// ```
#[must_use]
pub fn detect_layout(input: &str) -> Layout {
    let input = strip_byte_order_mark(input);
    let mut reader = reader(input, detect_delimiter(input));
    let mut header = ::csv::StringRecord::new();
    if !matches!(reader.read_record(&mut header), Ok(true)) {
        return Layout::Generic;
    }
    let has = |name: &str| header.iter().any(|cell| cell == name);
    if has("Key") && has("Item Type") && has("Publication Year") {
        Layout::Zotero
    } else if has("PublicationName") && has("Authors") {
        Layout::Syrf
    } else {
        Layout::Generic
    }
}

/// Parse CSV text into zero or more [`Record`]s, detecting the layout from the header row (see
/// [`detect_layout`]).
///
/// # Errors
///
/// As [`parse_as`].
///
/// ```
/// let input = "title,author,year\nA title,\"Doe, Jane; Roe, Rick\",2024\n";
/// let records = biblio::csv::parse(input).unwrap().records;
/// assert_eq!(records[0].title.as_deref(), Some("A title"));
/// assert_eq!(records[0].authors, vec!["Doe, Jane", "Roe, Rick"]);
/// ```
pub fn parse(input: &str) -> Result<Parsed, Error> {
    parse_as(detect_layout(input), input)
}

/// Parse CSV text in the given layout into zero or more [`Record`]s.
///
/// The first row is the header. Columns are separated by `,`, or by `;` when the header row has
/// more `;` than `,` outside quotes, as some spreadsheet programs write. A leading byte-order mark
/// is ignored.
///
/// # Cells
///
/// Every cell is trimmed, and a blank cell is no value. In the generic layout, a cell that is
/// exactly `NA` is blank too, since that is how R writes a missing value; a real value of `NA` in
/// such a file is lost. List cells are split on `;`, and blank parts are dropped. Where several
/// columns can fill a field, the first non-blank one in the header wins and the others are kept in
/// extras; the date is read from the date column when it holds a readable `YYYY`, `YYYY-MM` or
/// `YYYY-MM-DD` (a month or day out of range is left out), and from the year column otherwise,
/// which is then not kept. A date that cannot be read is dropped. A row whose cells are all blank
/// is skipped.
///
/// # Diagnostics
///
/// Every record is kept, even one without a title. Problems are reported as diagnostics, at the
/// line where the row starts:
///
/// - [`Diagnostic::MissingTitle`]: a row has no title.
/// - [`Diagnostic::ExtraCells`]: a row has more cells than the header, and some of the surplus
///   ones are not blank. They are dropped. A row with fewer cells is read as if the rest were
///   blank.
/// - [`Diagnostic::Truncated`]: the last row opens a quoted cell that is never closed, so it runs
///   to the end of the input. That row is dropped.
///
/// # Errors
///
/// Returns [`Error::EmptyInput`] if the input, after any leading byte-order mark, is empty or
/// whitespace-only, and [`Error::MalformedSyntax`] if a quoted cell is left open before any record
/// is complete. [`Error::Internal`] is reserved for a failure of the underlying CSV reader, which
/// cannot happen when reading text.
///
/// ```
/// use biblio::csv::{Layout, parse_as};
///
/// let input = "Title,Author,Manual Tags\nA title,\"Doe, Jane\",cats; dogs\n";
/// let record = parse_as(Layout::Zotero, input).unwrap().records.remove(0);
/// assert_eq!(record.keywords, vec!["cats", "dogs"]);
/// ```
pub fn parse_as(layout: Layout, input: &str) -> Result<Parsed, Error> {
    let input = strip_byte_order_mark(input);
    check_empty(input)?;
    let delimiter = detect_delimiter(input);

    let mut rows = Vec::new();
    let mut reader = reader(input, delimiter);
    let mut row = ::csv::StringRecord::new();
    while reader
        .read_record(&mut row)
        .map_err(|e| Error::Internal(e.to_string()))?
    {
        let start = row.position().map_or(0, ::csv::Position::byte);
        rows.push((usize::try_from(start).unwrap_or(usize::MAX), row.clone()));
    }

    let mut lines = LineCounter::new(input);
    let mut diagnostics = Vec::new();

    // A quoted cell left open runs to the end of the input, so only the last row can have one.
    let open_row = rows
        .last()
        .map(|&(start, _)| start)
        .filter(|&start| ends_inside_quotes(input.get(start..).unwrap_or(""), delimiter));
    if let Some(start) = open_row {
        rows.pop();
        // A counter of its own: the rows before this one are counted afterwards, in order.
        let line = LineCounter::new(input).line_at(start);
        let message = "a quoted cell is not closed before the end of the input".to_owned();
        // Without a complete record before it, nothing can be read.
        let no_record = rows
            .iter()
            .skip(1)
            .all(|(_, row)| row.iter().all(|cell| is_blank(layout, cell)));
        if no_record {
            return Err(Error::MalformedSyntax { line, message });
        }
        diagnostics.push(Diagnostic::Truncated { line, message });
    }

    let mut rows = rows.into_iter();
    // Input that is not blank has a header row, but an empty one would do no harm.
    let header = rows.next().map(|(_, header)| header).unwrap_or_default();
    let columns: Vec<Column> = header
        .iter()
        .map(|name| Column {
            field: layout.field_for(name),
            key: format!("csv:{name}"),
        })
        .collect();

    let mut records = Vec::new();
    for (start, row) in rows {
        if row.iter().all(|cell| is_blank(layout, cell)) {
            continue;
        }
        let line = lines.line_at(start);
        if row
            .iter()
            .skip(columns.len())
            .any(|cell| !is_blank(layout, cell))
        {
            diagnostics.push(Diagnostic::ExtraCells {
                line,
                count: row.len() - columns.len(),
            });
        }
        records.push(read_row(layout, &columns, &row, line, &mut diagnostics));
    }

    Ok(parsed(records, diagnostics))
}

/// A column of the input: the field it fills, if any, and the extras key used when it does not.
struct Column {
    field: Option<Field>,
    /// `"csv:{header}"`, with the header as written.
    key: String,
}

/// Build a [`Record`] from one row, reporting a missing title at `line`.
fn read_row(
    layout: Layout,
    columns: &[Column],
    row: &::csv::StringRecord,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Record {
    // The first non-blank value of each field, with the index of its column.
    let mut values: [Option<(&str, usize)>; Field::COUNT] = [None; Field::COUNT];
    let mut extras: HashMap<String, Vec<String>> = HashMap::new();

    for (index, column) in columns.iter().enumerate() {
        let cell = row.get(index).map_or("", str::trim);
        if is_blank(layout, cell) {
            continue;
        }
        match column.field {
            Some(field) if values[field as usize].is_none() => {
                values[field as usize] = Some((cell, index));
            }
            _ => push_extra(&mut extras, column.key.clone(), cell),
        }
    }

    let value = |field: Field| values[field as usize].map(|(cell, _)| cell);
    let owned = |field: Field| value(field).map(str::to_owned);

    // The identifier: `ISBN`, or `ISSN` when it is blank. An `ISSN` beside an `ISBN` is kept.
    let isbn = match (values[Field::Isbn as usize], values[Field::Issn as usize]) {
        (Some((preferred, _)), Some((beside, index))) => {
            push_extra(&mut extras, columns[index].key.clone(), beside);
            Some(preferred.to_owned())
        }
        (Some((isbn, _)), None) => Some(isbn.to_owned()),
        (None, issn) => issn.map(|(issn, _)| issn.to_owned()),
    };

    // The date column wins when it can be read, and the year column is then not kept. Text in
    // either that gives no date is kept in extras.
    let iso_date = value(Field::IsoDate).and_then(parse_iso_date);
    let year_date = || {
        value(Field::Year)
            .and_then(|year| year.parse::<i32>().ok())
            .map(|year| PublicationDate {
                year,
                month: None,
                day: None,
            })
    };
    let date = iso_date.or_else(year_date);
    if let (None, Some((text, index))) = (iso_date, values[Field::IsoDate as usize]) {
        push_extra(&mut extras, columns[index].key.clone(), text);
    }
    if let (None, Some((text, index))) = (date, values[Field::Year as usize]) {
        push_extra(&mut extras, columns[index].key.clone(), text);
    }

    let urls = match (value(Field::Urls), value(Field::Link)) {
        (Some(list), _) => split_list(list),
        (None, single) => single.map(str::to_owned).into_iter().collect(),
    };

    let title = owned(Field::Title);
    report_missing_title(title.as_deref(), line, diagnostics);

    Record {
        title,
        authors: value(Field::Authors).map_or_else(Vec::new, split_list),
        date,
        journal: owned(Field::Journal),
        doi: owned(Field::Doi),
        pages: owned(Field::Pages),
        volume: owned(Field::Volume),
        number: owned(Field::Number),
        abstract_text: owned(Field::Abstract),
        isbn,
        database: owned(Field::Database),
        label: owned(Field::Label),
        keywords: value(Field::Keywords).map_or_else(Vec::new, split_list),
        urls,
        extras,
    }
}

/// Whether `cell` holds no value: it is blank, or, in the generic layout, exactly `NA`, which R
/// writes for a missing value.
fn is_blank(layout: Layout, cell: &str) -> bool {
    let cell = cell.trim();
    cell.is_empty() || (layout == Layout::Generic && cell == "NA")
}

/// Split a list cell on `;`, trimming each value and dropping blank ones.
fn split_list(cell: &str) -> Vec<String> {
    non_blank(cell.split(';').map(str::to_owned).collect())
}

/// Read a date written `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, allowing a leading `-` for years before
/// the common era and one-digit months and days. A month outside 1–12 is left out, and so is a day
/// outside 1–31 or without a month, so `2019-05-00` gives May 2019. Anything after the day's
/// digits, such as a time, is ignored. Returns `None` if there is no readable year.
fn parse_iso_date(value: &str) -> Option<PublicationDate> {
    let (negative, rest) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let mut parts = rest.split('-');
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());

    let year_part = parts.next().filter(|part| digits(part))?;
    let magnitude = year_part.parse::<i64>().ok()?;
    let year = i32::try_from(if negative { -magnitude } else { magnitude }).ok()?;

    let month = parts
        .next()
        .filter(|part| digits(part))
        .and_then(|part| part.parse::<u8>().ok())
        .filter(|month| (1..=12).contains(month));
    // The day may be followed by a time, as in `2019-05-15T10:00`.
    let day = month
        .and(parts.next())
        .map(|part| &part[..part.bytes().take_while(u8::is_ascii_digit).count()])
        .filter(|part| digits(part))
        .and_then(|part| part.parse::<u8>().ok())
        .filter(|day| (1..=31).contains(day));
    Some(PublicationDate { year, month, day })
}

/// A CSV reader over `input` that reads every row, the header included, and accepts rows of any
/// length.
fn reader(input: &str, delimiter: u8) -> ::csv::Reader<&[u8]> {
    ::csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(input.as_bytes())
}

/// The delimiter of the input: `;` if the header row, the first line that is not empty, has more
/// `;` than `,` outside quotes, and `,` otherwise.
fn detect_delimiter(input: &str) -> u8 {
    let (mut commas, mut semicolons) = (0_usize, 0_usize);
    let mut state = CellState::Start;
    // The reader skips blank lines before the header, so they say nothing about the delimiter.
    for byte in input.trim_start_matches(['\r', '\n']).bytes() {
        if !state.is_quoted() {
            match byte {
                b'\n' | b'\r' => break,
                b',' => commas += 1,
                b';' => semicolons += 1,
                _ => {}
            }
        }
        state = state.next(byte, matches!(byte, b',' | b';'));
    }
    if semicolons > commas { b';' } else { b',' }
}

/// Where the CSV reader is within a cell. It follows the reader's rules: a `"` opens quotes only at
/// the start of a cell, `""` inside quotes is a quote, and after a closing quote the rest of the
/// cell is read as it is.
#[derive(Clone, Copy)]
enum CellState {
    /// At the start of a cell.
    Start,
    /// In a cell that did not start with a quote, or after a closing quote.
    Unquoted,
    /// Inside quotes.
    Quoted,
    /// On a quote inside quotes, which either closes them or, doubled, is a quote.
    QuoteInQuoted,
}

impl CellState {
    /// The state after `byte`, where `ends_cell` says whether `byte` ends a cell outside quotes.
    fn next(self, byte: u8, ends_cell: bool) -> Self {
        match (self, byte) {
            (Self::Quoted, b'"') => Self::QuoteInQuoted,
            (Self::Quoted, _) | (Self::QuoteInQuoted, b'"') => Self::Quoted,
            (_, _) if ends_cell => Self::Start,
            (Self::Start, b'"') => Self::Quoted,
            _ => Self::Unquoted,
        }
    }

    /// Whether the reader is inside quotes, where delimiters and line breaks are text.
    fn is_quoted(self) -> bool {
        matches!(self, Self::Quoted)
    }
}

/// Whether `row`, the text of the last row to the end of the input, ends inside a quoted cell.
fn ends_inside_quotes(row: &str, delimiter: u8) -> bool {
    row.bytes()
        .fold(CellState::Start, |state, byte| {
            let ends_cell = byte == delimiter || byte == b'\n' || byte == b'\r';
            state.next(byte, ends_cell)
        })
        .is_quoted()
}

/// Turns byte offsets of rows into one-indexed line numbers, counting newlines once.
struct LineCounter<'a> {
    input: &'a [u8],
    /// The offset up to which newlines have been counted.
    counted: usize,
    /// The line at `counted`.
    line: usize,
}

impl<'a> LineCounter<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            counted: 0,
            line: 1,
        }
    }

    /// The line where a row whose reader position is `start` begins. The position may come before
    /// blank lines that the reader skipped, so those are stepped over first. Positions must not
    /// decrease between calls.
    fn line_at(&mut self, start: usize) -> usize {
        let mut start = start.min(self.input.len());
        while self
            .input
            .get(start)
            .is_some_and(|&byte| byte == b'\n' || byte == b'\r')
        {
            start += 1;
        }
        if start > self.counted {
            // A line ends with `\n`, `\r\n` or a lone `\r`. Rows start after their line breaks, so
            // a `\r\n` is never split between two counts.
            let span = &self.input[self.counted..start];
            self.line += span
                .iter()
                .enumerate()
                .filter(|&(i, &byte)| {
                    byte == b'\n' || (byte == b'\r' && span.get(i + 1) != Some(&b'\n'))
                })
                .count();
            self.counted = start;
        }
        self.line
    }
}

/// Serialize [`Record`]s to CSV in the generic layout (see [`Layout::Generic`]).
///
/// See [`serialize_as`] for how cells are written.
///
/// ```
/// let record = biblio::Record::builder()
///     .title("A title")
///     .author("Doe, Jane")
///     .build();
/// let output = biblio::csv::serialize(&[record]);
/// assert!(output.starts_with("title,author,year,date,"));
/// ```
#[must_use]
pub fn serialize(records: &[Record]) -> String {
    serialize_as(Layout::Generic, records)
}

/// Serialize [`Record`]s to CSV in the given layout.
///
/// The header row names the layout's columns (see [`Layout`]), then every `csv:` extra of the
/// records that the layout does not read, sorted by name. Cells holding `,`, `;`, `"` or a line
/// break are quoted, with `"` doubled; rows end with `\r\n`, and no byte-order mark is written.
/// With no records, only the header row is written.
///
/// List values are joined with `; `, and blank ones are skipped. An extra's first value is written,
/// because a cell holds one value; extras of other formats are not written. An extra that the
/// layout would read into a field, such as a date that could not be read, is written in that
/// field's column when the record leaves it empty; a second identifier is added to the record's
/// own with a space, as Zotero lists several ISSNs. Zotero writes the identifier to `ISSN` when
/// every part of it looks like an ISSN (`1234-5678`), and to `ISBN` otherwise.
///
/// Values are trimmed when read, so leading and trailing whitespace is lost. Some values do not
/// come back the same when read again: a list value containing `;` splits; a blank title is read
/// as none, and so is a generic cell of exactly `NA`; a record with no value in any column gives a
/// blank row, which is skipped; an extra that the layout reads into a field the record already
/// fills, such as a second `DOI` column, is not written; and a layout without a column for a field
/// drops it (`SyRF` keeps only the year of a date, and Zotero and `SyRF` only the first link).
///
/// Cells are written as they are: one starting with `=`, `+`, `-` or `@` may be run as a formula
/// by a spreadsheet program that opens the file, so a caller offering the file for download
/// should neutralise such cells if its records come from untrusted input.
///
/// ```
/// use biblio::csv::{Layout, serialize_as};
///
/// let record = biblio::Record::builder()
///     .title("A title")
///     .isbn("1234-5678")
///     .build();
/// let output = serialize_as(Layout::Zotero, &[record]);
/// assert!(output.starts_with("Key,Item Type,Publication Year,"));
/// ```
#[must_use]
pub fn serialize_as(layout: Layout, records: &[Record]) -> String {
    let extra_columns: BTreeSet<&str> = records
        .iter()
        .flat_map(|record| own_extras(&record.extras, "csv"))
        .map(|(name, _)| name)
        .filter(|name| !layout.is_own_column(name))
        .collect();

    let mut out = String::new();
    let header = layout
        .columns()
        .iter()
        .map(|(name, _)| *name)
        .chain(extra_columns.iter().copied());
    write_row(&mut out, header);

    for record in records {
        let extra = |name: &str| {
            record
                .extras
                .get(&format!("csv:{name}"))
                .and_then(|values| values.first())
                .cloned()
        };
        let cells: Vec<String> = layout
            .columns()
            .iter()
            .map(|&(name, source)| match source {
                Source::Field(field) => field_cell(layout, field, record),
                Source::PassThrough => extra(name),
            })
            .chain(extra_columns.iter().map(|name| extra(name)))
            .map(Option::unwrap_or_default)
            .collect();
        write_row(&mut out, cells.iter().map(String::as_str));
    }

    out
}

/// The cell a column for `field` holds: the record's value, or text read beside it and kept in
/// extras, such as a second identifier or a date that could not be read.
///
/// An extra counts when `layout` would read a column of its name into `field`. It fills a column
/// the record leaves empty, and a date column whose date has no month, which the year already
/// gives; in Zotero, a second identifier is added to the identifier with a space, as Zotero lists
/// several ISSNs.
fn field_cell(layout: Layout, field: Field, record: &Record) -> Option<String> {
    let beside = own_extras(&record.extras, "csv")
        .into_iter()
        .find(|(name, _)| layout.field_for(name) == Some(field))
        .and_then(|(_, values)| non_blank_values(values).next())
        .map(str::to_owned);
    let own = cell(layout, field, record);
    match (own, beside) {
        (Some(own), Some(beside)) if matches!(field, Field::Isbn | Field::Issn) => {
            Some(format!("{own} {beside}"))
        }
        (_, Some(beside))
            if field == Field::IsoDate && record.date.is_some_and(|date| date.month.is_none()) =>
        {
            Some(beside)
        }
        (Some(own), _) => Some(own),
        (None, beside) => beside,
    }
}

/// The text of `field` of `record` in `layout`, if it has one.
fn cell(layout: Layout, field: Field, record: &Record) -> Option<String> {
    let list = |values: &[String]| {
        let joined = non_blank_values(values).collect::<Vec<_>>().join("; ");
        (!joined.is_empty()).then_some(joined)
    };
    match field {
        Field::Title => record.title.clone(),
        Field::Authors => list(&record.authors),
        Field::IsoDate => record.date.map(|date| date.to_string()),
        Field::Year => record.date.map(|date| date.year.to_string()),
        Field::Journal => record.journal.clone(),
        Field::Doi => record.doi.clone(),
        Field::Pages => record.pages.clone(),
        Field::Volume => record.volume.clone(),
        Field::Number => record.number.clone(),
        Field::Abstract => record.abstract_text.clone(),
        Field::Isbn | Field::Issn => {
            let isbn = record.isbn.as_deref()?;
            let to_issn = layout == Layout::Zotero && is_issn_list(isbn);
            (to_issn == (field == Field::Issn)).then(|| isbn.to_owned())
        }
        Field::Database => record.database.clone(),
        Field::Label => record.label.clone(),
        Field::Keywords => list(&record.keywords),
        Field::Urls => list(&record.urls),
        Field::Link => non_blank_values(&record.urls).next().map(str::to_owned),
    }
}

/// Whether every part of `identifier`, split on whitespace, `,` and `;`, has the shape of an ISSN:
/// four digits, an optional hyphen, three digits and a check digit or `X`.
fn is_issn_list(identifier: &str) -> bool {
    let is_issn = |part: &str| {
        let compact: Vec<u8> = part.bytes().filter(|&b| b != b'-').collect();
        let hyphens = part.len() - compact.len();
        compact.len() == 8
            && (hyphens == 0 || (hyphens == 1 && part.as_bytes().get(4) == Some(&b'-')))
            && compact[..7].iter().all(u8::is_ascii_digit)
            && (compact[7].is_ascii_digit() || compact[7] == b'X' || compact[7] == b'x')
    };
    let mut parts = identifier
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|part| !part.is_empty())
        .peekable();
    parts.peek().is_some() && parts.all(is_issn)
}

/// Write one row of cells, quoting those that need it, followed by `\r\n`.
fn write_row<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>) {
    for (index, cell) in cells.enumerate() {
        if index > 0 {
            out.push(',');
        }
        if cell.contains([',', ';', '"', '\r', '\n']) {
            out.push('"');
            out.push_str(&cell.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push_str("\r\n");
}
