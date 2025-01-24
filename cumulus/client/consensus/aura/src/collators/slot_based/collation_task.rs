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

use codec::Encode;
use std::{fs, fs::File, path::PathBuf};

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{MaybeCompressedPoV, PoV, SubmitCollationParams};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{
	BlockNumber as RelayBlockNumber, CollatorPair, Hash as RelayHash, Id as ParaId,
};

use cumulus_primitives_core::relay_chain::HeadData;
use futures::prelude::*;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::traits::{Block as BlockT, Header, NumberFor};

use super::CollatorMessage;

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// Parameters for the collation task.
pub struct Params<Block: BlockT, RClient, CS> {
	/// A handle to the relay-chain client.
	pub relay_client: RClient,
	/// The collator key used to sign collations before submitting to validators.
	pub collator_key: CollatorPair,
	/// The para's ID.
	pub para_id: ParaId,
	/// Whether we should reinitialize the collator config (i.e. we are transitioning to aura).
	pub reinitialize: bool,
	/// Collator service interface
	pub collator_service: CS,
	/// Receiver channel for communication with the block builder task.
	pub collator_receiver: TracingUnboundedReceiver<CollatorMessage<Block>>,
	/// The handle from the special slot based block import.
	pub block_import_handle: Option<super::SlotBasedBlockImportHandle<Block>>,
	/// Whether to export the PoV to a file. Useful for debugging purposes.
	pub export_pov: Option<PathBuf>,
}

/// Asynchronously executes the collation task for a parachain.
///
/// This function initializes the collator subsystems necessary for producing and submitting
/// collations to the relay chain. It listens for new best relay chain block notifications and
/// handles collator messages. If our parachain is scheduled on a core and we have a candidate,
/// the task will build a collation and send it to the relay chain.
pub async fn run_collation_task<Block, RClient, CS>(
	Params {
		relay_client,
		collator_key,
		para_id,
		reinitialize,
		collator_service,
		mut collator_receiver,
		mut block_import_handle,
		export_pov,
	}: Params<Block, RClient, CS>,
) where
	Block: BlockT,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	RClient: RelayChainInterface + Clone + 'static,
{
	let Ok(mut overseer_handle) = relay_client.overseer_handle() else {
		tracing::error!(target: LOG_TARGET, "Failed to get overseer handle.");
		return
	};

	cumulus_client_collator::initialize_collator_subsystems(
		&mut overseer_handle,
		collator_key,
		para_id,
		reinitialize,
	)
	.await;

	loop {
		tokio::select! {
				collator_message = collator_receiver.next() => {
					let Some(message) = collator_message else {
						return;
					};

					handle_collation_message(message, &collator_service, &mut overseer_handle, &export_pov).await;
				},
				block_import_msg = block_import_handle.as_mut().map(|h| h.next().fuse()).unwrap(), if block_import_handle.is_some() => {
				let (_, _) = block_import_msg;
				// TODO: Implement me.
				// Issue: https://github.com/paritytech/polkadot-sdk/issues/6495
		}
			}
	}
}

/// Handle an incoming collation message from the block builder task.
/// This builds the collation from the [`CollatorMessage`] and submits it to
/// the collation-generation subsystem of the relay chain.
async fn handle_collation_message<Block: BlockT>(
	message: CollatorMessage<Block>,
	collator_service: &impl CollatorServiceInterface<Block>,
	overseer_handle: &mut OverseerHandle,
	export_pov: &Option<PathBuf>,
) {
	let CollatorMessage {
		parent_header,
		parachain_candidate,
		validation_code_hash,
		relay_parent_header,
		core_index,
	} = message;

	let hash = parachain_candidate.block.header().hash();
	let number = *parachain_candidate.block.header().number();
	let (collation, block_data) =
		match collator_service.build_collation(&parent_header, hash, parachain_candidate) {
			Some(collation) => collation,
			None => {
				tracing::warn!(target: LOG_TARGET, %hash, ?number, ?core_index, "Unable to build collation.");
				return;
			},
		};

	tracing::info!(
		target: LOG_TARGET,
		"PoV size {{ header: {:.2}kB, extrinsics: {:.2}kB, storage_proof: {:.2}kB }}",
		block_data.header().encoded_size() as f64 / 1024f64,
		block_data.extrinsics().encoded_size() as f64 / 1024f64,
		block_data.storage_proof().encoded_size() as f64 / 1024f64,
	);

	if let Some(ref export_pov_path) = export_pov {
		export_pov_to_path::<Block>(
			export_pov_path,
			collation.proof_of_validity.clone().into_compressed(),
			hash,
			*block_data.header().number(),
			parent_header.clone(),
			*relay_parent_header.state_root(),
			*relay_parent_header.number(),
		);
	}

	if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
		tracing::info!(
			target: LOG_TARGET,
			"Compressed PoV size: {}kb",
			pov.block_data.0.len() as f64 / 1024f64,
		);
	}

	tracing::debug!(target: LOG_TARGET, ?core_index, %hash, %number, "Submitting collation for core.");
	overseer_handle
		.send_msg(
			CollationGenerationMessage::SubmitCollation(SubmitCollationParams {
				relay_parent: relay_parent_header.hash(),
				collation,
				parent_head: parent_header.encode().into(),
				validation_code_hash,
				core_index,
				result_sender: None,
			}),
			"SubmitCollation",
		)
		.await;
}

/// Export the given `pov` to the file system at `path`.
///
/// The file will be named `block_hash_block_number.pov`.
///
/// The `parent_header`, `relay_parent_storage_root` and `relay_parent_number` will also be
/// stored in the file alongside the `pov`. This enables stateless validation of the `pov`.
fn export_pov_to_path<Block: BlockT>(
	path: &PathBuf,
	pov: PoV,
	block_hash: Block::Hash,
	block_number: NumberFor<Block>,
	parent_header: Block::Header,
	relay_parent_storage_root: RelayHash,
	relay_parent_number: RelayBlockNumber,
) {
	if let Err(error) = fs::create_dir_all(&path) {
		tracing::error!(target: LOG_TARGET, %error, path = %path.display(), "Failed to create PoV export directory");
		return
	}

	let mut file = match File::create(path.join(format!("{block_hash:?}_{block_number}.pov"))) {
		Ok(f) => f,
		Err(error) => {
			tracing::error!(target: LOG_TARGET, %error, "Failed to export PoV.");
			return
		},
	};

	pov.encode_to(&mut file);
	HeadData(parent_header.encode()).encode_to(&mut file);
	relay_parent_storage_root.encode_to(&mut file);
	relay_parent_number.encode_to(&mut file);
}
