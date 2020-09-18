// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Message delivery loop. Designed to work with message-lane pallet.
//!
//! Single relay instance delivers messages of single lane in single direction.
//! To serve two-way lane, you would need two instances of relay.
//! To serve N two-way lanes, you would need N*2 instances of relay.
//!
//! Please keep in mind that the best header in this file is actually best
//! finalized header. I.e. when talking about headers in lane context, we
//! only care about finalized headers.

// Until there'll be actual message-lane in the runtime.
#![allow(dead_code)]

use crate::message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf};
use crate::message_race_delivery::run as run_message_delivery_race;
use crate::message_race_receiving::run as run_message_receiving_race;

use async_trait::async_trait;
use futures::{channel::mpsc::unbounded, future::FutureExt, stream::StreamExt};
use relay_utils::{interval, process_future_result, retry_backoff, FailedClient, MaybeConnectionError};
use std::{fmt::Debug, future::Future, ops::RangeInclusive, time::Duration};

/// Source client trait.
#[async_trait(?Send)]
pub trait SourceClient<P: MessageLane>: Clone {
	/// Type of error this clients returns.
	type Error: std::fmt::Debug + MaybeConnectionError;

	/// Try to reconnect to source node.
	fn reconnect(self) -> Self;

	/// Returns state of the client.
	async fn state(&self) -> Result<SourceClientState<P>, Self::Error>;

	/// Get nonce of instance of latest generated message.
	async fn latest_generated_nonce(
		&self,
		id: SourceHeaderIdOf<P>,
	) -> Result<(SourceHeaderIdOf<P>, P::MessageNonce), Self::Error>;
	/// Get nonce of the latest message, which receiving has been confirmed by the target chain.
	async fn latest_confirmed_received_nonce(
		&self,
		id: SourceHeaderIdOf<P>,
	) -> Result<(SourceHeaderIdOf<P>, P::MessageNonce), Self::Error>;

	/// Prove messages in inclusive range [begin; end].
	async fn prove_messages(
		&self,
		id: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
	) -> Result<(SourceHeaderIdOf<P>, RangeInclusive<P::MessageNonce>, P::MessagesProof), Self::Error>;

	/// Submit messages receiving proof.
	async fn submit_messages_receiving_proof(
		&self,
		generated_at_block: TargetHeaderIdOf<P>,
		proof: P::MessagesReceivingProof,
	) -> Result<RangeInclusive<P::MessageNonce>, Self::Error>;
}

/// Target client trait.
#[async_trait(?Send)]
pub trait TargetClient<P: MessageLane>: Clone {
	/// Type of error this clients returns.
	type Error: std::fmt::Debug + MaybeConnectionError;

	/// Try to reconnect to source node.
	fn reconnect(self) -> Self;

	/// Returns state of the client.
	async fn state(&self) -> Result<TargetClientState<P>, Self::Error>;

	/// Get nonce of latest received message.
	async fn latest_received_nonce(
		&self,
		id: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, P::MessageNonce), Self::Error>;

	/// Prove messages receiving at given block.
	async fn prove_messages_receiving(
		&self,
		id: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, P::MessagesReceivingProof), Self::Error>;

	/// Submit messages proof.
	async fn submit_messages_proof(
		&self,
		generated_at_header: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		proof: P::MessagesProof,
	) -> Result<RangeInclusive<P::MessageNonce>, Self::Error>;
}

/// State of the client.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientState<SelfHeaderId, PeerHeaderId> {
	/// Best header id of this chain.
	pub best_self: SelfHeaderId,
	/// Best header id of the peer chain.
	pub best_peer: PeerHeaderId,
}

/// State of source client in one-way message lane.
pub type SourceClientState<P> = ClientState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>>;

/// State of target client in one-way message lane.
pub type TargetClientState<P> = ClientState<TargetHeaderIdOf<P>, SourceHeaderIdOf<P>>;

/// Both clients state.
#[derive(Debug, Default)]
pub struct ClientsState<P: MessageLane> {
	/// Source client state.
	pub source: Option<SourceClientState<P>>,
	/// Target client state.
	pub target: Option<TargetClientState<P>>,
}

