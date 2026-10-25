# biblio

[![CI](https://github.com/chmodas/biblio/actions/workflows/ci.yml/badge.svg)](https://github.com/chmodas/biblio/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/chmodas/biblio/graph/badge.svg?token=5KAFOZEH6G)](https://codecov.io/gh/chmodas/biblio)

biblio parses and serializes **RIS**, **NBIB (PubMed)**, **EndNote XML**, **BibTeX**, **BibLaTeX** and **CSV** through a single unified `Record` type, so you can read references in one format and write them in another without hand-rolling format-specific logic.

> **Work in progress.** The current `Record` carries the minimum useful set of fields (title, authors, date, journal, DOI, pages, volume, number, abstract, ISBN/ISSN, source database, label, keywords, URLs). Additional fields – publication type, language, funding, affiliations, and more – will be added in upcoming releases.

## Quick start

Add biblio to your project (requires Rust 1.85.0 or newer):

```sh
cargo add biblio
```

Parse a BibTeX file and re-export as RIS:

```rust
use biblio::{bibtex, ris};

let bibtex_input = r#"
@article{doe2024,
  title  = {An interesting finding},
  author = {Doe, Jane and Smith, John},
  year   = {2024},
  journal = {Nature},
  doi    = {10.1038/s41586-024-07386-0},
}
"#;

let parsed = bibtex::parse(bibtex_input).unwrap();
let ris_output = ris::serialize(&parsed.records);
println!("{ris_output}");
```

Every format module exposes the same two-function interface:

```rust
fn parse(input: &str) -> Result<Parsed, Error>;
fn serialize(records: &[Record]) -> String;
```

## The `Record` type

`Record` has public fields and is built with `Record::builder()`:

```rust
use biblio::{Record, PublicationDate};

let rec = Record::builder()
    .title("My paper")
    .author("Doe, Jane")
    .date(PublicationDate { year: 2024, month: None, day: None })
    .build();
```

`Record`, `Parsed`, `Diagnostic` and `Error` are `#[non_exhaustive]`, so fields and variants can be added without breaking your code: build records with the builder, and give a `match` on `Diagnostic` or `Error` a wildcard arm.

The title is optional, because some records have none. Every field without a slot of its own lands in `extras: HashMap<String, Vec<String>>`, keyed by format and tag (`"ris:N1"`, `"nbib:PMID"`, `"bibtex:note"`, `"endnote:urls/pdf-urls/url"`, …) with its values in order, so fields without a slot are no longer dropped. Each serialiser writes back only the extras of its own format: converting BibTeX to RIS keeps the BibTeX-only fields on the record but does not write them into the RIS.

## CSV

`biblio::csv` reads three column layouts, detected from the header row, and writes each of them:

- **Generic**: columns named after the record's fields (`title`, `author`, `year`, `date`, `journal`, `doi`, `pages`, `volume`, `number`, `abstract`, `isbn`, `source`, `label`, `keywords`, `url`), matched in any case. A cell of exactly `NA` is read as blank, as R writes missing values.
- **Zotero**: Zotero's CSV export.
- **SyRF**: the SyRF import layout.

Authors, keywords and links in one cell are separated by `;`. Files separated by `;` instead of `,` are read too. Other columns are kept in `extras` and written back.

```rust
let parsed = biblio::csv::parse(&text)?;
let layout = biblio::csv::detect_layout(&text);
let csv = biblio::csv::serialize_as(layout, &parsed.records);
```

Cells are written as they are, so one starting with `=`, `+`, `-` or `@` may be run as a formula by a spreadsheet program. If you offer CSV built from untrusted records for download, neutralise such cells first.

## Reading files in other encodings

The parsers take text, but exports are not always UTF-8. `biblio::decode` turns the bytes of a file into text and never fails: it follows a byte-order mark (UTF-8 or UTF-16), keeps valid UTF-8 as it is, then uses the encoding named in an XML declaration, and otherwise reads Windows-1252, which older Windows exporters write.

```rust
let bytes = std::fs::read("export.ris")?;
let decoded = biblio::decode(&bytes);
println!("read as {}", decoded.encoding); // e.g. "windows-1252"
if decoded.replaced {
    eprintln!("some characters could not be read");
}
let parsed = biblio::ris::parse(&decoded.text)?;
```

## Errors and diagnostics

A bad record does not reject the whole file. Each parser keeps every record it can read and returns a `Parsed` with the `records` and a list of `diagnostics`, sorted by line, describing what it had to drop or patch: records without a title, `RIS` records not closed with `ER`, lines that belong to no record, `BibTeX` entries that could not be parsed, and `EndNote XML` that breaks off part way.

```rust
let parsed = biblio::ris::parse(input)?;
for diagnostic in &parsed.diagnostics {
    eprintln!("{diagnostic}");
}
```

`Err` is kept for problems with the whole input: empty input, or `EndNote XML` that breaks off before any record is complete.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
