// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use crate::{Network, Validator};
use crate::state_machine::{ConsensusGossip, TopicNotification, PERIODIC_MAINTENANCE_INTERVAL};

use sc_network::{Event, ReputationChange};

use futures::prelude::*;
use libp2p::PeerId;
use sp_runtime::{traits::Block as BlockT, ConsensusEngineId};
use std::{borrow::Cow, pin::Pin, sync::Arc, task::{Context, Poll}};
use sp_utils::mpsc::TracingUnboundedReceiver;

/// Wraps around an implementation of the `Network` crate and provides gossiping capabilities on
/// top of it.
pub struct GossipEngine<B: BlockT> {
	state_machine: ConsensusGossip<B>,
	network: Box<dyn Network<B> + Send>,
	periodic_maintenance_interval: futures_timer::Delay,
	network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
	engine_id: ConsensusEngineId,
}

impl<B: BlockT> Unpin for GossipEngine<B> {}

impl<B: BlockT> GossipEngine<B> {
	/// Create a new instance.
	pub fn new<N: Network<B> + Send + Clone + 'static>(
		mut network: N,
		engine_id: ConsensusEngineId,
		protocol_name: impl Into<Cow<'static, [u8]>>,
		validator: Arc<dyn Validator<B>>,
	) -> Self where B: 'static {
		let mut state_machine = ConsensusGossip::new();

		// We grab the event stream before registering the notifications protocol, otherwise we
		// might miss events.
		let network_event_stream = network.event_stream();

		network.register_notifications_protocol(engine_id, protocol_name.into());
		state_machine.register_validator(&mut network, engine_id, validator);

		GossipEngine {
			state_machine,
			network: Box::new(network),
			periodic_maintenance_interval: futures_timer::Delay::new(PERIODIC_MAINTENANCE_INTERVAL),
			network_event_stream,
			engine_id,
		}
	}

	pub fn report(&self, who: PeerId, reputation: ReputationChange) {
		self.network.report_peer(who, reputation);
	}

	/// Registers a message without propagating it to any peers. The message
	/// becomes available to new peers or when the service is asked to gossip
	/// the message's topic. No validation is performed on the message, if the
	/// message is already expired it should be dropped on the next garbage
	/// collection.
	pub fn register_gossip_message(
		&mut self,
		topic: B::Hash,
		message: Vec<u8>,
	) {
		self.state_machine.register_message(topic, self.engine_id, message);
	}

	/// Broadcast all messages with given topic.
	pub fn broadcast_topic(&mut self, topic: B::Hash, force: bool) {
		self.state_machine.broadcast_topic(&mut *self.network, topic, force);
	}

	/// Get data of valid, incoming messages for a topic (but might have expired meanwhile).
	pub fn messages_for(&mut self, topic: B::Hash)
		-> TracingUnboundedReceiver<TopicNotification>
	{
		self.state_machine.messages_for(self.engine_id, topic)
	}

	/// Send all messages with given topic to a peer.
	pub fn send_topic(
		&mut self,
		who: &PeerId,
		topic: B::Hash,
		force: bool
	) {
		self.state_machine.send_topic(&mut *self.network, who, topic, self.engine_id, force)
	}

	/// Multicast a message to all peers.
	pub fn gossip_message(
		&mut self,
		topic: B::Hash,
		message: Vec<u8>,
		force: bool,
	) {
		self.state_machine.multicast(&mut *self.network, topic, self.engine_id, message, force)
	}

	/// Send addressed message to the given peers. The message is not kept or multicast
	/// later on.
	pub fn send_message(&mut self, who: Vec<sc_network::PeerId>, data: Vec<u8>) {
		for who in &who {
			self.state_machine.send_message(&mut *self.network, who, self.engine_id, data.clone());
		}
	}

	/// Notify everyone we're connected to that we have the given block.
	///
	/// Note: this method isn't strictly related to gossiping and should eventually be moved
	/// somewhere else.
	pub fn announce(&self, block: B::Hash, associated_data: Vec<u8>) {
		self.network.announce(block, associated_data);
	}
}

