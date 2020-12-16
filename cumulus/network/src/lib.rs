// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Parachain specific networking
//!
//! Provides a custom block announcement implementation for parachains
//! that use the relay chain provided consensus. See [`BlockAnnounceValidator`]
//! and [`WaitToAnnounce`] for more information about this implementation.

#[cfg(test)]
mod tests;
mod wait_on_relay_chain_block;

use sc_client_api::{Backend, BlockchainEvents};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::{
	block_validation::{BlockAnnounceValidator as BlockAnnounceValidatorT, Validation},
	SyncOracle,
};
use sp_core::traits::SpawnNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, HashFor, Header as HeaderT},
};

use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_node_subsystem::messages::StatementDistributionMessage;
use polkadot_overseer::OverseerHandler;
use polkadot_primitives::v1::{
	Block as PBlock, Hash as PHash, Id as ParaId, OccupiedCoreAssumption, ParachainHost,
	SigningContext,
};
use polkadot_service::ClientHandle;

use codec::{Decode, Encode};
use futures::{
	channel::{mpsc, oneshot},
	future::{ready, FutureExt},
	pin_mut, select, Future, StreamExt,
};
use log::trace;

use std::{fmt, marker::PhantomData, pin::Pin, sync::Arc};

use wait_on_relay_chain_block::WaitOnRelayChainBlock;

type BoxedError = Box<dyn std::error::Error + Send>;

#[derive(Debug)]
struct BlockAnnounceError(String);
impl std::error::Error for BlockAnnounceError {}

impl fmt::Display for BlockAnnounceError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

/// Parachain specific block announce validator.
///
/// This block announce validator is required if the parachain is running
/// with the relay chain provided consensus to make sure each node only
/// imports a reasonable number of blocks per round. The relay chain provided
/// consensus doesn't have any authorities and so it could happen that without
/// this special block announce validator a node would need to import *millions*
/// of blocks per round, which is clearly not doable.
///
/// To solve this problem, each block announcement is delayed until a collator
/// has received a [`Statement::Seconded`] for its `PoV`. This message tells the
/// collator that its `PoV` was validated successfully by a parachain validator and
/// that it is very likely that this `PoV` will be included in the relay chain. Every
/// collator that doesn't receive the message for its `PoV` will not announce its block.
/// For more information on the block announcement, see [`WaitToAnnounce`].
///
/// For each block announcement that is received, the generic block announcement validation
/// will call this validator and provides the extra data that was attached to the announcement.
/// We call this extra data `justification`.
/// It is expected that the attached data is a SCALE encoded [`SignedFullStatement`]. The
/// statement is checked to be a [`Statement::Seconded`] and that it is signed by an active
/// parachain validator.
///
/// If no justification was provided we check if the block announcement is at the tip of the known
/// chain. If it is at the tip, it is required to provide a justification or otherwise we reject
/// it. However, if the announcement is for a block below the tip the announcement is accepted
/// as it probably comes from a node that is currently syncing the chain.
pub struct BlockAnnounceValidator<Block, P, B, BCE> {
	phantom: PhantomData<Block>,
	relay_chain_client: Arc<P>,
	relay_chain_backend: Arc<B>,
	para_id: ParaId,
	relay_chain_sync_oracle: Box<dyn SyncOracle + Send>,
	wait_on_relay_chain_block: WaitOnRelayChainBlock<B, BCE>,
}

impl<Block, P, B, BCE> BlockAnnounceValidator<Block, P, B, BCE> {
	/// Create a new [`BlockAnnounceValidator`].
	pub fn new(
		relay_chain_client: Arc<P>,
		para_id: ParaId,
		relay_chain_sync_oracle: Box<dyn SyncOracle + Send>,
		relay_chain_backend: Arc<B>,
		relay_chain_blockchain_events: Arc<BCE>,
	) -> Self {
		Self {
			phantom: Default::default(),
			relay_chain_client,
			para_id,
			relay_chain_sync_oracle,
			relay_chain_backend: relay_chain_backend.clone(),
			wait_on_relay_chain_block: WaitOnRelayChainBlock::new(
				relay_chain_backend,
				relay_chain_blockchain_events,
			),
		}
	}
}

