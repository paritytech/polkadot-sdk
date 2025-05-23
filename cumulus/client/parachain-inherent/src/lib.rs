// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Client side code for generating the parachain inherent.

use codec::Decode;
use cumulus_primitives_core::{
	relay_chain::{self, Block as RelayBlock, Hash as PHash, HrmpChannelId},
	ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::RelayChainInterface;

mod mock;

use cumulus_primitives_core::relay_chain::Header as RelayHeader;
pub use cumulus_primitives_parachain_inherent::{ParachainInherentData, INHERENT_IDENTIFIER};
pub use mock::{MockValidationDataInherentDataProvider, MockXcmConfig};

const LOG_TARGET: &str = "parachain-inherent";

/// Collect the relevant relay chain state in form of a proof for putting it into the validation
/// data inherent.
async fn collect_relay_storage_proof(
	relay_chain_interface: &impl RelayChainInterface,
	para_id: ParaId,
	relay_parent: PHash,
	include_authorities: bool,
	include_next_authorities: bool,
) -> Option<sp_state_machine::StorageProof> {
	use relay_chain::well_known_keys as relay_well_known_keys;

	let ingress_channels = relay_chain_interface
		.get_storage_by_key(
			relay_parent,
			&relay_well_known_keys::hrmp_ingress_channel_index(para_id),
		)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				error = ?e,
				"Cannot obtain the hrmp ingress channel."
			)
		})
		.ok()?;

	let ingress_channels = ingress_channels
		.map(|raw| <Vec<ParaId>>::decode(&mut &raw[..]))
		.transpose()
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				error = ?e,
				"Cannot decode the hrmp ingress channel index.",
			)
		})
		.ok()?
		.unwrap_or_default();

	let egress_channels = relay_chain_interface
		.get_storage_by_key(
			relay_parent,
			&relay_well_known_keys::hrmp_egress_channel_index(para_id),
		)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				error = ?e,
				"Cannot obtain the hrmp egress channel.",
			)
		})
		.ok()?;

	let egress_channels = egress_channels
		.map(|raw| <Vec<ParaId>>::decode(&mut &raw[..]))
		.transpose()
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				error = ?e,
				"Cannot decode the hrmp egress channel index.",
			)
		})
		.ok()?
		.unwrap_or_default();

	let mut relevant_keys = vec![
		relay_well_known_keys::CURRENT_BLOCK_RANDOMNESS.to_vec(),
		relay_well_known_keys::ONE_EPOCH_AGO_RANDOMNESS.to_vec(),
		relay_well_known_keys::TWO_EPOCHS_AGO_RANDOMNESS.to_vec(),
		relay_well_known_keys::CURRENT_SLOT.to_vec(),
		relay_well_known_keys::ACTIVE_CONFIG.to_vec(),
		relay_well_known_keys::dmq_mqc_head(para_id),
		// TODO paritytech/polkadot#6283: Remove all usages of `relay_dispatch_queue_size`
		// We need to keep this here until all parachains have migrated to
		// `relay_dispatch_queue_remaining_capacity`.
		#[allow(deprecated)]
		relay_well_known_keys::relay_dispatch_queue_size(para_id),
		relay_well_known_keys::relay_dispatch_queue_remaining_capacity(para_id).key,
		relay_well_known_keys::hrmp_ingress_channel_index(para_id),
		relay_well_known_keys::hrmp_egress_channel_index(para_id),
		relay_well_known_keys::upgrade_go_ahead_signal(para_id),
		relay_well_known_keys::upgrade_restriction_signal(para_id),
		relay_well_known_keys::para_head(para_id),
	];
	relevant_keys.extend(ingress_channels.into_iter().map(|sender| {
		relay_well_known_keys::hrmp_channels(HrmpChannelId { sender, recipient: para_id })
	}));
	relevant_keys.extend(egress_channels.into_iter().map(|recipient| {
		relay_well_known_keys::hrmp_channels(HrmpChannelId { sender: para_id, recipient })
	}));

	if include_authorities {
		relevant_keys.push(relay_well_known_keys::AUTHORITIES.to_vec());
	}

	if include_next_authorities {
		relevant_keys.push(relay_well_known_keys::NEXT_AUTHORITIES.to_vec());
	}

	relay_chain_interface
		.prove_read(relay_parent, &relevant_keys)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				error = ?e,
				"Cannot obtain read proof from relay chain.",
			);
		})
		.ok()
}

pub struct ParachainInherentDataProvider;

impl ParachainInherentDataProvider {
	/// Create the [`ParachainInherentData`] at the given `relay_parent`.
	///
	/// Returns `None` if the creation failed.
	pub async fn create_at(
		relay_parent: PHash,
		relay_chain_interface: &impl RelayChainInterface,
		validation_data: &PersistedValidationData,
		para_id: ParaId,
		relay_parent_descendants: Vec<RelayHeader>,
	) -> Option<ParachainInherentData> {
		// Only include next epoch authorities when the descendants include an epoch digest.
		// Skip the first entry because this is the relay parent itself.
		let include_next_authorities = relay_parent_descendants.iter().skip(1).any(|header| {
			sc_consensus_babe::find_next_epoch_digest::<RelayBlock>(header)
				.ok()
				.flatten()
				.is_some()
		});
		let relay_chain_state = collect_relay_storage_proof(
			relay_chain_interface,
			para_id,
			relay_parent,
			!relay_parent_descendants.is_empty(),
			include_next_authorities,
		)
		.await?;

		let downward_messages = relay_chain_interface
			.retrieve_dmq_contents(para_id, relay_parent)
			.await
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					relay_parent = ?relay_parent,
					error = ?e,
					"An error occurred during requesting the downward messages.",
				);
			})
			.ok()?;
		let horizontal_messages = relay_chain_interface
			.retrieve_all_inbound_hrmp_channel_contents(para_id, relay_parent)
			.await
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					relay_parent = ?relay_parent,
					error = ?e,
					"An error occurred during requesting the inbound HRMP messages.",
				);
			})
			.ok()?;

		Some(ParachainInherentData {
			downward_messages,
			horizontal_messages,
			validation_data: validation_data.clone(),
			relay_chain_state,
			relay_parent_descendants,
		})
	}
}