impl<B: BlockT> Future for GossipEngine<B> {
	type Output = ();

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		let this = &mut *self;

		loop {
			match this.network_event_stream.poll_next_unpin(cx) {
				Poll::Ready(Some(event)) => match event {
					Event::NotificationStreamOpened { remote, engine_id: msg_engine_id, role } => {
						if msg_engine_id != this.engine_id {
							continue;
						}
						this.state_machine.new_peer(&mut *this.network, remote, role);
					}
					Event::NotificationStreamClosed { remote, engine_id: msg_engine_id } => {
						if msg_engine_id != this.engine_id {
							continue;
						}
						this.state_machine.peer_disconnected(&mut *this.network, remote);
					},
					Event::NotificationsReceived { remote, messages } => {
						let engine_id = this.engine_id.clone();
						this.state_machine.on_incoming(
							&mut *this.network,
							remote,
							messages.into_iter()
								.filter_map(|(engine, data)| if engine == engine_id {
									Some((engine, data.to_vec()))
								} else { None })
								.collect()
						);
					},
					Event::Dht(_) => {}
				}
				// The network event stream closed. Do the same for [`GossipValidator`].
				Poll::Ready(None) => return Poll::Ready(()),
				Poll::Pending => break,
			}
		}

		while let Poll::Ready(()) = this.periodic_maintenance_interval.poll_unpin(cx) {
			this.periodic_maintenance_interval.reset(PERIODIC_MAINTENANCE_INTERVAL);
			this.state_machine.tick(&mut *this.network);
		}

		Poll::Pending
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ValidationResult, ValidatorContext};
	use substrate_test_runtime_client::runtime::Block;

	struct TestNetwork {}

	impl<B: BlockT> Network<B> for Arc<TestNetwork> {
		fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
			let (_tx, rx) = futures::channel::mpsc::channel(0);

			// Return rx and drop tx. Thus the given channel will yield `Poll::Ready(None)` on first
			// poll.
			Box::pin(rx)
		}

		fn report_peer(&self, _: PeerId, _: ReputationChange) {
			unimplemented!();
		}

		fn disconnect_peer(&self, _: PeerId) {
			unimplemented!();
		}

		fn write_notification(&self, _: PeerId, _: ConsensusEngineId, _: Vec<u8>) {
			unimplemented!();
		}

		fn register_notifications_protocol(&self, _: ConsensusEngineId, _: Cow<'static, [u8]>) {}

		fn announce(&self, _: B::Hash, _: Vec<u8>) {
			unimplemented!();
		}
	}

	struct TestValidator {}

	impl<B: BlockT> Validator<B> for TestValidator {
		fn validate(
			&self,
			_: &mut dyn ValidatorContext<B>,
			_: &PeerId,
			_: &[u8]
		) -> ValidationResult<B::Hash> {
			unimplemented!();
		}
	}

	/// Regression test for the case where the `GossipEngine.network_event_stream` closes. One
	/// should not ignore a `Poll::Ready(None)` as `poll_next_unpin` will panic on subsequent calls.
	///
	/// See https://github.com/paritytech/substrate/issues/5000 for details.
	#[test]
	fn returns_when_network_event_stream_closes() {
		let mut gossip_engine = GossipEngine::<Block>::new(
			Arc::new(TestNetwork{}),
			[1, 2, 3, 4],
			"my_protocol".as_bytes(),
			Arc::new(TestValidator{}),
		);

		futures::executor::block_on(futures::future::poll_fn(move |ctx| {
			if let Poll::Pending = gossip_engine.poll_unpin(ctx) {
				panic!(
					"Expected gossip engine to finish on first poll, given that \
					 `GossipEngine.network_event_stream` closes right away."
				)
			}
			Poll::Ready(())
		}))
	}
}
