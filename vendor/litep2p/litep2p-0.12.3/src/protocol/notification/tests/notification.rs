// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use crate::{
    mock::substream::{DummySubstream, MockSubstream},
    protocol::{
        self,
        connection::ConnectionHandle,
        notification::{
            negotiation::HandshakeEvent,
            tests::make_notification_protocol,
            types::{Direction, NotificationError, NotificationEvent},
            ConnectionState, InboundState, NotificationProtocol, OutboundState, PeerContext,
            PeerState, ValidationResult,
        },
        InnerTransportEvent, ProtocolCommand, SubstreamError,
    },
    substream::Substream,
    transport::Endpoint,
    types::{protocol::ProtocolName, ConnectionId, SubstreamId},
    PeerId,
};

use futures::StreamExt;
use multiaddr::Multiaddr;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    oneshot,
};

use std::{task::Poll, time::Duration};

fn next_inbound_state(state: usize) -> InboundState {
    match state {
        0 => InboundState::Closed,
        1 => InboundState::ReadingHandshake,
        2 => InboundState::Validating {
            inbound: Substream::new_mock(
                PeerId::random(),
                SubstreamId::from(0usize),
                Box::new(MockSubstream::new()),
            ),
        },
        3 => InboundState::SendingHandshake,
        4 => InboundState::Open {
            inbound: Substream::new_mock(
                PeerId::random(),
                SubstreamId::from(0usize),
                Box::new(MockSubstream::new()),
            ),
        },
        _ => panic!(),
    }
}

fn next_outbound_state(state: usize) -> OutboundState {
    match state {
        0 => OutboundState::Closed,
        1 => OutboundState::OutboundInitiated {
            substream: SubstreamId::new(),
        },
        2 => OutboundState::Negotiating,
        3 => OutboundState::Open {
            handshake: vec![1, 3, 3, 7],
            outbound: Substream::new_mock(
                PeerId::random(),
                SubstreamId::from(0usize),
                Box::new(MockSubstream::new()),
            ),
        },
        _ => panic!(),
    }
}

#[tokio::test]
async fn connection_closed_for_outbound_open_substream() {
    let peer = PeerId::random();

    for i in 0..5 {
        connection_closed(
            peer,
            PeerState::Validating {
                direction: Direction::Inbound,
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                outbound: OutboundState::Open {
                    handshake: vec![1, 2, 3, 4],
                    outbound: Substream::new_mock(
                        PeerId::random(),
                        SubstreamId::from(0usize),
                        Box::new(MockSubstream::new()),
                    ),
                },
                inbound: next_inbound_state(i),
            },
            Some(NotificationEvent::NotificationStreamOpenFailure {
                peer,
                error: NotificationError::Rejected,
            }),
        )
        .await;
    }
}

#[tokio::test]
async fn connection_closed_for_outbound_initiated_substream() {
    let peer = PeerId::random();

    for i in 0..5 {
        connection_closed(
            peer,
            PeerState::Validating {
                direction: Direction::Inbound,
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                outbound: OutboundState::OutboundInitiated {
                    substream: SubstreamId::from(0usize),
                },
                inbound: next_inbound_state(i),
            },
            Some(NotificationEvent::NotificationStreamOpenFailure {
                peer,
                error: NotificationError::Rejected,
            }),
        )
        .await;
    }
}

#[tokio::test]
async fn connection_closed_for_outbound_negotiated_substream() {
    let peer = PeerId::random();

    for i in 0..5 {
        connection_closed(
            peer,
            PeerState::Validating {
                direction: Direction::Inbound,
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                outbound: OutboundState::Negotiating,
                inbound: next_inbound_state(i),
            },
            Some(NotificationEvent::NotificationStreamOpenFailure {
                peer,
                error: NotificationError::Rejected,
            }),
        )
        .await;
    }
}

