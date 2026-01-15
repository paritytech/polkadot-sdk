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
    protocol::libp2p::kademlia::query::QueryId, substream::Substream,
    utils::futures_stream::FuturesStream, PeerId,
};

use bytes::{Bytes, BytesMut};
use futures::{future::BoxFuture, Stream, StreamExt};

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

/// Read timeout for inbound messages.
const READ_TIMEOUT: Duration = Duration::from_secs(15);
/// Write timeout for outbound messages.
const WRITE_TIMEOUT: Duration = Duration::from_secs(15);

/// Faulure reason.
#[derive(Debug)]
pub enum FailureReason {
    /// Substream was closed while reading/writing message to remote peer.
    SubstreamClosed,

    /// Timeout while reading/writing to substream.
    Timeout,
}

/// Query result.
#[derive(Debug)]
pub enum QueryResult {
    /// Message was sent to remote peer successfully.
    /// This result is only reported for send-only queries. Queries that include reading a
    /// response won't report it and will only yield a [`QueryResult::ReadSuccess`].
    SendSuccess {
        /// Substream.
        substream: Substream,
    },

    /// Failed to send message to remote peer.
    SendFailure {
        /// Failure reason.
        reason: FailureReason,
    },

    /// Message was read from the remote peer successfully.
    ReadSuccess {
        /// Substream.
        substream: Substream,

        /// Read message.
        message: BytesMut,
    },

    /// Failed to read message from remote peer.
    ReadFailure {
        /// Failure reason.
        reason: FailureReason,
    },

    /// Result that must be treated as send success. This is needed as a workaround to support
    /// older litep2p nodes not sending `PUT_VALUE` ACK messages and not reading them.
    // TODO: remove this as part of https://github.com/paritytech/litep2p/issues/429.
    AssumeSendSuccess,
}

/// Query result.
#[derive(Debug)]
pub struct QueryContext {
    /// Peer ID.
    pub peer: PeerId,

    /// Query ID.
    pub query_id: Option<QueryId>,

    /// Query result.
    pub result: QueryResult,
}

/// Query executor.
pub struct QueryExecutor {
    /// Pending futures.
    futures: FuturesStream<BoxFuture<'static, QueryContext>>,
}

impl QueryExecutor {
    /// Create new [`QueryExecutor`]
    pub fn new() -> Self {
        Self {
            futures: FuturesStream::new(),
        }
    }

