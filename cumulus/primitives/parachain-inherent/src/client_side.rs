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
	relay_chain::{
		self,
		v1::{HrmpChannelId, ParachainHost},
		Block as PBlock, Hash as PHash,
	},
	InboundDownwardMessage, InboundHrmpMessage, ParaId, PersistedValidationData,
};
use polkadot_client::{Client, ClientHandle, ExecuteWithClient};
use sc_client_api::Backend;
use sp_api::ProvideRuntimeApi;
use sp_runtime::generic::BlockId;
use sp_state_machine::Backend as _;
use std::collections::BTreeMap;

const LOG_TARGET: &str = "parachain-inherent";

/// Returns the whole contents of the downward message queue for the parachain we are collating
/// for.
///
/// Returns `None` in case of an error.
fn retrieve_dmq_contents<PClient>(
	polkadot_client: &PClient,
	para_id: ParaId,
	relay_parent: PHash,
) -> Option<Vec<InboundDownwardMessage>>
where
	PClient: ProvideRuntimeApi<PBlock>,
	PClient::Api: ParachainHost<PBlock>,
{
	polkadot_client
		.runtime_api()
		.dmq_contents_with_context(
			&BlockId::hash(relay_parent),
			sp_core::ExecutionContext::Importing,
			para_id,
		)
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				error = ?e,
				"An error occured during requesting the downward messages.",
			);
		})
		.ok()
}

/// Returns channels contents for each inbound HRMP channel addressed to the parachain we are
/// collating for.
///
/// Empty channels are also included.
fn retrieve_all_inbound_hrmp_channel_contents<PClient>(
	polkadot_client: &PClient,
	para_id: ParaId,
	relay_parent: PHash,
) -> Option<BTreeMap<ParaId, Vec<InboundHrmpMessage>>>
where
	PClient: ProvideRuntimeApi<PBlock>,
	PClient::Api: ParachainHost<PBlock>,
{
	polkadot_client
		.runtime_api()
		.inbound_hrmp_channels_contents_with_context(
			&BlockId::hash(relay_parent),
			sp_core::ExecutionContext::Importing,
			para_id,
		)
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				error = ?e,
				"An error occured during requesting the inbound HRMP messages.",
			);
		})
		.ok()
}

/// Collect the relevant relay chain state in form of a proof for putting it into the validation
/// data inherent.
fn collect_relay_storage_proof(
	polkadot_backend: &impl Backend<PBlock>,
	para_id: ParaId,
	relay_parent: PHash,
) -> Option<sp_state_machine::StorageProof> {
	use relay_chain::well_known_keys as relay_well_known_keys;

	let relay_parent_state_backend = polkadot_backend
		.state_at(BlockId::Hash(relay_parent))
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				error = ?e,
				"Cannot obtain the state of the relay chain.",
			)
		})
		.ok()?;

	let ingress_channels = relay_parent_state_backend
		.storage(&relay_well_known_keys::hrmp_ingress_channel_index(para_id))
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				error = ?e,
				"Cannot obtain the hrmp ingress channel index."
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

	let egress_channels = relay_parent_state_backend
		.storage(&relay_well_known_keys::hrmp_egress_channel_index(para_id))
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				error = ?e,
				"Cannot obtain the hrmp egress channel index.",
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

	let mut relevant_keys = vec![];
	relevant_keys.push(relay_well_known_keys::CURRENT_SLOT.to_vec());
	relevant_keys.push(relay_well_known_keys::ACTIVE_CONFIG.to_vec());
	relevant_keys.push(relay_well_known_keys::dmq_mqc_head(para_id));
	relevant_keys.push(relay_well_known_keys::relay_dispatch_queue_size(para_id));
	relevant_keys.push(relay_well_known_keys::hrmp_ingress_channel_index(para_id));
	relevant_keys.push(relay_well_known_keys::hrmp_egress_channel_index(para_id));
	relevant_keys.extend(ingress_channels.into_iter().map(|sender| {
		relay_well_known_keys::hrmp_channels(HrmpChannelId { sender, recipient: para_id })
	}));
	relevant_keys.extend(egress_channels.into_iter().map(|recipient| {
		relay_well_known_keys::hrmp_channels(HrmpChannelId { sender: para_id, recipient })
	}));

	sp_state_machine::prove_read(relay_parent_state_backend, relevant_keys)
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				error = ?e,
				"Failed to collect required relay chain state storage proof.",
			)
		})
		.ok()
}

impl ParachainInherentData {
	/// Create the [`ParachainInherentData`] at the given `relay_parent`.
	///
	/// Returns `None` if the creation failed.
	pub fn create_at<PClient>(
		relay_parent: PHash,
		polkadot_client: &PClient,
		polkadot_backend: &impl Backend<PBlock>,
		validation_data: &PersistedValidationData,
		para_id: ParaId,
	) -> Option<ParachainInherentData>
	where
		PClient: ProvideRuntimeApi<PBlock>,
		PClient::Api: ParachainHost<PBlock>,
	{
		let relay_chain_state =
			collect_relay_storage_proof(polkadot_backend, para_id, relay_parent)?;
		let downward_messages = retrieve_dmq_contents(polkadot_client, para_id, relay_parent)?;
		let horizontal_messages =
			retrieve_all_inbound_hrmp_channel_contents(polkadot_client, para_id, relay_parent)?;

		Some(ParachainInherentData {
			downward_messages,
			horizontal_messages,
			validation_data: validation_data.clone(),
			relay_chain_state,
		})
	}

	/// Create the [`ParachainInherentData`] at the given `relay_parent`.
	///
	/// Returns `None` if the creation failed.
	pub fn create_at_with_client(
		relay_parent: PHash,
		polkadot_client: &Client,
		relay_chain_backend: &impl Backend<PBlock>,
		validation_data: &PersistedValidationData,
		para_id: ParaId,
	) -> Option<ParachainInherentData> {
		polkadot_client.execute_with(CreateAtWithClient {
			relay_chain_backend,
			validation_data,
			para_id,
			relay_parent,
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

/// Special structure to run [`ParachainInherentData::create_at`] with a [`Client`].
struct CreateAtWithClient<'a, B> {
	relay_parent: PHash,
	relay_chain_backend: &'a B,
	validation_data: &'a PersistedValidationData,
	para_id: ParaId,
}

impl<'a, B> ExecuteWithClient for CreateAtWithClient<'a, B>
where
	B: Backend<PBlock>,
{
	type Output = Option<ParachainInherentData>;

	fn execute_with_client<Client, Api, Backend>(
		self,
		client: std::sync::Arc<Client>,
	) -> Self::Output
	where
		Client: ProvideRuntimeApi<PBlock>,
		Client::Api: ParachainHost<PBlock>,
	{
		ParachainInherentData::create_at(
			self.relay_parent,
			&*client,
			self.relay_chain_backend,
			self.validation_data,
			self.para_id,
		)
	}
}
