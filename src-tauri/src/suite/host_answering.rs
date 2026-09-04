//! What happens to a request when the extension host stops answering.
//!
//! `P0-08` fixed two failures that look identical from the window and are not.
//! A host that has **died** ends its stream, and everything waiting on it has
//! to be told; a host that is **wedged** never ends anything, and the calls
//! made to it have to give up on their own. Neither is a slow extension.
//!
//! `tests/exthost.rs` covers the first from outside: it kills a real Node and
//! watches `alive()` turn over. What it cannot do is separate the two halves of
//! why a later request fails, and that matters, because one of them is a race.
//! Measured on this tree by removing the `closed` check: the integration test
//! did not fail, **it hung**, waiting for a reply from a process that no longer
//! existed. A run that never ends is worse than one that fails, because CI
//! reports it as a timeout with no name attached.
//!
//! So these are unit tests over `RpcPeer` alone, with a deadline on every wait.
//! No Node, no pipes, no `AppHandle`: the only thing under test is whether a
//! caller is answered, and every one of them either answers or fails by name.

use std::time::Duration;

use serde_json::json;

use crate::exthost::RpcPeer;

/// Long enough that a working answer arrives, short enough that a hang is a
/// failed test rather than a stalled suite.
const DEADLINE: Duration = Duration::from_secs(2);

/// Everything waiting when the stream ends is told, rather than left holding a
/// `oneshot` whose sender has been dropped.
///
/// A leaked sender never resolves. Before `give_up_on_everything` a launch, a
/// render or a form submission simply waited for as long as the launcher was
/// open, showing the view it had when the host died.
#[tokio::test]
async fn a_request_in_flight_is_failed_when_the_host_goes() {
    let (peer, _outbound, _incoming) = RpcPeer::new();

    let waiting = {
        let peer = peer.clone();
        tokio::spawn(async move { peer.request("Manager/load", json!({})).await })
    };

    // The request has to be in the pending map before the stream ends, or this
    // would be testing the post-death path below instead.
    tokio::time::sleep(Duration::from_millis(20)).await;

    peer.give_up_on_everything("the extension host stopped");

    let answered = tokio::time::timeout(DEADLINE, waiting)
        .await
        .expect("a request in flight was never answered")
        .expect("the waiting task panicked");

    let err = answered.expect_err("a dead host answered a request");
    assert!(
        err.message.contains("stopped"),
        "the failure has to say the host went, not something generic: {err}",
    );
}

/// A request made *after* the death fails at once, rather than depending on a
/// race with the writer.
///
/// This is the case the window actually hits, because a crash is when it
/// retries. Failing everything already pending is not enough on its own: a
/// fresh request inserts a fresh entry into the map that nothing will ever
/// answer, and whether the send refuses depends on the writer task having
/// noticed the broken pipe yet.
///
/// The outbound receiver is deliberately **held open here**, which is what
/// makes this a real test of the `closed` flag rather than of the channel.
/// With the receiver alive the send always succeeds, so nothing but the flag
/// can stop the caller waiting forever.
#[tokio::test]
async fn a_request_made_after_the_host_died_fails_rather_than_waiting() {
    let (peer, outbound, _incoming) = RpcPeer::new();

    peer.give_up_on_everything("the extension host stopped");

    let answered = tokio::time::timeout(DEADLINE, peer.request("Manager/load", json!({})))
        .await
        .expect("a request after the host died waited for a reply nobody will send");

    assert!(
        answered.is_err(),
        "a dead host answered a request made after it died"
    );

    // Named after the assertions so it cannot be dropped early by a compiler
    // that has stopped seeing it used.
    drop(outbound);
}

/// And a live peer still answers, which is what stops the two above passing
/// for a peer that refuses everything.
///
/// The positive control. Without it a `request` hard-coded to fail would keep
/// this whole file green.
#[tokio::test]
async fn a_peer_that_has_not_been_given_up_on_still_answers() {
    let (peer, mut outbound, _incoming) = RpcPeer::new();

    let asking = {
        let peer = peer.clone();
        tokio::spawn(async move { peer.request("Manager/load", json!({})).await })
    };

    // The id is the peer's own, read off the frame it wrote, so the reply is
    // addressed the way the host would address it.
    let sent = tokio::time::timeout(DEADLINE, outbound.recv())
        .await
        .expect("the request was never written")
        .expect("the outbound channel closed");

    let frame: serde_json::Value = serde_json::from_str(&sent).expect("a JSON-RPC frame");
    let id = frame["id"].as_u64().expect("the request carries an id");

    peer.receive(&json!({ "jsonrpc": "2.0", "id": id, "result": "loaded" }).to_string());

    let answered = tokio::time::timeout(DEADLINE, asking)
        .await
        .expect("a live peer never answered")
        .expect("the waiting task panicked")
        .expect("a live peer failed a request");

    assert_eq!(answered, json!("loaded"));
}

/// Giving up twice is not a panic, and does not resurrect anything.
///
/// The reader task calls this once, but `unload` and a shutdown can both be
/// on their way down when it does. A second call has to be a no-op rather than
/// a poisoned lock behind every later launch.
#[tokio::test]
async fn giving_up_is_safe_to_do_twice() {
    let (peer, _outbound, _incoming) = RpcPeer::new();

    peer.give_up_on_everything("the extension host stopped");
    peer.give_up_on_everything("and again");

    let answered = tokio::time::timeout(DEADLINE, peer.request("Manager/load", json!({})))
        .await
        .expect("a request after two givings-up waited forever");

    assert!(answered.is_err());
}