impl<Block: BlockT, P, B, BCE> BlockAnnounceValidator<Block, P, B, BCE>
where
	P: ProvideRuntimeApi<PBlock> + Send + Sync + 'static,
	P::Api: ParachainHost<PBlock>,
	B: Backend<PBlock> + 'static,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<B, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
{
	/// Handle a block announcement with empty data (no statement) attached to it.
	fn handle_empty_block_announce_data(
		&self,
		header: Block::Header,
	) -> impl Future<Output = Result<Validation, BoxedError>> {
		let relay_chain_client = self.relay_chain_client.clone();
		let relay_chain_backend = self.relay_chain_backend.clone();
		let para_id = self.para_id;

		async move {
			// Check if block is equal or higher than best (this requires a justification)
			let relay_chain_info = relay_chain_backend.blockchain().info();
			let runtime_api_block_id = BlockId::Hash(relay_chain_info.best_hash);
			let block_number = header.number();

			let local_validation_data = relay_chain_client
				.runtime_api()
				.persisted_validation_data(
					&runtime_api_block_id,
					para_id,
					OccupiedCoreAssumption::TimedOut,
				)
				.map_err(|e| Box::new(BlockAnnounceError(format!("{:?}", e))) as Box<_>)?
				.ok_or_else(|| {
					Box::new(BlockAnnounceError(
						"Could not find parachain head in relay chain".into(),
					)) as Box<_>
				})?;
			let parent_head = Block::Header::decode(&mut &local_validation_data.parent_head.0[..])
				.map_err(|e| {
					Box::new(BlockAnnounceError(format!(
						"Failed to decode parachain head: {:?}",
						e
					))) as Box<_>
				})?;
			let known_best_number = parent_head.number();

			if block_number >= known_best_number {
				trace!(
					target: "cumulus-network",
					"validation failed because a justification is needed if the block at the top of the chain."
				);

				Ok(Validation::Failure { disconnect: false })
			} else {
				Ok(Validation::Success { is_new_best: false })
			}
		}
	}
}

impl<Block: BlockT, P, B, BCE> BlockAnnounceValidatorT<Block>
	for BlockAnnounceValidator<Block, P, B, BCE>
