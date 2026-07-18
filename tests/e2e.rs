use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use name_table::Identifier;
use signal_frame::ProtocolVersion;
use signal_sema_storage::{
    BelongsRelation, DocumentKey, DocumentKind, FamilyDeclaration, FixtureScope, FrameMessage,
    NameTableBytes, NexusActorDeclaration, NexusRoute, NexusRuntimeRoot, OpensRelation,
    SemaStorageRoot, SignalContractRoot, SlotIdentifier, StreamDeclaration, Wire,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

struct Processes(Vec<Child>);
impl Processes {
    fn terminate(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.0.clear();
    }
}
impl Drop for Processes {
    fn drop(&mut self) {
        self.terminate();
    }
}
struct ProductionBinaries {
    sema: PathBuf,
    schema: PathBuf,
    nomos: PathBuf,
    logos: PathBuf,
}
impl ProductionBinaries {
    fn from_environment() -> Self {
        Self {
            sema: std::env::var_os("SEMA_STORAGE_BIN")
                .expect("SEMA_STORAGE_BIN")
                .into(),
            schema: std::env::var_os("SCHEMA_ENGINE_BIN")
                .expect("SCHEMA_ENGINE_BIN")
                .into(),
            nomos: std::env::var_os("NOMOS_ENGINE_BIN")
                .expect("NOMOS_ENGINE_BIN")
                .into(),
            logos: std::env::var_os("LOGOS_ENGINE_BIN")
                .expect("LOGOS_ENGINE_BIN")
                .into(),
        }
    }

    fn spawn(&self, kind: &str, socket: &Path, state: &Path, upstream: Option<&Path>) -> Child {
        let program = match kind {
            "sema" => &self.sema,
            "schema" => &self.schema,
            "nomos" => &self.nomos,
            "logos" => &self.logos,
            _ => panic!("unknown production binary: {kind}"),
        };
        let mut command = Command::new(program);
        command.args(["daemon", socket.to_str().unwrap(), state.to_str().unwrap()]);
        if let Some(upstream) = upstream {
            command.arg(upstream);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let line = BufReader::new(child.stdout.take().expect("daemon stdout"))
            .lines()
            .next()
            .expect("daemon readiness line")
            .expect("read daemon readiness");
        assert_eq!(line, format!("READY {}", socket.display()));
        child
    }
}
struct TestSocket {
    stream: UnixStream,
    sequence: u64,
}
impl TestSocket {
    async fn connect(path: &Path) -> Self {
        let mut socket = Self {
            stream: UnixStream::connect(path).await.unwrap(),
            sequence: 0,
        };
        socket
            .stream
            .write_all(&Wire::frame_current_handshake_request().unwrap())
            .await
            .unwrap();
        assert!(
            Wire::decode_frame(&socket.read_frame().await.unwrap())
                .unwrap()
                .is_accepted_handshake()
        );
        socket
    }

    async fn request<Q>(&mut self, request: &Q, encode: impl Fn(&Q) -> Vec<u8>) {
        let frame = Wire::frame_request(encode(request), self.sequence).unwrap();
        self.sequence += 1;
        self.stream.write_all(&frame).await.unwrap();
    }

    async fn read_reply<R>(&mut self) -> R
    where
        R: rkyv::Archive,
        R::Archived: for<'a> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
            > + rkyv::Deserialize<R, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
    {
        let FrameMessage::Reply { payload, .. } =
            Wire::decode_frame(&self.read_frame().await.unwrap()).unwrap()
        else {
            panic!("expected shared reply frame")
        };
        rkyv::from_bytes::<R, rkyv::rancor::Error>(&payload).unwrap()
    }

    async fn read_frame(&mut self) -> std::io::Result<Vec<u8>> {
        let length = self.stream.read_u32().await? as usize;
        let mut frame = Vec::with_capacity(length + 4);
        frame.extend_from_slice(&(length as u32).to_be_bytes());
        frame.resize(length + 4, 0);
        self.stream.read_exact(&mut frame[4..]).await?;
        Ok(frame)
    }
}

async fn exchange<Q, R>(socket: &Path, request: &Q, encode: impl Fn(&Q) -> Vec<u8>) -> R
where
    R: rkyv::Archive,
    R::Archived: for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>
        + rkyv::Deserialize<R, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    let mut socket = TestSocket::connect(socket).await;
    socket.request(request, encode).await;
    socket.read_reply().await
}
/// The offline lockfile fixture the emitted `generated-spirit` crate builds against.
/// Its git-dependency revisions are underived here (a separate committed file), so
/// [`emitted_manifest_and_locked_fixture_pin_the_same_revisions`] guards it against
/// silently drifting from the manifest's [`NOTA_REV`] / [`SIGNAL_FRAME_REV`] on a
/// chain re-pin.
const SPIRIT_LOCK: &str = include_str!("fixtures/spirit-lock.toml");

/// The git revisions the emitted `generated-spirit` crate depends on, single-sourced
/// here so the manifest [`write_crate`] emits and the lockfile fixture [`SPIRIT_LOCK`]
/// cannot silently disagree after a chain re-pin. They must track the nota /
/// signal-frame revisions the daemon closure emits against (the flake's daemon
/// inputs); a re-pin updates this one place and the lockfile fixture together, and
/// [`emitted_manifest_and_locked_fixture_pin_the_same_revisions`] fails loudly if the
/// two drift apart.
const NOTA_REV: &str = "89dc3c85a9ff96d4e4d53accfd867df672cae5a8";
const SIGNAL_FRAME_REV: &str = "f46872e7e8edae5264c892443d415a273b231234";

fn write_crate(path: &Path, rust: &str) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("tests")).unwrap();
    fs::write(path.join("Cargo.lock"), SPIRIT_LOCK).unwrap();
    fs::write(
        path.join("Cargo.toml"),
        format!(
            "[package]\nname=\"generated-spirit\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrkyv={{version=\"0.8\",features=[\"bytecheck\"]}}\nsignal-frame={{git=\"https://github.com/LiGoldragon/signal-frame.git\",rev=\"{SIGNAL_FRAME_REV}\",default-features=false}}\nnota={{git=\"https://github.com/LiGoldragon/nota.git\",rev=\"{NOTA_REV}\",optional=true}}\n[features]\ndefault=[]\nnota-text=[\"dep:nota\"]\n"
        ),
    )
    .unwrap();
    // The pipeline now emits the full module head (marker, scalar prelude, NOTA
    // import), so the generated source is written verbatim — no prelude is injected.
    fs::write(path.join("src/lib.rs"), rust).unwrap();
    fs::write(
        path.join("tests/behavior.rs"),
        r#"use generated_spirit::{Description,Entry,Frame,Input,Kind,Magnitude,Output,Query,RecordIdentifier,RecordSet,SignalFrameError,Summary,Topic,Topics};
fn archived<T:rkyv::Archive>(){}
fn public_fields(entry:&Entry,query:&Query){let _: &Topics=&entry.topics;let _: &Kind=&entry.kind;let _: &Description=&entry.description;let _: &Magnitude=&entry.magnitude;let _: &Topic=&query.topic;let _: &Kind=&query.kind;}
#[test]
fn complete_public_surface_and_behavior(){
 archived::<Topic>();archived::<Topics>();archived::<Description>();archived::<Summary>();archived::<RecordIdentifier>();archived::<Entry>();archived::<Query>();archived::<RecordSet>();archived::<Kind>();archived::<Magnitude>();archived::<Input>();archived::<Output>();
 assert_eq!(Kind::Decision,Kind::Decision);assert_ne!(Kind::Decision,Kind::Constraint);assert_eq!(Magnitude::High,Magnitude::High);
 let _=public_fields as fn(&Entry,&Query);
 for name in [std::any::type_name::<Topic>(),std::any::type_name::<Topics>(),std::any::type_name::<Description>(),std::any::type_name::<Summary>(),std::any::type_name::<RecordIdentifier>(),std::any::type_name::<Entry>(),std::any::type_name::<Query>(),std::any::type_name::<RecordSet>(),std::any::type_name::<Kind>(),std::any::type_name::<Magnitude>(),std::any::type_name::<Input>(),std::any::type_name::<Output>()]{assert!(name.starts_with("generated_spirit::"));}
}

// The ordinary-exchange codec round-trip, both directions, against the hand-written
// reference wire expressed inline here (8-byte little-endian short header ahead of an
// rkyv archive) — the same wire the hand-written signal contracts speak. This is the
// working-programs witness for the Nomos-generated encode/decode bodies.
macro_rules! roundtrip {
 ($ty:ty,$value:expr) => {{
  let value: $ty = $value;
  // generated-encode / hand-written-decode
  let frame = value.encode_signal_frame().expect("generated encode");
  assert!(frame.len()>=8,"frame carries the short header");
  let header=u64::from_le_bytes(frame[..8].try_into().unwrap());
  assert_eq!(header,value.short_header(),"generated encode prepends the little-endian short header");
  let hand_decoded=rkyv::from_bytes::<$ty,rkyv::rancor::Error>(&frame[8..]).expect("hand-written decode of the archive tail");
  assert_eq!(hand_decoded,value,"generated-encode round-trips through the hand-written decode");
  // hand-written-encode / generated-decode
  let mut hand=value.short_header().to_le_bytes().to_vec();
  hand.extend_from_slice(&rkyv::to_bytes::<rkyv::rancor::Error>(&value).expect("hand-written encode"));
  assert_eq!(hand,frame,"the generated and hand-written encoders agree byte-for-byte");
  let (route,generated_decoded)=<$ty>::decode_signal_frame(&hand).expect("generated decode");
  assert_eq!(generated_decoded,value,"hand-written-encode round-trips through the generated decode");
  assert_eq!(route,value.route(),"the generated decode reports the operation route");
  // the guards: a corrupted header and a too-short frame are rejected loudly
  let mut corrupt=hand.clone();corrupt[0]^=0xFF;
  assert!(<$ty>::decode_signal_frame(&corrupt).is_err(),"a header that does not re-derive is rejected");
  assert!(matches!(<$ty>::decode_signal_frame(&frame[..4]),Err(SignalFrameError::FrameTooShort)),"a frame shorter than the short header is rejected");
 }};
}
#[test]
fn the_generated_codec_round_trips_every_ordinary_operation(){
 let entry=Entry{topics:Topics::new(vec![Topic::new("north-star")]),kind:Kind::Decision,description:Description::new("the ported spirit speaks the wire"),magnitude:Magnitude::High};
 let query=Query{topic:Topic::new("north-star"),kind:Kind::Constraint};
 roundtrip!(Input,Input::record(entry.clone()));
 roundtrip!(Input,Input::observe(query));
 roundtrip!(Output,Output::record_accepted(7));
 roundtrip!(Output,Output::records_observed(vec![entry]));
}

// The ordinary-exchange envelope: the generated into_frame / into_reply_frame wrap a
// payload into an ExchangeFrame (the two-way leg — no streaming/subscription body),
// and that frame round-trips through the signal-frame codec byte-for-byte. This is the
// working-programs witness for the Nomos-generated envelope surface the ported daemon
// speaks.
#[test]
fn the_generated_envelope_wraps_and_round_trips_the_exchange_frame(){
 let entry=Entry{topics:Topics::new(vec![Topic::new("north-star")]),kind:Kind::Decision,description:Description::new("the ported spirit speaks the wire"),magnitude:Magnitude::High};
 let exchange=signal_frame::ExchangeIdentifier::new(signal_frame::SessionEpoch::new(1),signal_frame::ExchangeLane::Connector,signal_frame::LaneSequence::first());
 // the request leg
 let request_value=Input::record(entry.clone());
 let expected_header=request_value.short_header();
 let request_frame:Frame=request_value.into_frame(exchange);
 assert_eq!(request_frame.short_header().value(),expected_header,"into_frame carries the operation's short header");
 match request_frame.body(){signal_frame::ExchangeFrameBody::Request{exchange:carried,..}=>assert_eq!(*carried,exchange,"the request frame echoes the exchange identifier"),other=>panic!("into_frame did not build a request body: {other:?}")}
 let request_bytes=request_frame.encode().expect("encode the request frame");
 let request_round=Frame::decode(&request_bytes).expect("decode the request frame");
 assert_eq!(request_round,request_frame,"the ordinary-exchange request frame round-trips through the signal-frame codec");
 // the reply leg
 let reply_value=Output::record_accepted(7);
 let reply_header=reply_value.short_header();
 let reply_frame:Frame=reply_value.into_reply_frame(exchange);
 assert_eq!(reply_frame.short_header().value(),reply_header,"into_reply_frame carries the operation's short header");
 match reply_frame.body(){signal_frame::ExchangeFrameBody::Reply{exchange:carried,..}=>assert_eq!(*carried,exchange,"the reply frame echoes the exchange identifier"),other=>panic!("into_reply_frame did not build a reply body: {other:?}")}
 let reply_bytes=reply_frame.encode().expect("encode the reply frame");
 let reply_round=Frame::decode(&reply_bytes).expect("decode the reply frame");
 assert_eq!(reply_round,reply_frame,"the ordinary-exchange reply frame round-trips through the signal-frame codec");
}
"#,
    )
    .unwrap();
}