#[tokio::test]
async fn connection_closed_for_initiated_substream() {
    let peer = PeerId::random();

    connection_closed(
        peer,
        PeerState::OutboundInitiated {
            substream: SubstreamId::new(),
        },
        Some(NotificationEvent::NotificationStreamOpenFailure {
            peer,
            error: NotificationError::Rejected,
        }),
    )
    .await;
}

#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic]
async fn connection_established_twice() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();
    let peer = PeerId::random();

    assert!(notif.on_connection_established(peer).await.is_ok());
    assert!(notif.on_connection_established(peer).await.is_err());
}

#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic]
async fn connection_closed_twice() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();
    let peer = PeerId::random();

    assert!(notif.on_connection_closed(peer).await.is_ok());
    assert!(notif.on_connection_closed(peer).await.is_err());
}

#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic]
async fn substream_open_failure_for_unknown_substream() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();

    notif
        .on_substream_open_failure(SubstreamId::new(), SubstreamError::ConnectionClosed)
        .await;
}

#[tokio::test]
async fn close_substream_to_unknown_peer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();
    let peer = PeerId::random();

    assert!(!notif.peers.contains_key(&peer));
    notif.on_close_substream(peer).await;
    assert!(!notif.peers.contains_key(&peer));
}

#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic]
async fn handshake_event_unknown_peer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();
    let peer = PeerId::random();

    assert!(!notif.peers.contains_key(&peer));
    notif
        .on_handshake_event(
            peer,
            HandshakeEvent::Negotiated {
                peer,
                handshake: vec![1, 3, 3, 7],
                substream: Substream::new_mock(
                    peer,
                    SubstreamId::from(0usize),
                    Box::new(DummySubstream::new()),
                ),
                direction: protocol::notification::negotiation::Direction::Inbound,
            },
        )
        .await;
    assert!(!notif.peers.contains_key(&peer));
}

#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic]
async fn handshake_event_invalid_state_for_outbound_substream() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, mut tx) = make_notification_protocol();
    let (peer, _receiver) = register_peer(&mut notif, &mut tx).await;

    notif
        .on_handshake_event(
            peer,
            HandshakeEvent::Negotiated {
                peer,
                handshake: vec![1, 3, 3, 7],
                substream: Substream::new_mock(
                    peer,
                    SubstreamId::from(0usize),
                    Box::new(DummySubstream::new()),
                ),
                direction: protocol::notification::negotiation::Direction::Outbound,
            },
        )
        .await;
}

#[tokio::test]
#[cfg(debug_assertions)]
#[should_panic]
async fn substream_open_failure_for_unknown_peer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();
    let peer = PeerId::random();
    let substream_id = SubstreamId::from(1337usize);

    notif.pending_outbound.insert(substream_id, peer);
    notif
        .on_substream_open_failure(substream_id, SubstreamError::ConnectionClosed)
        .await;
}

#[tokio::test]
async fn dial_failure_for_non_dialing_peer() {
    let (mut notif, mut handle, _sender, mut tx) = make_notification_protocol();
    let (peer, _receiver) = register_peer(&mut notif, &mut tx).await;

    // dial failure for the peer even though it's not dialing
    notif.on_dial_failure(peer, vec![]).await;

    assert!(std::matches!(
        notif.peers.get(&peer),
        Some(PeerContext {
            state: PeerState::Closed { .. }
        })
    ));
    futures::future::poll_fn(|cx| match handle.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        _ => panic!("invalid event"),
    })
    .await;
}

// inbound state is ignored
async fn connection_closed(peer: PeerId, state: PeerState, event: Option<NotificationEvent>) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, mut handle, _sender, _tx) = make_notification_protocol();

    notif.peers.insert(peer, PeerContext { state });
    notif.on_connection_closed(peer).await.unwrap();

    if let Some(expected) = event {
        assert_eq!(handle.next().await.unwrap(), expected);
    }
    assert!(!notif.peers.contains_key(&peer))
}

