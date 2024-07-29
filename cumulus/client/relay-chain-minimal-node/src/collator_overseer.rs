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

use futures::{select, StreamExt};
use std::sync::Arc;

use polkadot_overseer::{
	BlockInfo, Handle, Overseer, OverseerConnector, OverseerHandle, SpawnGlue, UnpinHandle,
};
use polkadot_service::overseer::{collator_overseer_builder, OverseerGenArgs};

use sc_service::TaskManager;
use sc_utils::mpsc::tracing_unbounded;

use cumulus_relay_chain_interface::RelayChainError;

use crate::BlockChainRpcClient;

fn build_overseer(
	connector: OverseerConnector,
	args: OverseerGenArgs<sc_service::SpawnTaskHandle, BlockChainRpcClient>,
) -> Result<
	(Overseer<SpawnGlue<sc_service::SpawnTaskHandle>, Arc<BlockChainRpcClient>>, OverseerHandle),
	RelayChainError,
> {
	let builder =
		collator_overseer_builder(args).map_err(|e| RelayChainError::Application(e.into()))?;

	builder
		.build_with_connector(connector)
		.map_err(|e| RelayChainError::Application(e.into()))
}

pub(crate) fn spawn_overseer(
	overseer_args: OverseerGenArgs<sc_service::SpawnTaskHandle, BlockChainRpcClient>,
	task_manager: &TaskManager,
	relay_chain_rpc_client: Arc<BlockChainRpcClient>,
) -> Result<polkadot_overseer::Handle, RelayChainError> {
	let (overseer, overseer_handle) = build_overseer(OverseerConnector::default(), overseer_args)
		.map_err(|e| {
		tracing::error!("Failed to initialize overseer: {}", e);
		e
	})?;

	let overseer_handle = Handle::new(overseer_handle);
	{
		let handle = overseer_handle.clone();
		task_manager.spawn_essential_handle().spawn_blocking(
			"overseer",
			None,
			Box::pin(async move {
				use futures::{pin_mut, FutureExt};

				let forward = forward_collator_events(relay_chain_rpc_client, handle).fuse();

				let overseer_fut = overseer.run().fuse();

				pin_mut!(overseer_fut);
				pin_mut!(forward);

				select! {
					_ = forward => (),
					_ = overseer_fut => (),
				}
			}),
		);
	}
	Ok(overseer_handle)
}

/// Minimal relay chain node representation
pub struct NewMinimalNode {
	/// Task manager running all tasks for the minimal node
	pub task_manager: TaskManager,
	/// Overseer handle to interact with subsystems
	pub overseer_handle: Handle,
}

/// Glues together the [`Overseer`] and `BlockchainEvents` by forwarding
/// import and finality notifications into the [`OverseerHandle`].
async fn forward_collator_events(
	client: Arc<BlockChainRpcClient>,
	mut handle: Handle,
) -> Result<(), RelayChainError> {
	let mut finality = client.finality_notification_stream().await?.fuse();
	let mut imports = client.import_notification_stream().await?.fuse();
	// Collators do no need to pin any specific blocks
	let (dummy_sink, _) = tracing_unbounded("does-not-matter", 42);
	let dummy_unpin_handle = UnpinHandle::new(Default::default(), dummy_sink);

	loop {
		select! {
			f = finality.next() => {
				match f {
					Some(header) => {
						let hash = header.hash();
						tracing::info!(
							target: "minimal-polkadot-node",
							"Received finalized block via RPC: #{} ({} -> {})",
							header.number,
							header.parent_hash,
							hash,
						);
						let unpin_handle = dummy_unpin_handle.clone();
						let block_info = BlockInfo { hash, parent_hash: header.parent_hash, number: header.number, unpin_handle };
						handle.block_finalized(block_info).await;
					}
					None => return Err(RelayChainError::GenericError("Relay chain finality stream ended.".to_string())),
				}
			},
			i = imports.next() => {
				match i {
					Some(header) => {
						let hash = header.hash();
						tracing::info!(
							target: "minimal-polkadot-node",
							"Received imported block via RPC: #{} ({} -> {})",
							header.number,
							header.parent_hash,
							hash,
						);
						let unpin_handle = dummy_unpin_handle.clone();
						let block_info = BlockInfo { hash, parent_hash: header.parent_hash, number: header.number, unpin_handle };
						handle.block_imported(block_info).await;
					}
					None => return Err(RelayChainError::GenericError("Relay chain import stream ended.".to_string())),
				}
			}
		}
	}
}