    /// Send message to remote peer.
    pub fn send_message(
        &mut self,
        peer: PeerId,
        query_id: Option<QueryId>,
        message: Bytes,
        mut substream: Substream,
    ) {
        self.futures.push(Box::pin(async move {
            match tokio::time::timeout(WRITE_TIMEOUT, substream.send_framed(message)).await {
                // Timeout error.
                Err(_) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::SendFailure {
                        reason: FailureReason::Timeout,
                    },
                },
                // Writing message to substream failed.
                Ok(Err(_)) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::SendFailure {
                        reason: FailureReason::SubstreamClosed,
                    },
                },
                Ok(Ok(())) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::SendSuccess { substream },
                },
            }
        }));
    }

    /// Send message and ignore sending errors.
    ///
    /// This is a hackish way of dealing with older litep2p nodes not expecting receiving
    /// `PUT_VALUE` ACK messages. This should eventually be removed.
    // TODO: remove this as part of https://github.com/paritytech/litep2p/issues/429.
    pub fn send_message_eat_failure(
        &mut self,
        peer: PeerId,
        query_id: Option<QueryId>,
        message: Bytes,
        mut substream: Substream,
    ) {
        self.futures.push(Box::pin(async move {
            match tokio::time::timeout(WRITE_TIMEOUT, substream.send_framed(message)).await {
                // Timeout error.
                Err(_) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::AssumeSendSuccess,
                },
                // Writing message to substream failed.
                Ok(Err(_)) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::AssumeSendSuccess,
                },
                Ok(Ok(())) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::SendSuccess { substream },
                },
            }
        }));
    }

    /// Read message from remote peer with timeout.
    pub fn read_message(
        &mut self,
        peer: PeerId,
        query_id: Option<QueryId>,
        mut substream: Substream,
    ) {
        self.futures.push(Box::pin(async move {
            match tokio::time::timeout(READ_TIMEOUT, substream.next()).await {
                Err(_) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadFailure {
                        reason: FailureReason::Timeout,
                    },
                },
                Ok(Some(Ok(message))) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadSuccess { substream, message },
                },
                Ok(None) | Ok(Some(Err(_))) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadFailure {
                        reason: FailureReason::SubstreamClosed,
                    },
                },
            }
        }));
    }

    /// Send request to remote peer and read response.
    pub fn send_request_read_response(
        &mut self,
        peer: PeerId,
        query_id: Option<QueryId>,
        message: Bytes,
        mut substream: Substream,
    ) {
        self.futures.push(Box::pin(async move {
            match tokio::time::timeout(WRITE_TIMEOUT, substream.send_framed(message)).await {
                // Timeout error.
                Err(_) =>
                    return QueryContext {
                        peer,
                        query_id,
                        result: QueryResult::SendFailure {
                            reason: FailureReason::Timeout,
                        },
                    },
                // Writing message to substream failed.
                Ok(Err(_)) => {
                    let _ = substream.close().await;
                    return QueryContext {
                        peer,
                        query_id,
                        result: QueryResult::SendFailure {
                            reason: FailureReason::SubstreamClosed,
                        },
                    };
                }
                // This will result in either `SendAndReadSuccess` or `SendSuccessReadFailure`.
                Ok(Ok(())) => (),
            };

            match tokio::time::timeout(READ_TIMEOUT, substream.next()).await {
                Err(_) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadFailure {
                        reason: FailureReason::Timeout,
                    },
                },
                Ok(Some(Ok(message))) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadSuccess { substream, message },
                },
                Ok(None) | Ok(Some(Err(_))) => QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadFailure {
                        reason: FailureReason::SubstreamClosed,
                    },
                },
            }
        }));
    }

    /// Send request to remote peer and read the response, ignoring it and any read errors.
    ///
    /// This is a hackish way of dealing with older litep2p nodes not sending `PUT_VALUE` ACK
    /// messages. This should eventually be removed.
    // TODO: remove this as part of https://github.com/paritytech/litep2p/issues/429.
    pub fn send_request_eat_response_failure(
        &mut self,
        peer: PeerId,
        query_id: Option<QueryId>,
        message: Bytes,
        mut substream: Substream,
    ) {
        self.futures.push(Box::pin(async move {
            match tokio::time::timeout(WRITE_TIMEOUT, substream.send_framed(message)).await {
                // Timeout error.
                Err(_) =>
                    return QueryContext {
                        peer,
                        query_id,
                        result: QueryResult::SendFailure {
                            reason: FailureReason::Timeout,
                        },
                    },
                // Writing message to substream failed.
                Ok(Err(_)) => {
                    let _ = substream.close().await;
                    return QueryContext {
                        peer,
                        query_id,
                        result: QueryResult::SendFailure {
                            reason: FailureReason::SubstreamClosed,
                        },
                    };
                }
                // This will result in either `SendAndReadSuccess` or `SendSuccessReadFailure`.
                Ok(Ok(())) => (),
            };

            // Ignore the read result (including errors).
            if let Ok(Some(Ok(message))) =
                tokio::time::timeout(READ_TIMEOUT, substream.next()).await
            {
                QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::ReadSuccess { substream, message },
                }
            } else {
                QueryContext {
                    peer,
                    query_id,
                    result: QueryResult::AssumeSendSuccess,
                }
            }
        }));
    }
}

impl Stream for QueryExecutor {
    type Item = QueryContext;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.futures.poll_next_unpin(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::substream::MockSubstream, types::SubstreamId};