// register new connection to `NotificationProtocol`
async fn register_peer(
    notif: &mut NotificationProtocol,
    sender: &mut Sender<InnerTransportEvent>,
) -> (PeerId, Receiver<ProtocolCommand>) {
    let peer = PeerId::random();
    let (conn_tx, conn_rx) = channel(64);

    sender
        .send(InnerTransportEvent::ConnectionEstablished {
            peer,
            connection: ConnectionId::new(),
            endpoint: Endpoint::dialer(Multiaddr::empty(), ConnectionId::from(0usize)),
            sender: ConnectionHandle::new(ConnectionId::from(0usize), conn_tx),
        })
        .await
        .unwrap();

    // poll the protocol to register the peer
    notif.next_event().await;

    assert!(std::matches!(
        notif.peers.get(&peer),
        Some(PeerContext {
            state: PeerState::Closed { .. }
        })
    ));

    (peer, conn_rx)
}

#[tokio::test]
async fn open_substream_connection_closed() {
    open_substream(PeerState::Closed { pending_open: None }, true).await;
}

#[tokio::test]
async fn open_substream_already_initiated() {
    open_substream(
        PeerState::OutboundInitiated {
            substream: SubstreamId::new(),
        },
        false,
    )
    .await;
}

#[tokio::test]
async fn open_substream_already_open() {
    let (shutdown, _rx) = oneshot::channel();
    open_substream(PeerState::Open { shutdown }, false).await;
}

#[tokio::test]
async fn open_substream_under_validation() {
    for i in 0..5 {
        for k in 0..4 {
            open_substream(
                PeerState::Validating {
                    direction: Direction::Inbound,
                    protocol: ProtocolName::from("/notif/1"),
                    fallback: None,
                    outbound: next_outbound_state(k),
                    inbound: next_inbound_state(i),
                },
                false,
            )
            .await;
        }
    }
}

async fn open_substream(state: PeerState, succeeds: bool) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, mut tx) = make_notification_protocol();
    let (peer, mut receiver) = register_peer(&mut notif, &mut tx).await;

    let context = notif.peers.get_mut(&peer).unwrap();
    context.state = state;

    notif.on_open_substream(peer).await.unwrap();
    assert!(receiver.try_recv().is_ok() == succeeds);
}

#[tokio::test]
async fn open_substream_no_connection() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, _handle, _sender, _tx) = make_notification_protocol();
    assert!(notif.on_open_substream(PeerId::random()).await.is_err());
}

#[tokio::test]
async fn remote_opens_multiple_inbound_substreams() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let protocol = ProtocolName::from("/notif/1");
    let (mut notif, _handle, _sender, mut tx) = make_notification_protocol();
    let (peer, _receiver) = register_peer(&mut notif, &mut tx).await;

    // open substream, poll the result and verify that the peer is in correct state
    tx.send(InnerTransportEvent::SubstreamOpened {
        peer,
        protocol: protocol.clone(),
        fallback: None,
        direction: protocol::Direction::Inbound,
        substream: Substream::new_mock(
            PeerId::random(),
            SubstreamId::from(0usize),
            Box::new(DummySubstream::new()),
        ),
        connection_id: ConnectionId::from(0usize),
    })
    .await
    .unwrap();
    notif.next_event().await;

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    direction: Direction::Inbound,
                    protocol,
                    fallback: None,
                    outbound: OutboundState::Closed,
                    inbound: InboundState::ReadingHandshake,
                },
        }) => {
            assert_eq!(protocol, &ProtocolName::from("/notif/1"));
        }
        state => panic!("invalid state: {state:?}"),
    }

    // try to open another substream and verify it's discarded and the state is otherwise
    // preserved
    let mut substream = MockSubstream::new();
    substream.expect_poll_close().times(1).return_once(|_| Poll::Ready(Ok(())));

    tx.send(InnerTransportEvent::SubstreamOpened {
        peer,
        protocol: protocol.clone(),
        fallback: None,
        direction: protocol::Direction::Inbound,
        substream: Substream::new_mock(
            PeerId::random(),
            SubstreamId::from(0usize),
            Box::new(substream),
        ),
        connection_id: ConnectionId::from(0usize),
    })
    .await
    .unwrap();
    notif.next_event().await;

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    direction: Direction::Inbound,
                    protocol,
                    fallback: None,
                    outbound: OutboundState::Closed,
                    inbound: InboundState::ReadingHandshake,
                },
        }) => {
            assert_eq!(protocol, &ProtocolName::from("/notif/1"));
        }
        state => panic!("invalid state: {state:?}"),
    }
}

