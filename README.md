# biblio

biblio parses and serialises **RIS**, **NBIB (PubMed)**, **EndNote XML**, **BibTeX**, and **BibLaTeX** through a single unified `Record` type, so you can read references in one format and write them in another without hand-rolling format-specific logic.

> **Work in progress.** The current `Record` carries the minimum useful set of fields (title, authors, date, journal, DOI, pages, volume, number, abstract, ISBN/ISSN). Additional fields – keywords, URLs, publication type, language, funding, affiliations, and more – will be added in upcoming releases.

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

let records = bibtex::parse(bibtex_input).unwrap();
let ris_output = ris::serialize(&records);
println!("{ris_output}");
```

Every format module exposes the same two-function interface:

```rust
fn parse(input: &str) -> Result<Vec<Record>, Error>;
fn serialize(records: &[Record]) -> String;
```

## The `Record` type

`Record` is a plain struct with public fields – no builder, no hidden state:

```rust
use biblio::{Record, PublicationDate};

let rec = Record {
    title: "My paper".into(),
    authors: vec!["Doe, Jane".into()],
    date: Some(PublicationDate { year: 2024, month: None, day: None }),
    ..Default::default()
};
```

Fields that don't map neatly to the common schema (PMID, PMCID, citation key, etc.) land in `extras: HashMap<String, String>` so nothing is silently dropped.

## Error handling

All parsers return `Result<Vec<Record>, biblio::Error>`. The error type distinguishes empty input, missing required fields (with the offending tag name and line number), malformed syntax, and internal errors.

## Licence

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