    #[tokio::test]
    async fn substream_read_timeout() {
        let mut executor = QueryExecutor::new();
        let peer = PeerId::random();
        let mut substream = MockSubstream::new();
        substream.expect_poll_next().returning(|_| Poll::Pending);
        let substream = Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream));

        executor.read_message(peer, None, substream);

        match tokio::time::timeout(Duration::from_secs(20), executor.next()).await {
            Ok(Some(QueryContext {
                peer: queried_peer,
                query_id,
                result,
            })) => {
                assert_eq!(peer, queried_peer);
                assert!(query_id.is_none());
                assert!(std::matches!(
                    result,
                    QueryResult::ReadFailure {
                        reason: FailureReason::Timeout
                    }
                ));
            }
            result => panic!("invalid result received: {result:?}"),
        }
    }

    #[tokio::test]
    async fn substream_read_substream_closed() {
        let mut executor = QueryExecutor::new();
        let peer = PeerId::random();
        let mut substream = MockSubstream::new();
        substream.expect_poll_next().times(1).return_once(|_| {
            Poll::Ready(Some(Err(crate::error::SubstreamError::ConnectionClosed)))
        });

        executor.read_message(
            peer,
            Some(QueryId(1338)),
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream)),
        );

        match tokio::time::timeout(Duration::from_secs(20), executor.next()).await {
            Ok(Some(QueryContext {
                peer: queried_peer,
                query_id,
                result,
            })) => {
                assert_eq!(peer, queried_peer);
                assert_eq!(query_id, Some(QueryId(1338)));
                assert!(std::matches!(
                    result,
                    QueryResult::ReadFailure {
                        reason: FailureReason::SubstreamClosed
                    }
                ));
            }
            result => panic!("invalid result received: {result:?}"),
        }
    }

    #[tokio::test]
    async fn send_succeeds_no_message_read() {
        let mut executor = QueryExecutor::new();
        let peer = PeerId::random();

        // prepare substream which succeeds in sending the message but closes right after
        let mut substream = MockSubstream::new();
        substream.expect_poll_ready().times(1).return_once(|_| Poll::Ready(Ok(())));
        substream.expect_start_send().times(1).return_once(|_| Ok(()));
        substream.expect_poll_flush().times(1).return_once(|_| Poll::Ready(Ok(())));
        substream.expect_poll_next().times(1).return_once(|_| {
            Poll::Ready(Some(Err(crate::error::SubstreamError::ConnectionClosed)))
        });

        executor.send_request_read_response(
            peer,
            Some(QueryId(1337)),
            Bytes::from_static(b"hello, world"),
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream)),
        );

        match tokio::time::timeout(Duration::from_secs(20), executor.next()).await {
            Ok(Some(QueryContext {
                peer: queried_peer,
                query_id,
                result,
            })) => {
                assert_eq!(peer, queried_peer);
                assert_eq!(query_id, Some(QueryId(1337)));
                assert!(std::matches!(
                    result,
                    QueryResult::ReadFailure {
                        reason: FailureReason::SubstreamClosed
                    }
                ));
            }
            result => panic!("invalid result received: {result:?}"),
        }
    }

    #[tokio::test]
    async fn send_fails_no_message_read() {
        let mut executor = QueryExecutor::new();
        let peer = PeerId::random();

        // prepare substream which succeeds in sending the message but closes right after
        let mut substream = MockSubstream::new();
        substream
            .expect_poll_ready()
            .times(1)
            .return_once(|_| Poll::Ready(Err(crate::error::SubstreamError::ConnectionClosed)));
        substream.expect_poll_close().times(1).return_once(|_| Poll::Ready(Ok(())));

        executor.send_request_read_response(
            peer,
            Some(QueryId(1337)),
            Bytes::from_static(b"hello, world"),
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream)),
        );

        match tokio::time::timeout(Duration::from_secs(20), executor.next()).await {
            Ok(Some(QueryContext {
                peer: queried_peer,
                query_id,
                result,
            })) => {
                assert_eq!(peer, queried_peer);
                assert_eq!(query_id, Some(QueryId(1337)));
                assert!(std::matches!(
                    result,
                    QueryResult::SendFailure {
                        reason: FailureReason::SubstreamClosed
                    }
                ));
            }
            result => panic!("invalid result received: {result:?}"),
        }
    }

    #[tokio::test]
    async fn read_message_timeout() {
        let mut executor = QueryExecutor::new();
        let peer = PeerId::random();

        // prepare substream which succeeds in sending the message but closes right after
        let mut substream = MockSubstream::new();
        substream.expect_poll_next().returning(|_| Poll::Pending);

        executor.read_message(
            peer,
            Some(QueryId(1336)),
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream)),
        );

        match tokio::time::timeout(Duration::from_secs(20), executor.next()).await {
            Ok(Some(QueryContext {
                peer: queried_peer,
                query_id,
                result,
            })) => {
                assert_eq!(peer, queried_peer);
                assert_eq!(query_id, Some(QueryId(1336)));
                assert!(std::matches!(
                    result,
                    QueryResult::ReadFailure {
                        reason: FailureReason::Timeout
                    }
                ));
            }
            result => panic!("invalid result received: {result:?}"),
        }
    }

    #[tokio::test]
    async fn read_message_substream_closed() {
        let mut executor = QueryExecutor::new();
        let peer = PeerId::random();

        // prepare substream which succeeds in sending the message but closes right after
        let mut substream = MockSubstream::new();
        substream
            .expect_poll_next()
            .times(1)
            .return_once(|_| Poll::Ready(Some(Err(crate::error::SubstreamError::ChannelClogged))));

        executor.read_message(
            peer,
            Some(QueryId(1335)),
            Substream::new_mock(peer, SubstreamId::from(0usize), Box::new(substream)),
        );

        match tokio::time::timeout(Duration::from_secs(20), executor.next()).await {
            Ok(Some(QueryContext {
                peer: queried_peer,
                query_id,
                result,
            })) => {
                assert_eq!(peer, queried_peer);
                assert_eq!(query_id, Some(QueryId(1335)));
                assert!(std::matches!(
                    result,
                    QueryResult::ReadFailure {
                        reason: FailureReason::SubstreamClosed
                    }
                ));
            }
            result => panic!("invalid result received: {result:?}"),
        }
    }
}