/// Run message lane service loop.
pub fn run<P: MessageLane>(
	mut source_client: impl SourceClient<P>,
	source_tick: Duration,
	mut target_client: impl TargetClient<P>,
	target_tick: Duration,
	reconnect_delay: Duration,
	stall_timeout: Duration,
	exit_signal: impl Future<Output = ()>,
) {
	let mut local_pool = futures::executor::LocalPool::new();
	let exit_signal = exit_signal.shared();

	local_pool.run_until(async move {
		loop {
			let result = run_until_connection_lost(
				source_client.clone(),
				source_tick,
				target_client.clone(),
				target_tick,
				stall_timeout,
				exit_signal.clone(),
			)
			.await;

			match result {
				Ok(()) => break,
				Err(failed_client) => {
					async_std::task::sleep(reconnect_delay).await;
					if failed_client == FailedClient::Both || failed_client == FailedClient::Source {
						source_client = source_client.reconnect();
					}
					if failed_client == FailedClient::Both || failed_client == FailedClient::Target {
						target_client = target_client.reconnect();
					}
				}
			}

			log::debug!(
				target: "bridge",
				"Restarting lane {} -> {}",
				P::SOURCE_NAME,
				P::TARGET_NAME,
			);
		}
	});
}

/// Run one-way message delivery loop until connection with target or source node is lost, or exit signal is received.
async fn run_until_connection_lost<P: MessageLane, SC: SourceClient<P>, TC: TargetClient<P>>(
	source_client: SC,
	source_tick: Duration,
	target_client: TC,
	target_tick: Duration,
	stall_timeout: Duration,
	exit_signal: impl Future<Output = ()>,
) -> Result<(), FailedClient> {
	let mut source_retry_backoff = retry_backoff();
	let mut source_client_is_online = false;
	let mut source_state_required = true;
	let source_state = source_client.state().fuse();
	let source_go_offline_future = futures::future::Fuse::terminated();
	let source_tick_stream = interval(source_tick).fuse();

	let mut target_retry_backoff = retry_backoff();
	let mut target_client_is_online = false;
	let mut target_state_required = true;
	let target_state = target_client.state().fuse();
	let target_go_offline_future = futures::future::Fuse::terminated();
	let target_tick_stream = interval(target_tick).fuse();

	let (
		(delivery_source_state_sender, delivery_source_state_receiver),
		(delivery_target_state_sender, delivery_target_state_receiver),
	) = (unbounded(), unbounded());
	let delivery_race_loop = run_message_delivery_race(
		source_client.clone(),
		delivery_source_state_receiver,
		target_client.clone(),
		delivery_target_state_receiver,
		stall_timeout,
	)
	.fuse();

	let (
		(receiving_source_state_sender, receiving_source_state_receiver),
		(receiving_target_state_sender, receiving_target_state_receiver),
	) = (unbounded(), unbounded());
	let receiving_race_loop = run_message_receiving_race(
		source_client.clone(),
		receiving_source_state_receiver,
		target_client.clone(),
		receiving_target_state_receiver,
		stall_timeout,
	)
	.fuse();

	let exit_signal = exit_signal.fuse();

	futures::pin_mut!(
		source_state,
		source_go_offline_future,
		source_tick_stream,
		target_state,
		target_go_offline_future,
		target_tick_stream,
		delivery_race_loop,
		receiving_race_loop,
		exit_signal
	);

	loop {
		futures::select! {
			new_source_state = source_state => {
				source_state_required = false;

				source_client_is_online = process_future_result(
					new_source_state,
					&mut source_retry_backoff,
					|new_source_state| {
						log::debug!(
							target: "bridge",
							"Received state from {} node: {:?}",
							P::SOURCE_NAME,
							new_source_state,
						);
						let _ = delivery_source_state_sender.unbounded_send(new_source_state.clone());
						let _ = receiving_source_state_sender.unbounded_send(new_source_state.clone());
					},
					&mut source_go_offline_future,
					|delay| async_std::task::sleep(delay),
					|| format!("Error retrieving state from {} node", P::SOURCE_NAME),
				).fail_if_connection_error(FailedClient::Source)?;
			},
			_ = source_go_offline_future => {
				source_client_is_online = true;
			},
			_ = source_tick_stream.next() => {
				source_state_required = true;
			},
			new_target_state = target_state => {
				target_state_required = false;

				target_client_is_online = process_future_result(
					new_target_state,
					&mut target_retry_backoff,
					|new_target_state| {
						log::debug!(
							target: "bridge",
							"Received state from {} node: {:?}",
							P::TARGET_NAME,
							new_target_state,
						);
						let _ = delivery_target_state_sender.unbounded_send(new_target_state.clone());
						let _ = receiving_target_state_sender.unbounded_send(new_target_state.clone());
					},
					&mut target_go_offline_future,
					|delay| async_std::task::sleep(delay),
					|| format!("Error retrieving state from {} node", P::TARGET_NAME),
				).fail_if_connection_error(FailedClient::Target)?;
			},
			_ = target_go_offline_future => {
				target_client_is_online = true;
			},
			_ = target_tick_stream.next() => {
				target_state_required = true;
			},

			delivery_error = delivery_race_loop => {
				match delivery_error {
					Ok(_) => unreachable!("only ends with error; qed"),
					Err(err) => return Err(err),
				}
			},
			receiving_error = receiving_race_loop => {
				match receiving_error {
					Ok(_) => unreachable!("only ends with error; qed"),
					Err(err) => return Err(err),
				}
			},

			() = exit_signal => {
				return Ok(());
			}
		}

		if source_client_is_online && source_state_required {
			log::debug!(target: "bridge", "Asking {} node about its state", P::SOURCE_NAME);
			source_state.set(source_client.state().fuse());
			source_client_is_online = false;
		}

		if target_client_is_online && target_state_required {
			log::debug!(target: "bridge", "Asking {} node about its state", P::TARGET_NAME);
			target_state.set(target_client.state().fuse());
			target_client_is_online = false;
		}
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use futures::stream::StreamExt;
	use parking_lot::Mutex;
	use relay_utils::HeaderId;
	use std::sync::Arc;

	pub fn header_id(number: TestSourceHeaderNumber) -> HeaderId<TestSourceHeaderNumber, TestSourceHeaderHash> {
		HeaderId(number, number)
	}

	pub type TestMessageNonce = u64;
	pub type TestMessagesProof = RangeInclusive<TestMessageNonce>;
	pub type TestMessagesReceivingProof = TestMessageNonce;

	pub type TestSourceHeaderNumber = u64;
	pub type TestSourceHeaderHash = u64;

	pub type TestTargetHeaderNumber = u64;
	pub type TestTargetHeaderHash = u64;

	#[derive(Debug)]
	pub enum TestError {
		Logic,
		Connection,
	}

	impl MaybeConnectionError for TestError {
		fn is_connection_error(&self) -> bool {
			match *self {
				TestError::Logic => false,
				TestError::Connection => true,
			}
		}
	}

	pub struct TestMessageLane;

	impl MessageLane for TestMessageLane {
		const SOURCE_NAME: &'static str = "TestSource";
		const TARGET_NAME: &'static str = "TestTarget";

		type MessageNonce = TestMessageNonce;

		type MessagesProof = TestMessagesProof;
		type MessagesReceivingProof = TestMessagesReceivingProof;

		type SourceHeaderNumber = TestSourceHeaderNumber;
		type SourceHeaderHash = TestSourceHeaderHash;

		type TargetHeaderNumber = TestTargetHeaderNumber;
		type TargetHeaderHash = TestTargetHeaderHash;
	}

	#[derive(Debug, Default, Clone)]
	pub struct TestClientData {
		is_source_fails: bool,
		is_source_reconnected: bool,
		source_state: SourceClientState<TestMessageLane>,
		source_latest_generated_nonce: TestMessageNonce,
		source_latest_confirmed_received_nonce: TestMessageNonce,
		submitted_messages_receiving_proofs: Vec<TestMessagesReceivingProof>,
		is_target_fails: bool,
		is_target_reconnected: bool,
		target_state: SourceClientState<TestMessageLane>,
		target_latest_received_nonce: TestMessageNonce,
		submitted_messages_proofs: Vec<TestMessagesProof>,
	}

	#[derive(Clone)]
	pub struct TestSourceClient {
		data: Arc<Mutex<TestClientData>>,
		tick: Arc<dyn Fn(&mut TestClientData)>,
	}

	#[async_trait(?Send)]
	impl SourceClient<TestMessageLane> for TestSourceClient {
		type Error = TestError;

		fn reconnect(self) -> Self {
			{
				let mut data = self.data.lock();
				(self.tick)(&mut *data);
				data.is_source_reconnected = true;
			}
			self
		}

		async fn state(&self) -> Result<SourceClientState<TestMessageLane>, Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			if data.is_source_fails {
				return Err(TestError::Connection);
			}
			Ok(data.source_state.clone())
		}

		async fn latest_generated_nonce(
			&self,
			id: SourceHeaderIdOf<TestMessageLane>,
		) -> Result<(SourceHeaderIdOf<TestMessageLane>, TestMessageNonce), Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			if data.is_source_fails {
				return Err(TestError::Connection);
			}
			Ok((id, data.source_latest_generated_nonce))
		}

		async fn latest_confirmed_received_nonce(
			&self,
			id: SourceHeaderIdOf<TestMessageLane>,
		) -> Result<(SourceHeaderIdOf<TestMessageLane>, TestMessageNonce), Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			Ok((id, data.source_latest_confirmed_received_nonce))
		}

		async fn prove_messages(
			&self,
			id: SourceHeaderIdOf<TestMessageLane>,
			nonces: RangeInclusive<TestMessageNonce>,
		) -> Result<
			(
				SourceHeaderIdOf<TestMessageLane>,
				RangeInclusive<TestMessageNonce>,
				TestMessagesProof,
			),
			Self::Error,
		> {
			Ok((id, nonces.clone(), nonces))
		}

		async fn submit_messages_receiving_proof(
			&self,
			_generated_at_block: TargetHeaderIdOf<TestMessageLane>,
			proof: TestMessagesReceivingProof,
		) -> Result<RangeInclusive<TestMessageNonce>, Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			data.submitted_messages_receiving_proofs.push(proof);
			data.source_latest_confirmed_received_nonce = proof;
			Ok(proof..=proof)
		}
	}

	#[derive(Clone)]
	pub struct TestTargetClient {
		data: Arc<Mutex<TestClientData>>,
		tick: Arc<dyn Fn(&mut TestClientData)>,
	}

	#[async_trait(?Send)]
	impl TargetClient<TestMessageLane> for TestTargetClient {
		type Error = TestError;

		fn reconnect(self) -> Self {
			{
				let mut data = self.data.lock();
				(self.tick)(&mut *data);
				data.is_target_reconnected = true;
			}
			self
		}

		async fn state(&self) -> Result<TargetClientState<TestMessageLane>, Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			if data.is_target_fails {
				return Err(TestError::Connection);
			}
			Ok(data.target_state.clone())
		}

		async fn latest_received_nonce(
			&self,
			id: TargetHeaderIdOf<TestMessageLane>,
		) -> Result<(TargetHeaderIdOf<TestMessageLane>, TestMessageNonce), Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			if data.is_target_fails {
				return Err(TestError::Connection);
			}
			Ok((id, data.target_latest_received_nonce))
		}

		async fn prove_messages_receiving(
			&self,
			id: TargetHeaderIdOf<TestMessageLane>,
		) -> Result<(TargetHeaderIdOf<TestMessageLane>, TestMessagesReceivingProof), Self::Error> {
			Ok((id, self.data.lock().target_latest_received_nonce))
		}

		async fn submit_messages_proof(
			&self,
			_generated_at_header: SourceHeaderIdOf<TestMessageLane>,
			nonces: RangeInclusive<TestMessageNonce>,
			proof: TestMessagesProof,
		) -> Result<RangeInclusive<TestMessageNonce>, Self::Error> {
			let mut data = self.data.lock();
			(self.tick)(&mut *data);
			if data.is_target_fails {
				return Err(TestError::Connection);
			}
			data.target_state.best_self =
				HeaderId(data.target_state.best_self.0 + 1, data.target_state.best_self.1 + 1);
			data.target_latest_received_nonce = *proof.end();
			data.submitted_messages_proofs.push(proof);
			Ok(nonces)
		}
	}

	fn run_loop_test(
		data: TestClientData,
		source_tick: Arc<dyn Fn(&mut TestClientData)>,
		target_tick: Arc<dyn Fn(&mut TestClientData)>,
		exit_signal: impl Future<Output = ()>,
	) -> TestClientData {
		async_std::task::block_on(async {
			let data = Arc::new(Mutex::new(data));

			let source_client = TestSourceClient {
				data: data.clone(),
				tick: source_tick,
			};
			let target_client = TestTargetClient {
				data: data.clone(),
				tick: target_tick,
			};
			run(
				source_client,
				Duration::from_millis(100),
				target_client,
				Duration::from_millis(100),
				Duration::from_millis(0),
				Duration::from_secs(60),
				exit_signal,
			);

			let result = data.lock().clone();
			result
		})
	}

	#[test]
	fn message_lane_loop_is_able_to_recover_from_connection_errors() {
		// with this configuration, source client will return Err, making source client
		// reconnect. Then the target client will fail with Err + reconnect. Then we finally
		// able to deliver messages.
		let (exit_sender, exit_receiver) = unbounded();
		let result = run_loop_test(
			TestClientData {
				is_source_fails: true,
				source_state: ClientState {
					best_self: HeaderId(0, 0),
					best_peer: HeaderId(0, 0),
				},
				source_latest_generated_nonce: 1,
				target_state: ClientState {
					best_self: HeaderId(0, 0),
					best_peer: HeaderId(0, 0),
				},
				target_latest_received_nonce: 0,
				..Default::default()
			},
			Arc::new(|data: &mut TestClientData| {
				if data.is_source_reconnected {
					data.is_source_fails = false;
					data.is_target_fails = true;
				}
			}),
			Arc::new(move |data: &mut TestClientData| {
				if data.is_target_reconnected {
					data.is_target_fails = false;
				}
				if data.target_state.best_peer.0 < 10 {
					data.target_state.best_peer =
						HeaderId(data.target_state.best_peer.0 + 1, data.target_state.best_peer.0 + 1);
				}
				if !data.submitted_messages_proofs.is_empty() {
					exit_sender.unbounded_send(()).unwrap();
				}
			}),
			exit_receiver.into_future().map(|(_, _)| ()),
		);

		assert_eq!(result.submitted_messages_proofs, vec![1..=1],);
	}

	#[test]
	fn message_lane_loop_works() {
		// with this configuration, target client must first sync headers [1; 10] and
		// then submit proof-of-messages [0; 10] at once
		let (exit_sender, exit_receiver) = unbounded();
		let result = run_loop_test(
			TestClientData {
				source_state: ClientState {
					best_self: HeaderId(10, 10),
					best_peer: HeaderId(0, 0),
				},
				source_latest_generated_nonce: 10,
				target_state: ClientState {
					best_self: HeaderId(0, 0),
					best_peer: HeaderId(0, 0),
				},
				target_latest_received_nonce: 0,
				..Default::default()
			},
			Arc::new(|_: &mut TestClientData| {}),
			Arc::new(move |data: &mut TestClientData| {
				// syncing source headers -> target chain (by one)
				if data.target_state.best_peer.0 < data.source_state.best_self.0 {
					data.target_state.best_peer =
						HeaderId(data.target_state.best_peer.0 + 1, data.target_state.best_peer.0 + 1);
				}
				// syncing source headers -> target chain (all at once)
				if data.source_state.best_peer.0 < data.target_state.best_self.0 {
					data.source_state.best_peer = data.target_state.best_self;
				}
				// if target has received all messages => increase target block so that confirmations may be sent
				if data.target_latest_received_nonce == 10 {
					data.target_state.best_self =
						HeaderId(data.source_state.best_self.0 + 1, data.source_state.best_self.0 + 1);
				}
				// if source has received all messages receiving confirmations => increase source block so that confirmations may be sent
				if data.source_latest_confirmed_received_nonce == 10 {
					exit_sender.unbounded_send(()).unwrap();
				}
			}),
			exit_receiver.into_future().map(|(_, _)| ()),
		);

		assert_eq!(result.submitted_messages_proofs, vec![1..=4, 5..=8, 9..=10],);
		assert!(!result.submitted_messages_receiving_proofs.is_empty());
	}
}
