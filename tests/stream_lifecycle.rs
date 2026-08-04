//! Process-level behavior witness for the strict generated observer stream.

use interface_protos::{Input, Refusal, Stream, StreamEvent, StreamIdentity, StreamOpen};
use language_engine_witness::stream_lifecycle::{
    ObservationEvent, ObserverInitiation, ObserverInitiationRefusal, ObserverRuntime,
    ObserverTermination, ObserverTerminationRefusal,
};

fn assert_input<Value: Input>() {}
fn assert_refusal<Value: Refusal>() {}

#[test]
fn generated_stream_contract_drives_live_runtime_lifecycle_refusals() {
    assert_input::<ObserverInitiation>();
    assert_input::<ObserverTermination>();
    assert_refusal::<ObserverInitiationRefusal>();
    assert_refusal::<ObserverTerminationRefusal>();

    let mut runtime = ObserverRuntime::new();
    assert_eq!(
        runtime.open(ObserverInitiation {
            subject: "   ".to_owned(),
        }),
        Err(ObserverInitiationRefusal::InvalidQuery)
    );

    let first = runtime
        .open(ObserverInitiation {
            subject: "guardian".to_owned(),
        })
        .expect("valid query establishes a typed stream");
    let second = runtime
        .open(ObserverInitiation {
            subject: "records".to_owned(),
        })
        .expect("second valid query establishes a distinct typed stream");
    assert_eq!(first.identity().value(), 1);
    assert_eq!(second.identity().value(), 2);
    assert_eq!(runtime.next(&first), Some(ObservationEvent { sequence: 1 }));
    assert_eq!(runtime.next(&first), None);

    runtime
        .terminate(ObserverTermination {
            stream: first.clone(),
        })
        .expect("first termination succeeds");
    assert_eq!(
        runtime.terminate(ObserverTermination { stream: first }),
        Err(ObserverTerminationRefusal::AlreadyClosed)
    );
    assert_eq!(
        runtime.terminate(ObserverTermination {
            stream: Stream::new(StreamIdentity::new(999)),
        }),
        Err(ObserverTerminationRefusal::UnknownStream)
    );
}