#[tokio::test]
async fn pending_outbound_tracked_correctly() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let protocol = ProtocolName::from("/notif/1");
    let (mut notif, _handle, _sender, mut tx) = make_notification_protocol();
    let (peer, _receiver) = register_peer(&mut notif, &mut tx).await;

    // open outbound substream
    notif.on_open_substream(peer).await.unwrap();

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::OutboundInitiated { substream },
        }) => {
            assert_eq!(substream, &SubstreamId::new());
        }
        state => panic!("invalid state: {state:?}"),
    }

    // then register inbound substream and verify that the state is changed to `Validating`
    notif
        .on_inbound_substream(
            protocol.clone(),
            None,
            peer,
            Substream::new_mock(
                PeerId::random(),
                SubstreamId::from(0usize),
                Box::new(DummySubstream::new()),
            ),
        )
        .await
        .unwrap();

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    direction: Direction::Outbound,
                    outbound: OutboundState::OutboundInitiated { .. },
                    inbound: InboundState::ReadingHandshake,
                    ..
                },
        }) => {}
        state => panic!("invalid state: {state:?}"),
    }

    // then negotiation event for the inbound handshake
    notif
        .on_handshake_event(
            peer,
            HandshakeEvent::Negotiated {
                peer,
                handshake: vec![1, 3, 3, 7],
                substream: Substream::new_mock(
                    PeerId::random(),
                    SubstreamId::from(0usize),
                    Box::new(DummySubstream::new()),
                ),
                direction: protocol::notification::negotiation::Direction::Inbound,
            },
        )
        .await;

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    direction: Direction::Outbound,
                    outbound: OutboundState::OutboundInitiated { .. },
                    inbound: InboundState::Validating { .. },
                    ..
                },
        }) => {}
        state => panic!("invalid state: {state:?}"),
    }

    // then reject the inbound peer even though an outbound substream was already established
    notif.on_validation_result(peer, ValidationResult::Reject).await.unwrap();

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::Closed { pending_open },
        }) => {
            assert_eq!(pending_open, &Some(SubstreamId::new()));
        }
        state => panic!("invalid state: {state:?}"),
    }

    // finally the outbound substream registers, verify that `pending_open` is set to `None`
    notif
        .on_outbound_substream(
            protocol,
            None,
            peer,
            SubstreamId::new(),
            Substream::new_mock(
                PeerId::random(),
                SubstreamId::from(0usize),
                Box::new(DummySubstream::new()),
            ),
        )
        .await
        .unwrap();

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::Closed { pending_open },
        }) => {
            assert!(pending_open.is_none());
        }
        state => panic!("invalid state: {state:?}"),
    }
}

