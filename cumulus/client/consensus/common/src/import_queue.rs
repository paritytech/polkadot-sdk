// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! (unstable) Composable utilities for constructing import queues for parachains.
//!
//! Unlike standalone chains, parachains have the requirement that all consensus logic
//! must be checked within the runtime. This property means that work which is normally
//! done in the import queue per-block, such as checking signatures, quorums, and whether
//! inherent extrinsics were constructed faithfully do not need to be done, per se.
//!
//! It may seem that it would be beneficial for the client to do these checks regardless,
//! but in practice this means that clients would just reject blocks which are _valid_ according
//! to their Parachain Validation Function, which is the ultimate source of consensus truth.
//!
//! However, parachain runtimes expose two different access points for executing blocks
//! in full nodes versus executing those blocks in the parachain validation environment.
//! At the time of writing, the inherent and consensus checks in most Cumulus runtimes
//! are only performed during parachain validation, not full node block execution.
//!
//! See <https://github.com/paritytech/cumulus/issues/2436> for details.

use sp_consensus::error::Error as ConsensusError;
use sp_runtime::traits::Block as BlockT;

use sc_consensus::{
	block_import::{BlockImport, BlockImportParams},
	import_queue::{BasicQueue, Verifier},
};

use crate::ParachainBlockImportMarker;

/// A [`Verifier`] for blocks which verifies absolutely nothing.
///
/// This should only be used when the runtime is responsible for checking block seals and inherents.
pub struct VerifyNothing;

#[async_trait::async_trait]
impl<Block: BlockT> Verifier<Block> for VerifyNothing {
	async fn verify(
		&mut self,
		params: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		Ok(params)
	}
}

/// An import queue which does no verification.
///
/// This should only be used when the runtime is responsible for checking block seals and inherents.
pub fn verify_nothing_import_queue<Block: BlockT, I>(
	block_import: I,
	spawner: &impl sp_core::traits::SpawnEssentialNamed,
	registry: Option<&substrate_prometheus_endpoint::Registry>,
) -> BasicQueue<Block>
where
	I: BlockImport<Block, Error = ConsensusError>
		+ ParachainBlockImportMarker
		+ Send
		+ Sync
		+ 'static,
{
	BasicQueue::new(VerifyNothing, Box::new(block_import), None, spawner, registry)
}
