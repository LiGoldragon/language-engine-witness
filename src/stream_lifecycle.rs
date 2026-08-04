//! Executable component-owned witness for the strict generated stream contract.
//!
//! The data types mirror the explicit Rust Logos lifecycle shape: initiation
//! accepts a typed query and returns `protos::Stream<Event>` directly, while
//! termination accepts that exact handle through its own typed input. Registry,
//! monotonic identity allocation, and event-queue behavior live here in the
//! component runtime rather than in Protos, Nomos, or the archiveable Logos
//! contract.

use std::collections::{BTreeMap, VecDeque};

use interface_protos::{Input, Refusal, Stream, StreamEvent, StreamIdentity, StreamOpen};

/// The generated observer stream declaration marker.
pub struct ObserverStream;

/// Typed query accepted by generated observer-stream initiation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObserverInitiation {
    /// A non-empty subject establishes one observation stream.
    pub subject: String,
}

impl Input for ObserverInitiation {}

/// Event delivered through one established observer stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationEvent {
    /// Component-local event sequence for this witness.
    pub sequence: u64,
}

/// Direct initiation success: the typed stream handle emitted by Rust Logos.
pub type ObserverHandle = Stream<ObservationEvent>;

/// Typed refusal for an invalid initiation query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObserverInitiationRefusal {
    /// The query did not select an observable subject.
    InvalidQuery,
}

impl std::fmt::Display for ObserverInitiationRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidQuery => formatter.write_str("invalid observer-stream query"),
        }
    }
}

impl std::error::Error for ObserverInitiationRefusal {}
impl Refusal for ObserverInitiationRefusal {}

/// Separate generated input for termination over an established typed handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObserverTermination {
    /// The exact typed handle returned by successful initiation.
    pub stream: ObserverHandle,
}

impl Input for ObserverTermination {}

/// Typed termination failures owned by the component runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObserverTerminationRefusal {
    /// No stream was ever established for this typed handle identity.
    UnknownStream,
    /// The handle names a known stream that has already been terminated.
    AlreadyClosed,
}

impl std::fmt::Display for ObserverTerminationRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStream => formatter.write_str("unknown observer stream"),
            Self::AlreadyClosed => formatter.write_str("observer stream already closed"),
        }
    }
}

impl std::error::Error for ObserverTerminationRefusal {}
impl Refusal for ObserverTerminationRefusal {}

#[derive(Debug)]
struct ObserverStreamState {
    open: bool,
    events: VecDeque<ObservationEvent>,
}

/// Component runtime that owns strict-stream behavior for this witness.
///
/// This intentionally has no persistence or restart contract. Its monotonic
/// identities and queues demonstrate only the live runtime behavior excluded
/// from the portable Protos/Nomos/Logos layers.
#[derive(Debug)]
pub struct ObserverRuntime {
    next_identity: u64,
    streams: BTreeMap<u64, ObserverStreamState>,
}

impl Default for ObserverRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ObserverRuntime {
    /// Construct a fresh component runtime with its first live identity at one.
    pub fn new() -> Self {
        Self {
            next_identity: 1,
            streams: BTreeMap::new(),
        }
    }

    /// Apply the generated termination input without defining a third universal
    /// close trait.
    pub fn terminate(
        &mut self,
        termination: ObserverTermination,
    ) -> Result<(), ObserverTerminationRefusal> {
        let identity = termination.stream.identity().value();
        let Some(state) = self.streams.get_mut(&identity) else {
            return Err(ObserverTerminationRefusal::UnknownStream);
        };
        if !state.open {
            return Err(ObserverTerminationRefusal::AlreadyClosed);
        }
        state.open = false;
        state.events.clear();
        Ok(())
    }
}

impl StreamOpen for ObserverRuntime {
    type Initiation = ObserverInitiation;
    type Event = ObservationEvent;
    type InitiationRefusal = ObserverInitiationRefusal;

    fn open(
        &mut self,
        initiation: Self::Initiation,
    ) -> Result<Stream<Self::Event>, Self::InitiationRefusal> {
        if initiation.subject.trim().is_empty() {
            return Err(ObserverInitiationRefusal::InvalidQuery);
        }
        let identity = self.next_identity;
        self.next_identity = self
            .next_identity
            .checked_add(1)
            .expect("observer witness identity counter exhausted");
        self.streams.insert(
            identity,
            ObserverStreamState {
                open: true,
                events: VecDeque::from([ObservationEvent { sequence: identity }]),
            },
        );
        Ok(Stream::new(StreamIdentity::new(identity)))
    }
}

impl StreamEvent for ObserverRuntime {
    type Event = ObservationEvent;

    fn next(&mut self, stream: &Stream<Self::Event>) -> Option<Self::Event> {
        self.streams
            .get_mut(&stream.identity().value())
            .filter(|state| state.open)
            .and_then(|state| state.events.pop_front())
    }
}
