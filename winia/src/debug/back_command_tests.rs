use super::parse_back;
use crate::back::BackSwipeEdge;
use crate::back::inbox::Signal;

#[test]
fn parses_back_start_with_the_default_edge() {
    let signal = parse_back(&["back", "start", "10", "20"]).expect("start parses");
    match signal {
        Signal::Started(event) => {
            assert_eq!(event.touch, (10.0, 20.0));
            assert_eq!(event.progress, 0.0);
            assert_eq!(event.edge, BackSwipeEdge::Left);
        }
        other => panic!("expected Started, got {other:?}"),
    }
}

#[test]
fn parses_back_start_with_an_explicit_edge() {
    let signal = parse_back(&["back", "start", "1", "2", "right"]).expect("start parses");
    match signal {
        Signal::Started(event) => assert_eq!(event.edge, BackSwipeEdge::Right),
        other => panic!("expected Started, got {other:?}"),
    }
}

#[test]
fn parses_progress_and_clamps_it() {
    let signal = parse_back(&["back", "progress", "0.4", "5", "6"]).expect("progress parses");
    match signal {
        Signal::Progressed(event) => {
            assert_eq!(event.progress, 0.4);
            assert_eq!(event.touch, (5.0, 6.0));
        }
        other => panic!("expected Progressed, got {other:?}"),
    }

    let signal = parse_back(&["back", "progress", "2.0"]).expect("progress parses");
    match signal {
        Signal::Progressed(event) => assert_eq!(event.progress, 1.0),
        other => panic!("expected Progressed, got {other:?}"),
    }
}

#[test]
fn parses_the_terminal_commands() {
    assert!(matches!(
        parse_back(&["back", "cancel"]),
        Some(Signal::Cancelled)
    ));
    assert!(matches!(
        parse_back(&["back", "complete"]),
        Some(Signal::Completed)
    ));
}

#[test]
fn rejects_malformed_back_commands() {
    assert!(parse_back(&["back"]).is_none());
    assert!(parse_back(&["back", "start", "x", "y"]).is_none());
    assert!(parse_back(&["back", "start", "1", "2", "diagonal"]).is_none());
    assert!(parse_back(&["back", "progress"]).is_none());
    assert!(parse_back(&["back", "jump"]).is_none());
}