where
	P: ProvideRuntimeApi<PBlock> + Send + Sync + 'static,
	P::Api: ParachainHost<PBlock>,
	B: Backend<PBlock> + 'static,
	BCE: BlockchainEvents<PBlock> + 'static + Send + Sync,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<B, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
{
	fn validate(
		&mut self,
		header: &Block::Header,
		mut data: &[u8],
	) -> Pin<Box<dyn Future<Output = Result<Validation, BoxedError>> + Send>> {
		if self.relay_chain_sync_oracle.is_major_syncing() {
			return ready(Ok(Validation::Success { is_new_best: false })).boxed();
		}

		if data.is_empty() {
			return self
				.handle_empty_block_announce_data(header.clone())
				.boxed();
		}

		let signed_stmt = match SignedFullStatement::decode(&mut data) {
			Ok(r) => r,
			Err(_) => return ready(Err(Box::new(BlockAnnounceError(
				"cannot decode block announcement justification, must be a `SignedFullStatement`"
					.into(),
			)) as Box<_>))
			.boxed(),
		};

		let relay_chain_client = self.relay_chain_client.clone();
		let header_encoded = header.encode();
		let wait_on_relay_chain_block = self.wait_on_relay_chain_block.clone();

		async move {
			// Check statement is a candidate statement.
			let candidate_receipt = match signed_stmt.payload() {
				Statement::Seconded(ref candidate_receipt) => candidate_receipt,
				_ => {
					return Err(Box::new(BlockAnnounceError(
						"block announcement justification must be a `Statement::Seconded`".into(),
					)) as Box<_>)
				}
			};

			// Check the header in the candidate_receipt match header given header.
			if header_encoded != candidate_receipt.commitments.head_data.0 {
				return Err(Box::new(BlockAnnounceError(
					"block announcement header does not match the one justified".into(),
				)) as Box<_>);
			}

			let relay_parent = &candidate_receipt.descriptor.relay_parent;

			wait_on_relay_chain_block
				.wait_on_relay_chain_block(*relay_parent)
				.await
				.map_err(|e| Box::new(BlockAnnounceError(e.to_string())) as Box<_>)?;

			let runtime_api = relay_chain_client.runtime_api();
			let validator_index = signed_stmt.validator_index();

			let runtime_api_block_id = BlockId::Hash(*relay_parent);
			let session_index = match runtime_api.session_index_for_child(&runtime_api_block_id) {
				Ok(r) => r,
				Err(e) => {
					return Err(Box::new(BlockAnnounceError(format!("{:?}", e))) as Box<_>);
				}
			};

			let signing_context = SigningContext {
				parent_hash: *relay_parent,
				session_index,
			};

			// Check that the signer is a legit validator.
			let authorities = match runtime_api.validators(&runtime_api_block_id) {
				Ok(r) => r,
				Err(e) => {
					return Err(Box::new(BlockAnnounceError(format!("{:?}", e))) as Box<_>);
				}
			};
			let signer = match authorities.get(validator_index as usize) {
				Some(r) => r,
				None => {
					return Err(Box::new(BlockAnnounceError(
						"block accouncement justification signer is a validator index out of bound"
							.to_string(),
					)) as Box<_>);
				}
			};

			// Check statement is correctly signed.
			if signed_stmt
				.check_signature(&signing_context, &signer)
				.is_err()
			{
				return Err(Box::new(BlockAnnounceError(
					"block announcement justification signature is invalid".to_string(),
				)) as Box<_>);
			}

			Ok(Validation::Success { is_new_best: true })
		}
		.boxed()
	}
}

/// Build a block announce validator instance.
///
/// Returns a boxed [`BlockAnnounceValidator`].
pub fn build_block_announce_validator<Block: BlockT, B>(
	relay_chain_client: polkadot_service::Client,
	para_id: ParaId,
	relay_chain_sync_oracle: Box<dyn SyncOracle + Send>,
	relay_chain_backend: Arc<B>,
) -> Box<dyn BlockAnnounceValidatorT<Block> + Send>
where
	B: Backend<PBlock> + Send + 'static,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<B, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
{
	BlockAnnounceValidatorBuilder::new(
		relay_chain_client,
		para_id,
		relay_chain_sync_oracle,
		relay_chain_backend,
	)
	.build()
}

/// Block announce validator builder.
///
/// Builds a [`BlockAnnounceValidator`] for a parachain. As this requires
/// a concrete relay chain client instance, the builder takes a [`polkadot_service::Client`]
/// that wraps this concrete instanace. By using [`polkadot_service::ExecuteWithClient`]
/// the builder gets access to this concrete instance.
struct BlockAnnounceValidatorBuilder<Block, B> {
	phantom: PhantomData<Block>,
	relay_chain_client: polkadot_service::Client,
	para_id: ParaId,
	relay_chain_sync_oracle: Box<dyn SyncOracle + Send>,
	relay_chain_backend: Arc<B>,
}

impl<Block: BlockT, B> BlockAnnounceValidatorBuilder<Block, B>
where
	B: Backend<PBlock> + Send + 'static,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<B, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
{
	/// Create a new instance of the builder.
	fn new(
		relay_chain_client: polkadot_service::Client,
		para_id: ParaId,
		relay_chain_sync_oracle: Box<dyn SyncOracle + Send>,
		relay_chain_backend: Arc<B>,
	) -> Self {
		Self {
			relay_chain_client,
			para_id,
			relay_chain_sync_oracle,
			relay_chain_backend,
			phantom: PhantomData,
		}
	}

	/// Build the block announce validator.
	fn build(self) -> Box<dyn BlockAnnounceValidatorT<Block> + Send> {
		self.relay_chain_client.clone().execute_with(self)
	}
}

