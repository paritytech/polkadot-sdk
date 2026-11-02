// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The relay chain's view of each source's newest included `StreamsRoot`.

use crate::LOG_TARGET;
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::StreamsRoot;
use cumulus_relay_chain_interface::{call_runtime_api, PHash, RelayChainInterface};
use polkadot_primitives::runtime_api::ParachainHost;
use sp_api::RuntimeApiInfo;
use std::sync::Arc;

/// The relay runtime API giving a source's newest included `StreamsRoot`.
pub const NEWEST_INCLUDED_PROVIDES: &str = "ParachainHost_newest_included_provides";

/// The `ParachainHost` version that added [`NEWEST_INCLUDED_PROVIDES`].
pub const NEWEST_INCLUDED_PROVIDES_VERSION: u32 = 17;

/// `source`'s newest included root as of relay block `at`. `None` also when the relay runtime
/// predates the API.
pub async fn newest_included_provides(
	relay: &(impl RelayChainInterface + ?Sized),
	at: PHash,
	source: ParaId,
) -> Option<StreamsRoot> {
	call_runtime_api::<Option<StreamsRoot>>(relay, NEWEST_INCLUDED_PROVIDES, at, source)
		.await
		.map_err(|e| log::debug!(target: LOG_TARGET, "{NEWEST_INCLUDED_PROVIDES} failed: {e}"))
		.ok()
		.flatten()
}

/// Whether relay block `at` has [`NEWEST_INCLUDED_PROVIDES`].
pub async fn relay_has_api(relay: &(impl RelayChainInterface + ?Sized), at: PHash) -> bool {
	let id = <dyn ParachainHost<polkadot_primitives::Block>>::ID;
	match relay.version(at).await {
		Ok(version) => version
			.api_version(&id)
			.is_some_and(|version| version >= NEWEST_INCLUDED_PROVIDES_VERSION),
		Err(e) => {
			log::debug!(target: LOG_TARGET, "Relay runtime version at {at:?} unavailable: {e}");
			false
		},
	}
}

/// Each source's newest root included in a finalized relay block.
#[async_trait::async_trait]
pub trait FinalizedRoots: Send + Sync {
	/// `para`'s newest included root as of the finalized relay block; `None` if unknown.
	async fn finalized_included_root(&self, para: ParaId) -> Option<StreamsRoot>;
}

/// [`FinalizedRoots`] read from the relay chain.
pub struct RelayFinalizedRoots(pub Arc<dyn RelayChainInterface>);

#[async_trait::async_trait]
impl FinalizedRoots for RelayFinalizedRoots {
	async fn finalized_included_root(&self, para: ParaId) -> Option<StreamsRoot> {
		let at = self
			.0
			.finalized_block_hash()
			.await
			.map_err(|e| log::debug!(target: LOG_TARGET, "No finalized relay block: {e}"))
			.ok()?;
		if !relay_has_api(&*self.0, at).await {
			return None;
		}
		newest_included_provides(&*self.0, at, para).await
	}
}
