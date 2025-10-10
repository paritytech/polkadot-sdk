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

//! A generic prospective parachains subsystem mockup suitable to be used in benchmarks.

use futures::FutureExt;
use polkadot_node_subsystem::{
	messages::ProspectiveParachainsMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::Hash;
use std::collections::HashMap;

use crate::availability::TestState;

pub struct MockProspectiveParachains {
	// Map relay parent -> candidate hashes (in order) used by the bench
	per_relay_parent_candidates: HashMap<Hash, Vec<polkadot_primitives::CandidateHash>>,
	state: Option<TestState>,
}

impl MockProspectiveParachains {
	pub fn new() -> Self {
		Self { per_relay_parent_candidates: HashMap::new(), state: None }
	}

	// Initialize from TestState so we can answer GetBackableCandidates
	pub fn with_test_state(mut self, state: &TestState) -> Self {
		for (relay_hash, receipts) in state.candidate_receipts.iter() {
			let hashes = receipts
				.iter()
				.map(|r| -> polkadot_primitives::CandidateHash { r.hash() })
				.collect::<Vec<_>>();
			self.per_relay_parent_candidates.insert(*relay_hash, hashes);
		}
		self.state = Some(state.clone());
		self
	}
}

#[overseer::subsystem(ProspectiveParachains, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockProspectiveParachains {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
impl MockProspectiveParachains {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");
			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					ProspectiveParachainsMessage::GetMinimumRelayParents(_relay_parent, tx) => {
						tx.send(vec![]).unwrap();
					},
					ProspectiveParachainsMessage::GetBackableCandidates(
						relay_parent,
						para_id,
						count,
						_ancestors,
						tx,
					) => {
						// Return up to `count` candidate hashes for the NEXT block's candidates of
						// this para.
						let mut response = Vec::new();
						if let Some(state) = &self.state {
							// find index of current relay_parent in block_infos
							if let Some((idx, _)) = state
								.block_infos
								.iter()
								.enumerate()
								.find(|(_, bi)| bi.hash == relay_parent)
							{
								let next_idx = (idx + 1) % state.block_infos.len();
								let next_hash = state.block_infos[next_idx].hash;
								if let Some(receipts) = state.candidate_receipts.get(&next_hash) {
									for r in receipts.iter() {
										if r.descriptor.para_id() == para_id &&
											response.len() < count as usize
										{
											response.push((r.hash(), relay_parent));
										}
									}
								}
							}
						}
						let _ = tx.send(response);
					},
					ProspectiveParachainsMessage::GetHypotheticalMembership(req, tx) => {
						tx.send(
							req.candidates
								.iter()
								.cloned()
								.map(|candidate| (candidate, vec![Hash::repeat_byte(0)]))
								.collect(),
						)
						.unwrap();
					},
					_ => {
						unimplemented!("Unexpected chain-api message")
					},
				},
			}
		}
	}
}