impl<Block: BlockT, B> polkadot_service::ExecuteWithClient
	for BlockAnnounceValidatorBuilder<Block, B>
where
	B: Backend<PBlock> + Send + 'static,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	sc_client_api::StateBackendFor<B, PBlock>: sc_client_api::StateBackend<HashFor<PBlock>>,
{
	type Output = Box<dyn BlockAnnounceValidatorT<Block> + Send>;

	fn execute_with_client<PClient, Api, PBackend>(self, client: Arc<PClient>) -> Self::Output
	where
		<Api as sp_api::ApiExt<PBlock>>::StateBackend:
			sp_api::StateBackend<sp_runtime::traits::BlakeTwo256>,
		PBackend: Backend<PBlock>,
		PBackend::State: sp_api::StateBackend<sp_runtime::traits::BlakeTwo256>,
		Api: polkadot_service::RuntimeApiCollection<StateBackend = PBackend::State>,
		PClient: polkadot_service::AbstractClient<PBlock, PBackend, Api = Api> + 'static,
	{
		Box::new(BlockAnnounceValidator::new(
			client.clone(),
			self.para_id,
			self.relay_chain_sync_oracle,
			self.relay_chain_backend,
			client,
		))
	}
}

/// Wait before announcing a block that a candidate message has been received for this block, then
/// add this message as justification for the block announcement.
///
/// This object will spawn a new task every time the method `wait_to_announce` is called and cancel
/// the previous task running.
pub struct WaitToAnnounce<Block: BlockT> {
	spawner: Arc<dyn SpawnNamed + Send + Sync>,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	overseer_handler: OverseerHandler,
	current_trigger: oneshot::Sender<()>,
}

impl<Block: BlockT> WaitToAnnounce<Block> {
	/// Create the `WaitToAnnounce` object
	pub fn new(
		spawner: Arc<dyn SpawnNamed + Send + Sync>,
		announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
		overseer_handler: OverseerHandler,
	) -> WaitToAnnounce<Block> {
		let (tx, _rx) = oneshot::channel();

		WaitToAnnounce {
			spawner,
			announce_block,
			overseer_handler,
			current_trigger: tx,
		}
	}

	/// Wait for a candidate message for the block, then announce the block. The candidate
	/// message will be added as justification to the block announcement.
	pub fn wait_to_announce(&mut self, block_hash: <Block as BlockT>::Hash, pov_hash: PHash) {
		let (tx, rx) = oneshot::channel();
		let announce_block = self.announce_block.clone();
		let overseer_handler = self.overseer_handler.clone();

		self.current_trigger = tx;

		self.spawner.spawn(
			"cumulus-wait-to-announce",
			async move {
				let t1 = wait_to_announce::<Block>(
					block_hash,
					pov_hash,
					announce_block,
					overseer_handler,
				)
				.fuse();
				let t2 = rx.fuse();

				pin_mut!(t1, t2);

				trace!(
					target: "cumulus-network",
					"waiting for announce block in a background task...",
				);

				select! {
					_ = t1 => {
						trace!(
							target: "cumulus-network",
							"block announcement finished",
						);
					},
					_ = t2 => {
						trace!(
							target: "cumulus-network",
							"previous task that waits for announce block has been canceled",
						);
					}
				}
			}
			.boxed(),
		);
	}
}

async fn wait_to_announce<Block: BlockT>(
	block_hash: <Block as BlockT>::Hash,
	pov_hash: PHash,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	mut overseer_handler: OverseerHandler,
) {
	let (sender, mut receiver) = mpsc::channel(5);
	overseer_handler
		.send_msg(StatementDistributionMessage::RegisterStatementListener(
			sender,
		))
		.await;

	while let Some(statement) = receiver.next().await {
		match &statement.payload() {
			Statement::Seconded(c) if &c.descriptor.pov_hash == &pov_hash => {
				announce_block(block_hash, statement.encode());

				break;
			}
			_ => {}
		}
	}
}
