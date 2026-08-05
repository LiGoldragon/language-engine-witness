//! Isolated producer for the frozen, exact-current-v14 archive fixture.
//!
//! This deliberately remains outside the witness crate's dependency graph:
//! v14 and the new generated bundle pin incompatible revisions of the native
//! `signal-domain` linkage.  A single Cargo graph would therefore hide the
//! real compatibility boundary instead of testing it.

use std::mem::{align_of, size_of};

use spirit::{
    Store,
    schema::{
        sema::{MigratedRecordCount, Migration, RecordFamily, SourceSchemaVersion, StoredRecord},
        signal::{
            Description, Domain, Domains, Entry, Importance, Information, Kind, Magnitude,
            RecordIdentifier,
        },
    },
};

fn entry() -> Entry {
    Entry {
        domains: Domains::new(vec![Domain::Information(Information::Documentation)]),
        kind: Kind::Decision,
        description: Description::new("preserved current-v14 record"),
        importance: Importance::new(Magnitude::Medium),
    }
}

fn stored_record() -> StoredRecord {
    StoredRecord {
        record_identifier: RecordIdentifier::new("preserved-current-v14-record"),
        entry: entry(),
    }
}

fn migration() -> Migration {
    Migration {
        source_schema_version: SourceSchemaVersion::new(14),
        migrated_record_count: MigratedRecordCount::new(1),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

macro_rules! archive_hex {
    ($value:expr) => {{
        let value = $value;
        hex(rkyv::to_bytes::<rkyv::rancor::Error>(&value)
            .expect("archive exact current-v14 fixture value")
            .as_slice())
    }};
}

fn fixture_document() -> serde_json::Value {
    // The pinned v14 Store must itself write and reopen a real current-v14
    // database before its archive output is accepted as fixture evidence.
    let temporary = tempfile::tempdir().expect("create current-v14 store sandbox");
    let path = temporary.path().join("current-v14.sema");
    let expected_entry = entry();
    let receipt = Store::open(&path)
        .expect("open current-v14 store")
        .record_entry(expected_entry.clone())
        .expect("write current-v14 entry");
    let reopened = Store::open(&path).expect("reopen current-v14 store");
    assert_eq!(
        reopened
            .entry_by_identifier(receipt.record_identifier.payload())
            .expect("look up reopened current-v14 entry"),
        Some(expected_entry),
    );

    let records = RecordFamily::records_family();
    let migrations = RecordFamily::migrations_family();
    let document = serde_json::json!({
        "source_revision": "7405eee89e3b1b5b6764eb1a50cbdf467b93c9a7",
        "store_schema_version": 14,
        "records": {
            "table": records.name().as_str(),
            "family": records.family().as_str(),
            "schema_hash": records.schema_hash().to_string(),
            "stored_record_hex": archive_hex!(stored_record()),
            "record_identifier_hex": archive_hex!(RecordIdentifier::new("preserved-current-v14-record")),
            "entry_hex": archive_hex!(entry()),
            "stored_record_size": size_of::<StoredRecord>(),
            "stored_record_align": align_of::<StoredRecord>(),
            "record_identifier_size": size_of::<RecordIdentifier>(),
            "record_identifier_align": align_of::<RecordIdentifier>(),
            "entry_size": size_of::<Entry>(),
            "entry_align": align_of::<Entry>(),
        },
        "migrations": {
            "table": migrations.name().as_str(),
            "family": migrations.family().as_str(),
            "schema_hash": migrations.schema_hash().to_string(),
            "migration_hex": archive_hex!(migration()),
            "source_schema_version_hex": archive_hex!(SourceSchemaVersion::new(14)),
            "migration_size": size_of::<Migration>(),
            "migration_align": align_of::<Migration>(),
            "source_schema_version_size": size_of::<SourceSchemaVersion>(),
            "source_schema_version_align": align_of::<SourceSchemaVersion>(),
        },
    });
    document
}

fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&fixture_document())
            .expect("encode exact current-v14 fixture")
    );
}

#[cfg(test)]
mod tests {
    use super::fixture_document;

    #[test]
    fn frozen_fixture_is_produced_by_the_pinned_current_v14_types() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../archive-fixture.json"))
                .expect("parse frozen current-v14 fixture");
        assert_eq!(fixture_document(), expected);
    }
}
