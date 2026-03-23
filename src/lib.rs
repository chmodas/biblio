//! A library for converting between bibliographic reference formats.
//!
//! `biblio` provides a unified [`Record`] type that captures the metadata common to
//! `RIS`, `NBIB`, `EndNote XML`, `BibTeX`, and `BibLaTeX` files.
mod error;
pub mod nbib;
mod record;

pub use error::Error;
pub use record::{PublicationDate, Record};