#[tokio::test]
async fn inbound_accepted_outbound_fails_to_open() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let protocol = ProtocolName::from("/notif/1");
    let (mut notif, mut handle, sender, mut tx) = make_notification_protocol();
    let (peer, receiver) = register_peer(&mut notif, &mut tx).await;

    // register inbound substream and verify that the state is `Validating`
    notif
        .on_inbound_substream(
            protocol.clone(),
            None,
            peer,
            Substream::new_mock(
                PeerId::random(),
                SubstreamId::from(0usize),
                Box::new(DummySubstream::new()),
            ),
        )
        .await
        .unwrap();

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    direction: Direction::Inbound,
                    outbound: OutboundState::Closed,
                    inbound: InboundState::ReadingHandshake,
                    ..
                },
        }) => {}
        state => panic!("invalid state: {state:?}"),
    }

    // then negotiation event for the inbound handshake
    notif
        .on_handshake_event(
            peer,
            HandshakeEvent::Negotiated {
                peer,
                handshake: vec![1, 3, 3, 7],
                substream: Substream::new_mock(
                    PeerId::random(),
                    SubstreamId::from(0usize),
                    Box::new(DummySubstream::new()),
                ),
                direction: protocol::notification::negotiation::Direction::Inbound,
            },
        )
        .await;

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    direction: Direction::Inbound,
                    outbound: OutboundState::Closed,
                    inbound: InboundState::Validating { .. },
                    ..
                },
        }) => {}
        state => panic!("invalid state: {state:?}"),
    }

    // discard the validation event
    assert!(tokio::time::timeout(Duration::from_secs(5), handle.next()).await.is_ok());

    // before the validation event is registered, close the connection
    drop(sender);
    drop(receiver);
    drop(tx);

    // then reject the inbound peer even though an outbound substream was already established
    assert!(notif.on_validation_result(peer, ValidationResult::Accept).await.is_err());

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::Closed { pending_open },
        }) => {
            assert!(pending_open.is_none());
        }
        state => panic!("invalid state: {state:?}"),
    }

    // verify that the user is not reported anything
    match tokio::time::timeout(Duration::from_secs(1), handle.next()).await {
        Err(_) => panic!("unexpected timeout"),
        Ok(Some(NotificationEvent::NotificationStreamOpenFailure {
            peer: event_peer,
            error,
        })) => {
            assert_eq!(peer, event_peer);
            assert_eq!(error, NotificationError::Rejected)
        }
        _ => panic!("invalid event"),
    }
}

#[tokio::test]
async fn open_substream_on_closed_connection() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, mut handle, sender, mut tx) = make_notification_protocol();
    let (peer, receiver) = register_peer(&mut notif, &mut tx).await;

    // before processing the open substream event, close the connection
    drop(sender);
    drop(receiver);
    drop(tx);

    // open outbound substream
    notif.on_open_substream(peer).await.unwrap();

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::Closed { pending_open: None },
        }) => {}
        state => panic!("invalid state: {state:?}"),
    }

    match tokio::time::timeout(Duration::from_secs(5), handle.next())
        .await
        .expect("operation to succeed")
    {
        Some(NotificationEvent::NotificationStreamOpenFailure { error, .. }) => {
            assert_eq!(error, NotificationError::NoConnection);
        }
        event => panic!("invalid event received: {event:?}"),
    }
}

// `NotificationHandle` may have an inconsistent view of the peer state and connection to peer may
// already been closed by the time `close_substream()` is called but this event hasn't yet been
// registered to `NotificationHandle` which causes it to send a stale disconnection request to
// `NotificationProtocol`.
//
// verify that `NotificationProtocol` ignores stale disconnection requests
#[tokio::test]
async fn close_already_closed_connection() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, mut handle, _, mut tx) = make_notification_protocol();
    let (peer, _) = register_peer(&mut notif, &mut tx).await;

    notif.peers.insert(
        peer,
        PeerContext {
            state: PeerState::Validating {
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                direction: Direction::Inbound,
                outbound: OutboundState::Open {
                    handshake: vec![1, 2, 3, 4],
                    outbound: Substream::new_mock(
                        PeerId::random(),
                        SubstreamId::from(0usize),
                        Box::new(MockSubstream::new()),
                    ),
                },
                inbound: InboundState::SendingHandshake,
            },
        },
    );
    notif
        .on_handshake_event(
            peer,
            HandshakeEvent::Negotiated {
                peer,
                handshake: vec![1],
                substream: Substream::new_mock(
                    PeerId::random(),
                    SubstreamId::from(0usize),
                    Box::new(MockSubstream::new()),
                ),
                direction: protocol::notification::negotiation::Direction::Inbound,
            },
        )
        .await;

    match handle.next().await {
        Some(NotificationEvent::NotificationStreamOpened { .. }) => {}
        _ => panic!("invalid event received"),
    }

    // close the substream but don't poll the `NotificationHandle`
    notif.shutdown_tx.send(peer).await.unwrap();

    // close the connection using the handle
    handle.close_substream(peer).await;

    // process the events
    notif.next_event().await;
    notif.next_event().await;

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::Closed { pending_open: None },
        }) => {}
        state => panic!("invalid state: {state:?}"),
    }
}

