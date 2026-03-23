//! A library for converting between bibliographic reference formats.
//!
//! `biblio` provides a unified [`Record`] type that captures the metadata common to
//! `RIS`, `NBIB`, `EndNote XML`, `BibTeX`, and `BibLaTeX` files.
pub mod bibtex;
pub mod endnote_xml;
mod error;
pub mod nbib;
mod parse_util;
mod record;
pub mod ris;

pub use error::Error;
pub use record::{PublicationDate, Record};
