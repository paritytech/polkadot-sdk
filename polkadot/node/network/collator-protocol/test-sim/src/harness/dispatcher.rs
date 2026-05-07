// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Dispatcher: drains outgoing subsystem messages, routes them to either the [`Recorder`]
//! (effects) or the responder (queries) per the [`crate::contract::classify`] rule.
//!
//! The full responder DSL lands in `crate::responder`. This file defines the dispatch loop and
//! the [`AnswerQuery`] trait that the responder will implement.

use crate::{
	contract::{classify, Classified, Query},
	harness::Recorder,
};
use polkadot_node_subsystem::messages::AllMessages;
use std::time::Instant;

/// Trait implemented by anything that can answer subsystem queries (the responder DSL).
pub trait AnswerQuery {
	/// Answer the given query. Implementations write back to the embedded `oneshot::Sender`s.
	/// If a query is unexpected, implementations panic with a descriptive message — that is a
	/// test bug (the test forgot to script a particular query path).
	fn answer(&mut self, query: Query);
}

/// Drains a single outgoing `AllMessages`, classifies it, and routes:
/// - Effects → recorded into `recorder`.
/// - Queries → forwarded to `responder`.
///
/// `now` is the simulated wall-clock instant at which the dispatch happens (the recorder uses
/// it to derive the entry's `sim_t`).
pub struct Dispatcher<'a, R: AnswerQuery + ?Sized> {
	/// Where effects accumulate.
	pub recorder: &'a mut Recorder,
	/// Where queries are routed.
	pub responder: &'a mut R,
}

impl<'a, R: AnswerQuery + ?Sized> Dispatcher<'a, R> {
	/// Create a new dispatcher.
	pub fn new(recorder: &'a mut Recorder, responder: &'a mut R) -> Self {
		Self { recorder, responder }
	}

	/// Process a single outbound message. One inbound message can yield multiple classified
	/// entries (e.g. a batched `SendRequests` or `SendCollationMessages`); the dispatcher
	/// records / forwards them in order.
	pub fn dispatch(&mut self, now: Instant, msg: AllMessages) {
		for c in classify(msg) {
			match c {
				Classified::Effect(effect) => self.recorder.record_effect(now, effect),
				Classified::Query(query) => self.responder.answer(query),
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::contract::Effect;
	use polkadot_node_network_protocol::peer_set::PeerSet;
	use polkadot_node_subsystem::messages::NetworkBridgeTxMessage;

	struct PanicResponder;
	impl AnswerQuery for PanicResponder {
		fn answer(&mut self, query: Query) {
			panic!("unexpected query: {:?}", query);
		}
	}

	#[test]
	fn effect_message_records_into_recorder() {
		let mut rec = Recorder::new();
		let mut resp = PanicResponder;
		let mut disp = Dispatcher::new(&mut rec, &mut resp);
		let peer = sc_network_types::PeerId::random();
		let msg = AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::DisconnectPeers(
			vec![peer],
			PeerSet::Collation,
		));
		disp.dispatch(Instant::now(), msg);
		assert_eq!(rec.len(), 1);
		assert!(matches!(
			rec.effects().next().unwrap(),
			Effect::DisconnectPeers { peer_set: PeerSet::Collation, .. }
		));
	}

	struct CountingResponder {
		count: usize,
	}
	impl AnswerQuery for CountingResponder {
		fn answer(&mut self, _query: Query) {
			self.count += 1;
		}
	}

	#[test]
	fn query_message_forwards_to_responder() {
		use polkadot_node_subsystem::messages::ChainApiMessage;
		let mut rec = Recorder::new();
		let mut resp = CountingResponder { count: 0 };
		let mut disp = Dispatcher::new(&mut rec, &mut resp);
		let (tx, _rx) = futures::channel::oneshot::channel();
		let msg = AllMessages::ChainApi(ChainApiMessage::FinalizedBlockNumber(tx));
		disp.dispatch(Instant::now(), msg);
		assert_eq!(rec.len(), 0, "query is not recorded as effect");
		assert_eq!(resp.count, 1, "query is forwarded to responder");
	}
}