/// Notification state was not reset correctly if the outbound substream failed to open after
/// inbound substream had been negotiated, causing `NotificationProtocol` to report open failure
/// twice, once when the failure occurred and again when the connection was closed.
#[tokio::test]
async fn open_failure_reported_once() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, mut handle, _, mut tx) = make_notification_protocol();
    let (peer, _) = register_peer(&mut notif, &mut tx).await;

    // move `peer` to state where the inbound substream has been negotiated
    // and the local node has initiated an outbound substream
    notif.peers.insert(
        peer,
        PeerContext {
            state: PeerState::Validating {
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                direction: Direction::Inbound,
                outbound: OutboundState::OutboundInitiated {
                    substream: SubstreamId::from(1337usize),
                },
                inbound: InboundState::Open {
                    inbound: Substream::new_mock(
                        peer,
                        SubstreamId::from(0usize),
                        Box::new(DummySubstream::new()),
                    ),
                },
            },
        },
    );
    notif.pending_outbound.insert(SubstreamId::from(1337usize), peer);

    notif
        .on_substream_open_failure(
            SubstreamId::from(1337usize),
            SubstreamError::ConnectionClosed,
        )
        .await;

    match handle.next().await {
        Some(NotificationEvent::NotificationStreamOpenFailure {
            peer: failed_peer,
            error,
        }) => {
            assert_eq!(failed_peer, peer);
            assert_eq!(error, NotificationError::Rejected);
        }
        _ => panic!("invalid event received"),
    }

    match notif.peers.get(&peer) {
        Some(PeerContext {
            state: PeerState::Closed { pending_open },
        }) => {
            assert_eq!(pending_open, &Some(SubstreamId::from(1337usize)));
        }
        state => panic!("invalid state for peer: {state:?}"),
    }

    // connection to `peer` is closed
    notif.on_connection_closed(peer).await.unwrap();

    futures::future::poll_fn(|cx| match handle.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        result => panic!("didn't expect event from channel, got {result:?}"),
    })
    .await;
}

// inboud substrem was received and it was sent to user for validation
//
// the validation took so long that remote opened another substream while validation for the
// previous inbound substrem was still pending
//
// verify that the new substream is rejected and that the peer state is set to `ValidationPending`
#[tokio::test]
async fn second_inbound_substream_rejected() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, mut handle, _, mut tx) = make_notification_protocol();
    let (peer, _) = register_peer(&mut notif, &mut tx).await;

    // move peer state to `Validating`
    let mut substream1 = MockSubstream::new();
    substream1.expect_poll_close().times(1).return_once(|_| Poll::Ready(Ok(())));

    notif.peers.insert(
        peer,
        PeerContext {
            state: PeerState::Validating {
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                direction: Direction::Inbound,
                outbound: OutboundState::Closed,
                inbound: InboundState::Validating {
                    inbound: Substream::new_mock(
                        peer,
                        SubstreamId::from(0usize),
                        Box::new(substream1),
                    ),
                },
            },
        },
    );

    // open a new inbound substream because validation took so long that `peer` decided
    // to open a new substream
    let mut substream2 = MockSubstream::new();
    substream2.expect_poll_close().times(1).return_once(|_| Poll::Ready(Ok(())));
    notif
        .on_inbound_substream(
            ProtocolName::from("/notif/1"),
            None,
            peer,
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream2)),
        )
        .await
        .unwrap();

    // verify that peer is moved to `ValidationPending`
    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::ValidationPending {
                    state: ConnectionState::Open,
                },
        }) => {}
        state => panic!("invalid state for peer: {state:?}"),
    }

    // user decide to reject the substream, verify that nothing is received over the event handle
    notif.on_validation_result(peer, ValidationResult::Reject).await.unwrap();

    notif.on_connection_closed(peer).await.unwrap();
    futures::future::poll_fn(|cx| match handle.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(()),
        result => panic!("didn't expect event from channel, got {result:?}"),
    })
    .await;
}

