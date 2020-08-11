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

//! Cumulus-specific network implementation.
//!
//! Contains message send between collators and logic to process them.

#[cfg(test)]
mod tests;

use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as ClientError, HeaderBackend};
use sp_consensus::{
	block_validation::{BlockAnnounceValidator, Validation},
	SyncOracle,
};
use sp_core::traits::SpawnNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};

use polkadot_collator::Network as CollatorNetwork;
use polkadot_network::legacy::gossip::{GossipMessage, GossipStatement};
use polkadot_primitives::v0::{Block as PBlock, Hash as PHash, Id as ParaId, ParachainHost};
use polkadot_statement_table::v0::{SignedStatement, Statement};
use polkadot_validation::check_statement;

use cumulus_primitives::HeadData;

use codec::{Decode, Encode};
use futures::{channel::oneshot, future::FutureExt, pin_mut, select, StreamExt};
use log::trace;

use parking_lot::Mutex;
use std::{marker::PhantomData, sync::Arc};

/// Validate that data is a valid justification from a relay-chain validator that the block is a
/// valid parachain-block candidate.
/// Data encoding is just `GossipMessage`, the relay-chain validator candidate statement message is
/// the justification.
///
/// Note: if no justification is provided the annouce is considered valid.
pub struct JustifiedBlockAnnounceValidator<B, P> {
	phantom: PhantomData<B>,
	polkadot_client: Arc<P>,
	para_id: ParaId,
	polkadot_sync_oracle: Box<dyn SyncOracle + Send>,
}

impl<B, P> JustifiedBlockAnnounceValidator<B, P> {
	pub fn new(
		polkadot_client: Arc<P>,
		para_id: ParaId,
		polkadot_sync_oracle: Box<dyn SyncOracle + Send>,
	) -> Self {
		Self {
			phantom: Default::default(),
			polkadot_client,
			para_id,
			polkadot_sync_oracle,
		}
	}
}

impl<B: BlockT, P> BlockAnnounceValidator<B> for JustifiedBlockAnnounceValidator<B, P>
where
	P: ProvideRuntimeApi<PBlock> + HeaderBackend<PBlock>,
	P::Api: ParachainHost<PBlock>,
{
	fn validate(
		&mut self,
		header: &B::Header,
		mut data: &[u8],
	) -> Result<Validation, Box<dyn std::error::Error + Send>> {
		if self.polkadot_sync_oracle.is_major_syncing() {
			return Ok(Validation::Success { is_new_best: false });
		}

		let runtime_api = self.polkadot_client.runtime_api();
		let polkadot_info = self.polkadot_client.info();

		if data.is_empty() {
			// Check if block is equal or higher than best (this requires a justification)
			let runtime_api_block_id = BlockId::Hash(polkadot_info.best_hash);
			let block_number = header.number();

			let local_validation_data = runtime_api
				.local_validation_data(&runtime_api_block_id, self.para_id)
				.map_err(|e| Box::new(ClientError::Msg(format!("{:?}", e))) as Box<_>)?
				.ok_or_else(|| {
					Box::new(ClientError::Msg(
						"Could not find parachain head in relay chain".into(),
					)) as Box<_>
				})?;
			let parent_head = HeadData::<B>::decode(&mut &local_validation_data.parent_head.0[..])
				.map_err(|e| {
					Box::new(ClientError::Msg(format!(
						"Failed to decode parachain head: {:?}",
						e
					))) as Box<_>
				})?;
			let known_best_number = parent_head.header.number();

			return Ok(if block_number >= known_best_number {
				trace!(
					target: "cumulus-network",
					"validation failed because a justification is needed if the block at the top of the chain."
				);

				Validation::Failure
			} else {
				Validation::Success { is_new_best: false }
			});
		}

		// Check data is a gossip message.
		let gossip_message = GossipMessage::decode(&mut data).map_err(|_| {
			Box::new(ClientError::BadJustification(
				"cannot decode block announced justification, must be a gossip message".to_string(),
			)) as Box<_>
		})?;

		// Check message is a gossip statement.
		let gossip_statement = match gossip_message {
			GossipMessage::Statement(gossip_statement) => gossip_statement,
			_ => {
				return Err(Box::new(ClientError::BadJustification(
					"block announced justification statement must be a gossip statement"
						.to_string(),
				)) as Box<_>)
			}
		};

		let GossipStatement {
			relay_chain_leaf,
			signed_statement: SignedStatement {
				statement,
				signature,
				sender,
			},
		} = gossip_statement;

		// Check that the relay chain parent of the block is the relay chain head
		let best_number = polkadot_info.best_number;

		match self.polkadot_client.number(relay_chain_leaf) {
			Err(err) => {
				return Err(Box::new(ClientError::Backend(format!(
					"could not find block number for {}: {}",
					relay_chain_leaf, err,
				))));
			}
			Ok(Some(x)) if x == best_number => {}
			Ok(None) => {
				return Err(Box::new(ClientError::UnknownBlock(
					relay_chain_leaf.to_string(),
				)));
			}
			Ok(Some(_)) => {
				trace!(
					target: "cumulus-network",
					"validation failed because the relay chain parent ({}) is not the relay chain \
					head ({})",
					relay_chain_leaf, best_number,
				);

				return Ok(Validation::Failure);
			}
		}

		let runtime_api_block_id = BlockId::Hash(relay_chain_leaf);
		let signing_context = runtime_api
			.signing_context(&runtime_api_block_id)
			.map_err(|e| Box::new(ClientError::Msg(format!("{:?}", e))) as Box<_>)?;

		// Check that the signer is a legit validator.
		let authorities = runtime_api
			.validators(&runtime_api_block_id)
			.map_err(|e| Box::new(ClientError::Msg(format!("{:?}", e))) as Box<_>)?;
		let signer = authorities.get(sender as usize).ok_or_else(|| {
			Box::new(ClientError::BadJustification(
				"block accounced justification signer is a validator index out of bound"
					.to_string(),
			)) as Box<_>
		})?;

		// Check statement is correctly signed.
		if !check_statement(&statement, &signature, signer.clone(), &signing_context) {
			return Err(Box::new(ClientError::BadJustification(
				"block announced justification signature is invalid".to_string(),
			)) as Box<_>);
		}

		// Check statement is a candidate statement.
		let candidate_receipt = match statement {
			Statement::Candidate(candidate_receipt) => candidate_receipt,
			_ => {
				return Err(Box::new(ClientError::BadJustification(
					"block announced justification statement must be a candidate statement"
						.to_string(),
				)) as Box<_>)
			}
		};

		// Check the header in the candidate_receipt match header given header.
		if header.encode() != candidate_receipt.head_data.0 {
			return Err(Box::new(ClientError::BadJustification(
				"block announced header does not match the one justified".to_string(),
			)) as Box<_>);
		}

		Ok(Validation::Success { is_new_best: true })
	}
}

