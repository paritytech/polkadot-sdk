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

use sp_runtime::traits::Block as BlockT;

use polkadot_node_primitives::PoV;
use polkadot_node_subsystem::messages::AvailabilityRecoveryMessage;

use futures::{channel::oneshot, stream::FuturesUnordered, Future, FutureExt, StreamExt};

use std::{collections::HashSet, pin::Pin, sync::Arc};

use crate::RecoveryHandle;

/// The active candidate recovery.
///
/// This handles the candidate recovery and tracks the activate recoveries.
pub(crate) struct ActiveCandidateRecovery<Block: BlockT> {
	/// The recoveries that are currently being executed.
	recoveries:
		FuturesUnordered<Pin<Box<dyn Future<Output = (Block::Hash, Option<Arc<PoV>>)> + Send>>>,
	/// The block hashes of the candidates currently being recovered.
	candidates: HashSet<Block::Hash>,
	recovery_handle: Box<dyn RecoveryHandle>,
}

impl<Block: BlockT> ActiveCandidateRecovery<Block> {
	pub fn new(recovery_handle: Box<dyn RecoveryHandle>) -> Self {
		Self { recoveries: Default::default(), candidates: Default::default(), recovery_handle }
	}

	/// Recover the given `candidate`.
	pub async fn recover_candidate(
		&mut self,
		block_hash: Block::Hash,
		candidate: &crate::Candidate<Block>,
	) {
		let (tx, rx) = oneshot::channel();

		self.recovery_handle
			.send_recovery_msg(
				AvailabilityRecoveryMessage::RecoverAvailableData(
					candidate.receipt.clone(),
					candidate.session_index,
					None,
					tx,
				),
				"ActiveCandidateRecovery",
			)
			.await;

		self.candidates.insert(block_hash);

		self.recoveries.push(
			async move {
				match rx.await {
					Ok(Ok(res)) => (block_hash, Some(res.pov)),
					Ok(Err(error)) => {
						tracing::debug!(
							target: crate::LOG_TARGET,
							?error,
							?block_hash,
							"Availability recovery failed",
						);
						(block_hash, None)
					},
					Err(_) => {
						tracing::debug!(
							target: crate::LOG_TARGET,
							"Availability recovery oneshot channel closed",
						);
						(block_hash, None)
					},
				}
			}
			.boxed(),
		);
	}

	/// Waits for the next recovery.
	///
	/// If the returned [`PoV`] is `None`, it means that the recovery failed.
	pub async fn wait_for_recovery(&mut self) -> (Block::Hash, Option<Arc<PoV>>) {
		loop {
			if let Some(res) = self.recoveries.next().await {
				self.candidates.remove(&res.0);
				return res
			} else {
				futures::pending!()
			}
		}
	}
}