// remote opened a substream, it was accepted by the local node and local node opened an outbound
// substream but it took so long to open that the inbound substream was closed and while the
// outbound substream was opening, another inbound substream was received from peer
//
// verify that this second inbound substream is rejected as an outbound substream for the previous
// connection is still pending
#[tokio::test]
async fn second_inbound_substream_opened_while_outbound_substream_was_opening() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut notif, mut handle, _zz, mut tx) = make_notification_protocol();
    let (peer, _zz) = register_peer(&mut notif, &mut tx).await;

    // move peer state to `Validating`
    let mut substream1 = MockSubstream::new();
    substream1
        .expect_poll_ready()
        .times(1)
        .return_once(|_| Poll::Ready(Err(SubstreamError::ConnectionClosed)));

    notif.peers.insert(
        peer,
        PeerContext {
            state: PeerState::Validating {
                protocol: ProtocolName::from("/notif/1"),
                fallback: None,
                direction: Direction::Inbound,
                outbound: OutboundState::Closed,
                inbound: InboundState::Validating {
                    inbound: Substream::new_mock(
                        peer,
                        SubstreamId::from(0usize),
                        Box::new(substream1),
                    ),
                },
            },
        },
    );

    // accept the inbound substream which is now closed
    notif.on_validation_result(peer, ValidationResult::Accept).await.unwrap();

    // verify that peer is sending handshake and that outbound substream is opening
    let substream_id = match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Validating {
                    fallback: None,
                    direction: Direction::Inbound,
                    outbound: OutboundState::OutboundInitiated { substream },
                    inbound: InboundState::SendingHandshake,
                    ..
                },
        }) => *substream,
        state => panic!("invalid state for peer: {state:?}"),
    };

    // poll the protocol and send handshake over the inbound substream
    notif.next_event().await;

    // verify that peer is closed
    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Closed {
                    pending_open: Some(pending_open),
                },
        }) => {
            assert_eq!(substream_id, *pending_open);
        }
        state => panic!("invalid state for peer: {state:?}"),
    }

    match handle.next().await {
        Some(NotificationEvent::NotificationStreamOpenFailure { .. }) => {}
        _ => panic!("invalid event received"),
    }

    // remote open second inbound substream
    let mut substream2 = MockSubstream::new();
    substream2.expect_poll_close().times(1).return_once(|_| Poll::Ready(Ok(())));

    notif
        .on_inbound_substream(
            ProtocolName::from("/notif/1"),
            None,
            peer,
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream2)),
        )
        .await
        .unwrap();

    // verify that peer is still closed
    match notif.peers.get(&peer) {
        Some(PeerContext {
            state:
                PeerState::Closed {
                    pending_open: Some(pending_open),
                },
        }) => {
            assert_eq!(substream_id, *pending_open);
        }
        state => panic!("invalid state for peer: {state:?}"),
    }
}

#[tokio::test]
async fn drop_handle_exits_protocol() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let (mut protocol, handle, _sender, _tx) = make_notification_protocol();

    // Simulate a handle drop.
    drop(handle);

    // Call `next_event` and ensure it returns true.
    let result = protocol.next_event().await;
    assert!(
        result,
        "Expected `next_event` to return true when `command_rx` is dropped"
    );
}
