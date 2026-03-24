//! A library for converting between bibliographic reference formats.
//!
//! `biblio` provides a unified [`Record`] type that captures the metadata common to `RIS`, `NBIB`,
//! `EndNote XML`, `BibTeX`, and `BibLaTeX` files. Each format has a dedicated module exposing the
//! same `parse` / `serialize` pair:
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
//! let records = bibtex::parse(bibtex_input).unwrap();
//! let ris_output = ris::serialize(&records);
//! assert!(ris_output.contains("TI  - An interesting finding"));
//! ```
pub mod bibtex;
pub mod endnote_xml;
mod error;
pub mod nbib;
mod parse_util;
mod record;
pub mod ris;

pub use error::Error;
pub use record::{PublicationDate, Record};
