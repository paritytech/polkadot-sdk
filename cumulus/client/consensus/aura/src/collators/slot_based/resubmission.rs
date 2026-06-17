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

//! Shared helpers for resolving relay-chain data needed to record resubmission entries,
//! used by both the build path (block_builder_task) and the import path (block_import).

use cumulus_primitives_core::{
	relay_chain::{Hash as RelayHash, SessionIndex},
	PersistedValidationData,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_primitives::{Id as ParaId, OccupiedCoreAssumption};

const LOG_TARGET: &str = "aura::resubmission";

/// Fetch the session index and persisted validation data (with `OccupiedCoreAssumption::TimedOut`)
/// for the given relay parent and para id.
///
/// Returns `None` on any error. The returned PVD is the relay-chain's own PVD
/// as-is — its `parent_head` is the currently-included head, which is the authoritative value for
/// resubmission.
pub(crate) async fn resolve_session_and_pvd<R: RelayChainInterface + ?Sized>(
	relay_client: &R,
	relay_parent: RelayHash,
	para_id: ParaId,
) -> Option<(SessionIndex, PersistedValidationData)> {
	let session = match relay_client.session_index_for_child(relay_parent).await {
		Ok(s) => s,
		Err(err) => {
			tracing::debug!(
				target: LOG_TARGET,
				?relay_parent,
				?err,
				"Failed to fetch relay-parent session; skipping resubmission entry.",
			);
			return None;
		},
	};

	let pvd = match relay_client
		.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::TimedOut)
		.await
	{
		Ok(Some(pvd)) => pvd,
		Ok(None) => {
			tracing::debug!(
				target: LOG_TARGET,
				?relay_parent,
				"No persisted validation data (TimedOut); skipping resubmission entry.",
			);
			return None;
		},
		Err(err) => {
			tracing::debug!(
				target: LOG_TARGET,
				?relay_parent,
				?err,
				"Failed to fetch persisted validation data; skipping resubmission entry.",
			);
			return None;
		},
	};

	Some((session, pvd))
}