/// The emitted `generated-spirit` manifest and the offline lockfile fixture pin the
/// same nota and signal-frame revisions. Both manifest revisions are single-sourced
/// from [`NOTA_REV`] / [`SIGNAL_FRAME_REV`]; this guards the separately-committed
/// [`SPIRIT_LOCK`] fixture against silently drifting from them on a chain re-pin. A
/// mismatch fails loudly here — a fast, daemon-free check — instead of red-lining the
/// buried offline `cargo test --locked` deep inside the four-process acceptance test.
#[test]
fn emitted_manifest_and_locked_fixture_pin_the_same_revisions() {
    assert!(
        SPIRIT_LOCK.contains(NOTA_REV),
        "the locked fixture must pin nota at NOTA_REV ({NOTA_REV}); a chain re-pin moved the emitted manifest without refreshing spirit-lock.toml"
    );
    assert!(
        SPIRIT_LOCK.contains(SIGNAL_FRAME_REV),
        "the locked fixture must pin signal-frame at SIGNAL_FRAME_REV ({SIGNAL_FRAME_REV}); a chain re-pin moved the emitted manifest without refreshing spirit-lock.toml"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_document_pushes_through_four_processes_and_recovers() {
    let temporary = tempfile::tempdir().unwrap();
    let sema = temporary.path().join("sema.sock");
    let schema = temporary.path().join("schema.sock");
    let nomos = temporary.path().join("nomos.sock");
    let logos = temporary.path().join("logos.sock");
    let database = temporary.path().join("isolated.sema");
    let binaries = ProductionBinaries::from_environment();
    let mut processes = Processes(vec![binaries.spawn("sema", &sema, &database, None)]);
    let mut unsupported = TestSocket {
        stream: UnixStream::connect(&sema).await.unwrap(),
        sequence: 0,
    };
    unsupported
        .stream
        .write_all(&Wire::frame_handshake_request(ProtocolVersion::new(99, 0, 0)).unwrap())
        .await
        .unwrap();
    assert!(
        !Wire::decode_frame(&unsupported.read_frame().await.unwrap())
            .unwrap()
            .is_accepted_handshake(),
        "unsupported shared wire versions are rejected before request admission"
    );
    processes
        .0
        .push(binaries.spawn("schema", &schema, &sema, None));
    processes
        .0
        .push(binaries.spawn("nomos", &nomos, &sema, Some(&schema)));
    processes
        .0
        .push(binaries.spawn("logos", &logos, &sema, Some(&nomos)));

    let mut projected = TestSocket::connect(&logos).await;
    projected
        .request(
            &signal_logos::Request::Subscribe {
                scope: FixtureScope(1),
            },
            |request| signal_logos::encode_request(request).unwrap(),
        )
        .await;
    let subscribed: signal_logos::Reply = projected.read_reply().await;
    assert!(matches!(subscribed, signal_logos::Reply::Subscribed { .. }));

    for request in [
        signal_schema::Request::StoreSignalContract {
            scope: FixtureScope(1),
            slot: SlotIdentifier(2),
            root: SignalContractRoot {
                contract: Identifier::new(0),
                streams: vec![StreamDeclaration {
                    stream: Identifier::new(1),
                }],
                opens: vec![OpensRelation {
                    operation: Identifier::new(2),
                    stream: Identifier::new(1),
                }],
                belongs: vec![BelongsRelation {
                    stream: Identifier::new(1),
                    contract: Identifier::new(0),
                }],
                names: NameTableBytes(Vec::new()),
            },
        },
        signal_schema::Request::StoreNexusRuntime {
            scope: FixtureScope(1),
            slot: SlotIdentifier(3),
            root: NexusRuntimeRoot {
                actors: vec![
                    NexusActorDeclaration {
                        actor: Identifier::new(3),
                    },
                    NexusActorDeclaration {
                        actor: Identifier::new(4),
                    },
                ],
                routes: vec![NexusRoute {
                    sender: Identifier::new(3),
                    receiver: Identifier::new(4),
                }],
                names: NameTableBytes(Vec::new()),
            },
        },
        signal_schema::Request::StoreSemaStorage {
            scope: FixtureScope(1),
            slot: SlotIdentifier(4),
            root: SemaStorageRoot {
                families: vec![FamilyDeclaration {
                    family: Identifier::new(5),
                    layout_version: 1,
                }],
                names: NameTableBytes(Vec::new()),
            },
        },
    ] {
        let reply: signal_schema::Reply = exchange(&schema, &request, |request| {
            signal_schema::encode_request(request).unwrap()
        })
        .await;
        assert!(matches!(reply, signal_schema::Reply::Stored(_)));
    }

    let schema_reply: signal_schema::Reply = exchange(
        &schema,
        &signal_schema::Request::IngestTypeSchema {
            scope: FixtureScope(1),
            slot: SlotIdentifier(1),
            legacy_text: include_str!("fixtures/spirit-min.schema").into(),
        },
        |request| signal_schema::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(schema_reply, signal_schema::Reply::Stored(_)));

    let pushed = tokio::time::timeout(
        Duration::from_secs(5),
        projected.read_reply::<signal_logos::Reply>(),
    )
    .await
    .expect("Schema → Nomos → Logos push completes without polling");
    let signal_logos::Reply::Event(event) = pushed else {
        panic!("expected projection event, got {pushed:?}");
    };
    let rust = event.rust;

    // Working-programs bar: the pipeline's emitted Rust must compile under its
    // locked manifest and pass its public-surface behavior tests. The witness no
    // longer byte-compares the emission against a schema-rust oracle projection.
    let generated = temporary.path().join("generated");
    write_crate(&generated, &rust);
    for feature_arguments in [
        &["test", "--quiet", "--locked"][..],
        &["test", "--quiet", "--locked", "--no-default-features"][..],
        &["test", "--quiet", "--locked", "--all-features"][..],
    ] {
        let status = Command::new("cargo")
            .args(feature_arguments)
            .current_dir(&generated)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "the emitted library compiles under its locked manifest and passes its public-surface behavior tests for {feature_arguments:?}"
        );
    }

    processes.terminate();
    for socket in [&sema, &schema, &nomos, &logos] {
        let _ = fs::remove_file(socket);
    }
    processes
        .0
        .push(binaries.spawn("sema", &sema, &database, None));
    processes
        .0
        .push(binaries.spawn("schema", &schema, &sema, None));
    processes
        .0
        .push(binaries.spawn("nomos", &nomos, &sema, Some(&schema)));
    processes
        .0
        .push(binaries.spawn("logos", &logos, &sema, Some(&nomos)));

    let recovered: signal_sema_storage::Reply = exchange(
        &sema,
        &signal_sema_storage::Request::Fetch {
            key: DocumentKey {
                scope: FixtureScope(1),
                kind: DocumentKind::Logos,
                slot: SlotIdentifier(1),
            },
            version: None,
        },
        |request| Wire::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(
        recovered,
        signal_sema_storage::Reply::Document(Some(_))
    ));
    for (kind, slot) in [
        (DocumentKind::SignalContract, SlotIdentifier(2)),
        (DocumentKind::NexusRuntime, SlotIdentifier(3)),
        (DocumentKind::SemaStorage, SlotIdentifier(4)),
    ] {
        let recovered_root: signal_sema_storage::Reply = exchange(
            &sema,
            &signal_sema_storage::Request::Fetch {
                key: DocumentKey {
                    scope: FixtureScope(1),
                    kind,
                    slot,
                },
                version: None,
            },
            |request| Wire::encode_request(request).unwrap(),
        )
        .await;
        let signal_sema_storage::Reply::Document(Some(document)) = recovered_root else {
            panic!("root-specific state did not recover: {kind:?}")
        };
        assert_eq!(document.payload.kind(), kind);
        assert_eq!(document.payload.validate(), Ok(()));
    }

    let mut resumed_projection = TestSocket::connect(&logos).await;
    resumed_projection
        .request(
            &signal_logos::Request::Subscribe {
                scope: FixtureScope(1),
            },
            |request| signal_logos::encode_request(request).unwrap(),
        )
        .await;
    let resubscribed: signal_logos::Reply = resumed_projection.read_reply().await;
    assert!(
        matches!(resubscribed, signal_logos::Reply::Subscribed { .. }),
        "the restarted Logos re-establishes the push subscription on its rebound socket"
    );

    let second_slot = SlotIdentifier(5);
    let second_stored: signal_schema::Reply = exchange(
        &schema,
        &signal_schema::Request::IngestTypeSchema {
            scope: FixtureScope(1),
            slot: second_slot,
            legacy_text: include_str!("fixtures/second-min.schema").into(),
        },
        |request| signal_schema::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(second_stored, signal_schema::Reply::Stored(_)));

    let resumed = tokio::time::timeout(
        Duration::from_secs(5),
        resumed_projection.read_reply::<signal_logos::Reply>(),
    )
    .await
    .expect("all four restarted daemons resume push progression without polling");
    let signal_logos::Reply::Event(second_event) = resumed else {
        panic!("expected a second projection event, got {resumed:?}");
    };

    assert_ne!(
        second_event.rust, rust,
        "the genuinely second document generates distinct Rust from the first"
    );
    assert_eq!(second_event.source.key.kind, DocumentKind::Logos);
    assert_eq!(second_event.source.key.slot, second_slot);

    let second_recovered: signal_sema_storage::Reply = exchange(
        &sema,
        &signal_sema_storage::Request::Fetch {
            key: DocumentKey {
                scope: FixtureScope(1),
                kind: DocumentKind::Logos,
                slot: second_slot,
            },
            version: None,
        },
        |request| Wire::encode_request(request).unwrap(),
    )
    .await;
    let signal_sema_storage::Reply::Document(Some(second_document)) = second_recovered else {
        panic!("the second Logos document did not store durably in Sema")
    };
    assert_eq!(second_document.payload.kind(), DocumentKind::Logos);
    assert_eq!(second_document.payload.validate(), Ok(()));
    assert_eq!(
        second_document.hash, second_event.logos,
        "the durable second document is the one the restarted pipeline just pushed"
    );
    assert_eq!(second_document.hash, second_event.source.hash);
}
