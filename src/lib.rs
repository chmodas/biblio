//! A library for converting between bibliographic reference formats.
//!
//! `biblio` provides a unified [`Record`] type that captures the metadata common to `RIS`, `NBIB`,
//! `EndNote XML`, `BibTeX`, `BibLaTeX` and CSV files. Each format has a dedicated module exposing
//! the same `parse` / `serialize` pair; [`csv`] reads three column layouts, including Zotero's
//! export, and can write each of them:
//!
//! ```
//! use biblio::{bibtex, ris};
//!
//! let bibtex_input = r#"
//! @article{doe2024,
//!   title  = {An interesting finding},
//!   author = {Doe, Jane and Smith, John},
//!   year   = {2024},
//!   journal = {Nature},
//! }
//! "#;
//!
//! let records = bibtex::parse(bibtex_input).unwrap().records;
//! let ris_output = ris::serialize(&records);
//! assert!(ris_output.contains("TI  - An interesting finding"));
//! // Fields only `BibTeX` has, such as the citation key, are not written into the `RIS`.
//! assert!(!ris_output.contains("doe2024"));
//! ```
//!
//! # Reading bytes
//!
//! The parsers take text. To read a file that may not be UTF-8, [`decode`] its bytes first; it
//! never fails, and reports the encoding it used:
//!
//! ```
//! // "Café" in Windows-1252, as an older Windows exporter writes it.
//! let bytes = b"TY  - JOUR\nTI  - Caf\xe9\nER  - \n";
//! let decoded = biblio::decode(bytes);
//! let parsed = biblio::ris::parse(&decoded.text).unwrap();
//! assert_eq!(parsed.records[0].title.as_deref(), Some("Café"));
//! ```
//!
//! # Features
//!
//! - `serde`: `Serialize` and `Deserialize` for [`Record`], [`PublicationDate`], [`Parsed`],
//!   [`Diagnostic`], [`Error`] and [`csv::Layout`], so parse results can be sent as JSON.
//!   Diagnostics and errors are tagged by a `kind` member:
//!
//! ```
//! # #[cfg(feature = "serde")]
//! # {
//! let parsed = biblio::ris::parse("TY  - JOUR\nER  -\n").unwrap();
//! let json = serde_json::to_value(&parsed).unwrap();
//! assert_eq!(json["diagnostics"][0]["kind"], "missing_title");
//! let back: biblio::Parsed = serde_json::from_value(json).unwrap();
//! assert_eq!(back, parsed);
//! # }
//! ```
pub mod bibtex;
pub mod csv;
mod decode;
pub mod endnote_xml;
mod error;
mod identifier;
pub mod nbib;
mod parse_util;
mod parsed;
mod record;
pub mod ris;

pub use decode::{Decoded, Encoding, decode};
pub use error::Error;
pub use parsed::{Diagnostic, Parsed};
pub use record::{PublicationDate, Record, RecordBuilder};
