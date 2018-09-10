// Copyright 2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate Demo.

// Substrate Demo is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate Demo is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate Demo.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate Demo-specific network implementation.
//!
//! This manages gossip of consensus messages for BFT.

#![warn(unused_extern_crates)]

extern crate substrate_bft as bft;
extern crate substrate_network;

extern crate demo_api;
extern crate demo_consensus;
extern crate demo_primitives;

extern crate ed25519;
extern crate futures;
extern crate tokio;
extern crate rhododendron;

#[macro_use]
extern crate log;

pub mod consensus;

use demo_primitives::{Block, Hash, Header};
use substrate_network::{NodeIndex, Context, Severity};
use substrate_network::consensus_gossip::ConsensusGossip;
use substrate_network::{message, generic_message};
use substrate_network::specialization::Specialization;
use substrate_network::StatusMessage as GenericFullStatus;

/// Demo protocol id.
pub const PROTOCOL_ID: ::substrate_network::ProtocolId = *b"dot";

type FullStatus = GenericFullStatus<Block>;

/// Specialization of the network service for the demo protocol.
pub type NetworkService = ::substrate_network::Service<Block, Protocol, Hash>;


/// Demo protocol attachment for substrate.
pub struct Protocol {
	consensus_gossip: ConsensusGossip<Block>,
	live_consensus: Option<Hash>,
}

impl Protocol {
	/// Instantiate a demo protocol handler.
	pub fn new() -> Self {
		Protocol {
			consensus_gossip: ConsensusGossip::new(),
			live_consensus: None,
		}
	}

	/// Note new consensus session.
	fn new_consensus(&mut self, parent_hash: Hash) {
		let old_consensus = self.live_consensus.take();
		self.live_consensus = Some(parent_hash);
		self.consensus_gossip.collect_garbage(old_consensus.as_ref());
	}
}

impl Specialization<Block> for Protocol {
	fn status(&self) -> Vec<u8> {
		Vec::new()
	}

	fn on_connect(&mut self, ctx: &mut Context<Block>, who: NodeIndex, status: FullStatus) {
		self.consensus_gossip.new_peer(ctx, who, status.roles);
	}

	fn on_disconnect(&mut self, ctx: &mut Context<Block>, who: NodeIndex) {
		self.consensus_gossip.peer_disconnected(ctx, who);
	}

	fn on_message(&mut self, ctx: &mut Context<Block>, who: NodeIndex, message: message::Message<Block>) {
		match message {
			generic_message::Message::BftMessage(msg) => {
				trace!(target: "demo-network", "BFT message from {}: {:?}", who, msg);
				// TODO: check signature here? what if relevant block is unknown?
				self.consensus_gossip.on_bft_message(ctx, who, msg)
			}
			generic_message::Message::ChainSpecific(_) => {
				trace!(target: "demo-network", "Bad message from {}", who);
				ctx.report_peer(who, Severity::Bad("Invalid demo protocol message format"));
			}
			_ => {}
		}
	}

	fn on_abort(&mut self) {
		self.consensus_gossip.abort();
	}

	fn maintain_peers(&mut self, _ctx: &mut Context<Block>) {
	}

	fn on_block_imported(&mut self, _ctx: &mut Context<Block>, _hash: Hash, _header: &Header) {
	}
}

