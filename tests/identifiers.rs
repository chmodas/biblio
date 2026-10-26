//! The identifier chosen when a record lists several is the same in every format.

use biblio::csv::{Layout, parse_as};
use biblio::{bibtex, csv, endnote_xml, nbib, ris};

/// One article's print and electronic ISSNs, listed as each format writes them, give the same
/// identifier in every format.
#[test]
fn same_identifier_in_every_format() {
    let records = [
        ris::parse("TY  - JOUR\nTI  - T\nSN  - 1939-1676\nSN  - 0891-6640\nER  -\n"),
        nbib::parse("PMID- 1\nTI  - T\nIS  - 1939-1676 (Electronic)\nIS  - 0891-6640 (Print)\n"),
        endnote_xml::parse(
            "<xml><records><record><titles><title>T</title></titles><isbn>1939-1676&#xD;0891-6640</isbn></record></records></xml>",
        ),
        bibtex::parse("@article{k, title = {T}, issn = {1939-1676, 0891-6640}}"),
        csv::parse("title,isbn\nT,1939-1676 0891-6640\n"),
        parse_as(Layout::Zotero, "Title,ISSN\nT,1939-1676 0891-6640\n"),
    ];
    for parsed in records {
        let record = &parsed.unwrap().records[0];
        assert_eq!(record.isbn.as_deref(), Some("0891-6640"), "{record:?}");
    }
}
