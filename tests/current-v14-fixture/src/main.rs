//! Isolated producer for the frozen, exact-current-v14 archive fixture.
//!
//! This deliberately remains outside the witness crate's dependency graph:
//! v14 and the new generated bundle pin incompatible revisions of the native
//! `signal-domain` linkage.  A single Cargo graph would therefore hide the
//! real compatibility boundary instead of testing it.

use std::{
    env,
    mem::{align_of, size_of},
    path::PathBuf,
    process,
};

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

/// Compress the repetitive redb fixture without introducing a decompressor
/// dependency into the generated-side witness. `rHH:NN;` represents NN copies
/// of byte HH; `bHEX;` carries one maximal uncompressed run.
fn run_length_encode(bytes: &[u8]) -> String {
    let mut encoded = String::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        let run_end = bytes[cursor..]
            .iter()
            .position(|candidate| *candidate != byte)
            .map_or(bytes.len(), |offset| cursor + offset);
        if run_end - cursor >= 4 {
            encoded.push('r');
            encoded.push_str(&format!("{byte:02x}:{}", run_end - cursor));
            cursor = run_end;
        } else {
            let start = cursor;
            cursor = run_end;
            while cursor < bytes.len() {
                let next = bytes[cursor];
                let next_end = bytes[cursor..]
                    .iter()
                    .position(|candidate| *candidate != next)
                    .map_or(bytes.len(), |offset| cursor + offset);
                if next_end - cursor >= 4 {
                    break;
                }
                cursor = next_end;
            }
            encoded.push('b');
            encoded.push_str(&hex(&bytes[start..cursor]));
        }
        encoded.push(';');
    }
    encoded
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
    let store = Store::open(&path).expect("open current-v14 store");
    store
        .import_record(
            "preserved-current-v14-record".to_owned(),
            expected_entry.clone(),
        )
        .expect("write current-v14 entry under its frozen key");
    drop(store);
    let reopened = Store::open(&path).expect("reopen current-v14 store");
    assert_eq!(
        reopened
            .entry_by_identifier("preserved-current-v14-record")
            .expect("look up reopened current-v14 entry"),
        Some(expected_entry),
    );
    drop(reopened);
    let store_fixture = std::fs::read(&path).expect("read closed current-v14 store fixture");

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
            "store_run_length": run_length_encode(&store_fixture),
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

fn assert_generated_record_is_readable_by_current_v14(path: PathBuf) -> Result<(), String> {
    let reopened = Store::open(&path).map_err(|error| {
        format!("open generated store with pinned current-v14 reader: {error:?}")
    })?;
    assert_eq!(
        reopened
            .entry_by_identifier("generated-after-current-v14-reopen")
            .map_err(|error| format!(
                "query generated record through current-v14 reader: {error:?}"
            ))?,
        Some(entry()),
    );
    Ok(())
}

fn main() {
    if let Some(path) = env::var_os("SPIRIT_V14_REOPEN_STORE") {
        match assert_generated_record_is_readable_by_current_v14(PathBuf::from(path)) {
            Ok(()) => return,
            Err(error) if error.contains("StorageLayoutMismatch") => {
                eprintln!("pinned-v14-reader-api-unavailable: {error}");
                process::exit(42);
            }
            Err(error) => panic!("{error}"),
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&fixture_document())
            .expect("encode exact current-v14 fixture")
    );
}

#[cfg(test)]
mod tests {
    use spirit::schema::{
        sema::{Migration, SourceSchemaVersion, StoredRecord},
        signal::{Entry, RecordIdentifier},
    };

    use super::fixture_document;

    fn fixture_bytes(fixture: &serde_json::Value, section: &str, field: &str) -> Vec<u8> {
        let text = fixture[section][field]
            .as_str()
            .unwrap_or_else(|| panic!("frozen fixture lacks {section}.{field}"));
        assert_eq!(text.len() % 2, 0, "{section}.{field} has odd hex length");
        (0..text.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&text[index..index + 2], 16)
                    .unwrap_or_else(|error| panic!("{section}.{field} invalid hex: {error}"))
            })
            .collect()
    }

    #[test]
    fn frozen_fixture_is_produced_by_the_pinned_current_v14_types() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../archive-fixture.json"))
                .expect("parse frozen current-v14 fixture");
        assert_eq!(fixture_document(), expected);
    }

    #[test]
    fn frozen_archives_restore_the_pinned_current_v14_types() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../archive-fixture.json"))
                .expect("parse frozen current-v14 fixture");
        let stored_record = rkyv::from_bytes::<StoredRecord, rkyv::rancor::Error>(&fixture_bytes(
            &fixture,
            "records",
            "stored_record_hex",
        ))
        .expect("restore current-v14 StoredRecord");
        assert_eq!(stored_record, super::stored_record());
        let key = rkyv::from_bytes::<RecordIdentifier, rkyv::rancor::Error>(&fixture_bytes(
            &fixture,
            "records",
            "record_identifier_hex",
        ))
        .expect("restore current-v14 RecordIdentifier");
        assert_eq!(key.payload(), "preserved-current-v14-record");
        let entry = rkyv::from_bytes::<Entry, rkyv::rancor::Error>(&fixture_bytes(
            &fixture,
            "records",
            "entry_hex",
        ))
        .expect("restore current-v14 Entry closure");
        assert_eq!(entry, super::entry());
        let migration = rkyv::from_bytes::<Migration, rkyv::rancor::Error>(&fixture_bytes(
            &fixture,
            "migrations",
            "migration_hex",
        ))
        .expect("restore current-v14 Migration");
        assert_eq!(migration, super::migration());
        let migration_key = rkyv::from_bytes::<SourceSchemaVersion, rkyv::rancor::Error>(
            &fixture_bytes(&fixture, "migrations", "source_schema_version_hex"),
        )
        .expect("restore current-v14 SourceSchemaVersion");
        assert_eq!(*migration_key.payload(), 14);
    }
}
