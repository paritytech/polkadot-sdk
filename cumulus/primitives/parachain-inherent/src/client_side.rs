// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Client side code for generating the parachain inherent.

use crate::ParachainInherentData;
use codec::Decode;
use cumulus_primitives_core::{
	relay_chain::{self, v1::HrmpChannelId, Hash as PHash},
	ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use sp_runtime::generic::BlockId;

const LOG_TARGET: &str = "parachain-inherent";

/// Collect the relevant relay chain state in form of a proof for putting it into the validation
/// data inherent.
async fn collect_relay_storage_proof(
	relay_chain_interface: &impl RelayChainInterface,
	para_id: ParaId,
	relay_parent: PHash,
) -> Option<sp_state_machine::StorageProof> {
	use relay_chain::well_known_keys as relay_well_known_keys;

	let relay_parent_block_id = BlockId::Hash(relay_parent);
	let ingress_channels = relay_chain_interface
		.get_storage_by_key(
			&relay_parent_block_id,
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
			&relay_parent_block_id,
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

	let mut relevant_keys = Vec::new();
	relevant_keys.push(relay_well_known_keys::CURRENT_SLOT.to_vec());
	relevant_keys.push(relay_well_known_keys::ACTIVE_CONFIG.to_vec());
	relevant_keys.push(relay_well_known_keys::dmq_mqc_head(para_id));
	relevant_keys.push(relay_well_known_keys::relay_dispatch_queue_size(para_id));
	relevant_keys.push(relay_well_known_keys::hrmp_ingress_channel_index(para_id));
	relevant_keys.push(relay_well_known_keys::hrmp_egress_channel_index(para_id));
	relevant_keys.push(relay_well_known_keys::upgrade_go_ahead_signal(para_id));
	relevant_keys.push(relay_well_known_keys::upgrade_restriction_signal(para_id));
	relevant_keys.extend(ingress_channels.into_iter().map(|sender| {
		relay_well_known_keys::hrmp_channels(HrmpChannelId { sender, recipient: para_id })
	}));
	relevant_keys.extend(egress_channels.into_iter().map(|recipient| {
		relay_well_known_keys::hrmp_channels(HrmpChannelId { sender: para_id, recipient })
	}));

	relay_chain_interface
		.prove_read(&relay_parent_block_id, &relevant_keys)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent_block_id,
				error = ?e,
				"Cannot obtain read proof from relay chain.",
			);
		})
		.ok()
}

impl ParachainInherentData {
	/// Create the [`ParachainInherentData`] at the given `relay_parent`.
	///
	/// Returns `None` if the creation failed.
	pub async fn create_at(
		relay_parent: PHash,
		relay_chain_interface: &impl RelayChainInterface,
		validation_data: &PersistedValidationData,
		para_id: ParaId,
	) -> Option<ParachainInherentData> {
		let relay_chain_state =
			collect_relay_storage_proof(relay_chain_interface, para_id, relay_parent).await?;

		let downward_messages = relay_chain_interface
			.retrieve_dmq_contents(para_id, relay_parent)
			.await
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					relay_parent = ?relay_parent,
					error = ?e,
					"An error occured during requesting the downward messages.",
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
					"An error occured during requesting the inbound HRMP messages.",
				);
			})
			.ok()?;

		Some(ParachainInherentData {
			downward_messages,
			horizontal_messages,
			validation_data: validation_data.clone(),
			relay_chain_state,
		})
	}
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for ParachainInherentData {
	fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(crate::INHERENT_IDENTIFIER, &self)
	}

	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}