/// A `BlockAnnounceValidator` that will be able to validate data when its internal
/// `BlockAnnounceValidator` is set.
pub struct DelayedBlockAnnounceValidator<B: BlockT>(
	Arc<Mutex<Option<Box<dyn BlockAnnounceValidator<B> + Send>>>>,
);

impl<B: BlockT> DelayedBlockAnnounceValidator<B> {
	pub fn new() -> DelayedBlockAnnounceValidator<B> {
		DelayedBlockAnnounceValidator(Arc::new(Mutex::new(None)))
	}

	pub fn set(&self, validator: Box<dyn BlockAnnounceValidator<B> + Send>) {
		*self.0.lock() = Some(validator);
	}
}

impl<B: BlockT> Clone for DelayedBlockAnnounceValidator<B> {
	fn clone(&self) -> DelayedBlockAnnounceValidator<B> {
		DelayedBlockAnnounceValidator(self.0.clone())
	}
}

impl<B: BlockT> BlockAnnounceValidator<B> for DelayedBlockAnnounceValidator<B> {
	fn validate(
		&mut self,
		header: &B::Header,
		data: &[u8],
	) -> Result<Validation, Box<dyn std::error::Error + Send>> {
		match self.0.lock().as_mut() {
			Some(validator) => validator.validate(header, data),
			None => {
				log::warn!("BlockAnnounce validator not yet set, rejecting block announcement");
				Ok(Validation::Failure)
			}
		}
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
	collator_network: Arc<dyn CollatorNetwork>,
	current_trigger: oneshot::Sender<()>,
}

impl<Block: BlockT> WaitToAnnounce<Block> {
	/// Create the `WaitToAnnounce` object
	pub fn new(
		spawner: Arc<dyn SpawnNamed + Send + Sync>,
		announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
		collator_network: Arc<dyn CollatorNetwork>,
	) -> WaitToAnnounce<Block> {
		let (tx, _rx) = oneshot::channel();

		WaitToAnnounce {
			spawner,
			announce_block,
			collator_network,
			current_trigger: tx,
		}
	}

	/// Wait for a candidate message for the block, then announce the block. The candidate
	/// message will be added as justification to the block announcement.
	pub fn wait_to_announce(
		&mut self,
		hash: <Block as BlockT>::Hash,
		relay_chain_leaf: PHash,
		head_data: Vec<u8>,
	) {
		let (tx, rx) = oneshot::channel();
		let announce_block = self.announce_block.clone();
		let collator_network = self.collator_network.clone();

		self.current_trigger = tx;

		self.spawner.spawn(
			"cumulus-wait-to-announce",
			async move {
				let t1 = wait_to_announce::<Block>(
					hash,
					relay_chain_leaf,
					announce_block,
					collator_network,
					&head_data,
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
	hash: <Block as BlockT>::Hash,
	relay_chain_leaf: PHash,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	collator_network: Arc<dyn CollatorNetwork>,
	head_data: &Vec<u8>,
) {
	let mut checked_statements = collator_network.checked_statements(relay_chain_leaf);

	while let Some(statement) = checked_statements.next().await {
		match &statement.statement {
			Statement::Candidate(c) if &c.head_data.0 == head_data => {
				let gossip_message: GossipMessage = GossipStatement {
					relay_chain_leaf,
					signed_statement: statement,
				}
				.into();

				announce_block(hash, gossip_message.encode());

				break;
			}
			_ => {}
		}
	}
}
