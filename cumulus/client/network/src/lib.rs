// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use sp_consensus::block_validation::{
	BlockAnnounceValidator as BlockAnnounceValidatorT, Validation,
};
use sp_core::traits::SpawnNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};

use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_node_primitives::{CollationSecondedSignal, Statement};
use polkadot_parachain::primitives::HeadData;
use polkadot_primitives::v1::{
	Block as PBlock, CandidateReceipt, CompactStatement, Hash as PHash, Id as ParaId,
	OccupiedCoreAssumption, SigningContext, UncheckedSigned,
};

use codec::{Decode, DecodeAll, Encode};
use futures::{channel::oneshot, future::FutureExt, Future};

use std::{convert::TryFrom, fmt, marker::PhantomData, pin::Pin, sync::Arc};

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "sync::cumulus";

type BoxedError = Box<dyn std::error::Error + Send>;

#[derive(Debug)]
struct BlockAnnounceError(String);
impl std::error::Error for BlockAnnounceError {}

impl fmt::Display for BlockAnnounceError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

/// The data that we attach to a block announcement.
///
/// This will be used to prove that a header belongs to a block that is probably being backed by
/// the relay chain.
#[derive(Encode, Debug)]
pub struct BlockAnnounceData {
	/// The receipt identifying the candidate.
	receipt: CandidateReceipt,
	/// The seconded statement issued by a relay chain validator that approves the candidate.
	statement: UncheckedSigned<CompactStatement>,
	/// The relay parent that was used as context to sign the [`Self::statement`].
	relay_parent: PHash,
}

impl Decode for BlockAnnounceData {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let receipt = CandidateReceipt::decode(input)?;
		let statement = UncheckedSigned::<CompactStatement>::decode(input)?;

		let relay_parent = match PHash::decode(input) {
			Ok(p) => p,
			// For being backwards compatible, we support missing relay-chain parent.
			Err(_) => receipt.descriptor.relay_parent,
		};

		Ok(Self { receipt, statement, relay_parent })
	}
}

impl BlockAnnounceData {
	/// Validate that the receipt, statement and announced header match.
	///
	/// This will not check the signature, for this you should use [`BlockAnnounceData::check_signature`].
	fn validate(&self, encoded_header: Vec<u8>) -> Result<(), Validation> {
		let candidate_hash = if let CompactStatement::Seconded(h) =
			self.statement.unchecked_payload()
		{
			h
		} else {
			tracing::debug!(target: LOG_TARGET, "`CompactStatement` isn't the candidate variant!",);
			return Err(Validation::Failure { disconnect: true })
		};

		if *candidate_hash != self.receipt.hash() {
			tracing::debug!(
				target: LOG_TARGET,
				"Receipt candidate hash doesn't match candidate hash in statement",
			);
			return Err(Validation::Failure { disconnect: true })
		}

		if HeadData(encoded_header).hash() != self.receipt.descriptor.para_head {
			tracing::debug!(
				target: LOG_TARGET,
				"Receipt para head hash doesn't match the hash of the header in the block announcement",
			);
			return Err(Validation::Failure { disconnect: true })
		}

		Ok(())
	}

	/// Check the signature of the statement.
	///
	/// Returns an `Err(_)` if it failed.
	async fn check_signature<RCInterface>(
		self,
		relay_chain_client: &RCInterface,
	) -> Result<Validation, BlockAnnounceError>
	where
		RCInterface: RelayChainInterface + 'static,
	{
		let validator_index = self.statement.unchecked_validator_index();

		let runtime_api_block_id = BlockId::Hash(self.relay_parent);
		let session_index =
			match relay_chain_client.session_index_for_child(&runtime_api_block_id).await {
				Ok(r) => r,
				Err(e) => return Err(BlockAnnounceError(format!("{:?}", e))),
			};

		let signing_context = SigningContext { parent_hash: self.relay_parent, session_index };

		// Check that the signer is a legit validator.
		let authorities = match relay_chain_client.validators(&runtime_api_block_id).await {
			Ok(r) => r,
			Err(e) => return Err(BlockAnnounceError(format!("{:?}", e))),
		};
		let signer = match authorities.get(validator_index.0 as usize) {
			Some(r) => r,
			None => {
				tracing::debug!(
					target: LOG_TARGET,
					"Block announcement justification signer is a validator index out of bound",
				);

				return Ok(Validation::Failure { disconnect: true })
			},
		};

		// Check statement is correctly signed.
		if self.statement.try_into_checked(&signing_context, &signer).is_err() {
			tracing::debug!(
				target: LOG_TARGET,
				"Block announcement justification signature is invalid.",
			);

			return Ok(Validation::Failure { disconnect: true })
		}

		Ok(Validation::Success { is_new_best: true })
	}
}

impl TryFrom<&'_ CollationSecondedSignal> for BlockAnnounceData {
	type Error = ();

	fn try_from(signal: &CollationSecondedSignal) -> Result<BlockAnnounceData, ()> {
		let receipt = if let Statement::Seconded(receipt) = signal.statement.payload() {
			receipt.to_plain()
		} else {
			return Err(())
		};

		Ok(BlockAnnounceData {
			receipt,
			statement: signal.statement.convert_payload().into(),
			relay_parent: signal.relay_parent,
		})
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
/// It is expected that the attached data is a SCALE encoded [`BlockAnnounceData`]. The
/// statement is checked to be a [`CompactStatement::Candidate`] and that it is signed by an active
/// parachain validator.
///
/// If no justification was provided we check if the block announcement is at the tip of the known
/// chain. If it is at the tip, it is required to provide a justification or otherwise we reject
/// it. However, if the announcement is for a block below the tip the announcement is accepted
/// as it probably comes from a node that is currently syncing the chain.
#[derive(Clone)]
pub struct BlockAnnounceValidator<Block, RCInterface> {
	phantom: PhantomData<Block>,
	relay_chain_interface: RCInterface,
	para_id: ParaId,
}

impl<Block, RCInterface> BlockAnnounceValidator<Block, RCInterface>
where
	RCInterface: Clone,
{
	/// Create a new [`BlockAnnounceValidator`].
	pub fn new(relay_chain_interface: RCInterface, para_id: ParaId) -> Self {
		Self {
			phantom: Default::default(),
			relay_chain_interface: relay_chain_interface.clone(),
			para_id,
		}
	}
}

impl<Block: BlockT, RCInterface> BlockAnnounceValidator<Block, RCInterface>
where
	RCInterface: RelayChainInterface + Clone,
{
	/// Get the included block of the given parachain in the relay chain.
	async fn included_block(
		relay_chain_interface: &RCInterface,
		block_id: &BlockId<PBlock>,
		para_id: ParaId,
	) -> Result<Block::Header, BoxedError> {
		let validation_data = relay_chain_interface
			.persisted_validation_data(block_id, para_id, OccupiedCoreAssumption::TimedOut)
			.await
			.map_err(|e| Box::new(BlockAnnounceError(format!("{:?}", e))) as Box<_>)?
			.ok_or_else(|| {
				Box::new(BlockAnnounceError("Could not find parachain head in relay chain".into()))
					as Box<_>
			})?;
		let para_head =
			Block::Header::decode(&mut &validation_data.parent_head.0[..]).map_err(|e| {
				Box::new(BlockAnnounceError(format!("Failed to decode parachain head: {:?}", e)))
					as Box<_>
			})?;

		Ok(para_head)
	}

	/// Get the backed block hash of the given parachain in the relay chain.
	async fn backed_block_hash(
		relay_chain_interface: &RCInterface,
		block_id: &BlockId<PBlock>,
		para_id: ParaId,
	) -> Result<Option<PHash>, BoxedError> {
		let candidate_receipt = relay_chain_interface
			.candidate_pending_availability(block_id, para_id)
			.await
			.map_err(|e| Box::new(BlockAnnounceError(format!("{:?}", e))) as Box<_>)?;

		Ok(candidate_receipt.map(|cr| cr.descriptor.para_head))
	}

	/// Handle a block announcement with empty data (no statement) attached to it.
	async fn handle_empty_block_announce_data(
		&self,
		header: Block::Header,
	) -> Result<Validation, BoxedError> {
		let relay_chain_interface = self.relay_chain_interface.clone();
		let para_id = self.para_id;

		// Check if block is equal or higher than best (this requires a justification)
		let relay_chain_best_hash = relay_chain_interface
			.best_block_hash()
			.await
			.map_err(|e| Box::new(e) as Box<_>)?;
		let runtime_api_block_id = BlockId::Hash(relay_chain_best_hash);
		let block_number = header.number();

		let best_head =
			Self::included_block(&relay_chain_interface, &runtime_api_block_id, para_id).await?;
		let known_best_number = best_head.number();
		let backed_block = || async {
			Self::backed_block_hash(&relay_chain_interface, &runtime_api_block_id, para_id).await
		};

		if best_head == header {
			tracing::debug!(target: LOG_TARGET, "Announced block matches best block.",);

			Ok(Validation::Success { is_new_best: true })
		} else if Some(HeadData(header.encode()).hash()) == backed_block().await? {
			tracing::debug!(target: LOG_TARGET, "Announced block matches latest backed block.",);

			Ok(Validation::Success { is_new_best: true })
		} else if block_number >= known_best_number {
			tracing::debug!(
					target: LOG_TARGET,
					"Validation failed because a justification is needed if the block at the top of the chain."
				);

			Ok(Validation::Failure { disconnect: false })
		} else {
			Ok(Validation::Success { is_new_best: false })
		}
	}
}

impl<Block: BlockT, RCInterface> BlockAnnounceValidatorT<Block>
	for BlockAnnounceValidator<Block, RCInterface>
where
	RCInterface: RelayChainInterface + Clone + 'static,
{
	fn validate(
		&mut self,
		header: &Block::Header,
		data: &[u8],
	) -> Pin<Box<dyn Future<Output = Result<Validation, BoxedError>> + Send>> {
		let relay_chain_interface = self.relay_chain_interface.clone();
		let mut data = data.to_vec();
		let header = header.clone();
		let header_encoded = header.encode();
		let block_announce_validator = self.clone();

		async move {
			let relay_chain_is_syncing = relay_chain_interface
				.is_major_syncing()
				.await
				.map_err(|e| {
					tracing::error!(target: LOG_TARGET, "Unable to determine sync status. {}", e)
				})
				.unwrap_or(false);

			if relay_chain_is_syncing {
				return Ok(Validation::Success { is_new_best: false })
			}

			if data.is_empty() {
				return block_announce_validator.handle_empty_block_announce_data(header).await
			}

			let block_announce_data = match BlockAnnounceData::decode_all(&mut data) {
				Ok(r) => r,
				Err(err) =>
					return Err(Box::new(BlockAnnounceError(format!(
						"Can not decode the `BlockAnnounceData`: {:?}",
						err
					))) as Box<_>),
			};

			if let Err(e) = block_announce_data.validate(header_encoded) {
				return Ok(e)
			}

			let relay_parent = block_announce_data.receipt.descriptor.relay_parent;

			relay_chain_interface
				.wait_for_block(relay_parent)
				.await
				.map_err(|e| Box::new(BlockAnnounceError(e.to_string())) as Box<_>)?;

			block_announce_data
				.check_signature(&relay_chain_interface)
				.await
				.map_err(|e| Box::new(e) as Box<_>)
		}
		.boxed()
	}
}

/// Wait before announcing a block that a candidate message has been received for this block, then
/// add this message as justification for the block announcement.
///
/// This object will spawn a new task every time the method `wait_to_announce` is called and cancel
/// the previous task running.
pub struct WaitToAnnounce<Block: BlockT> {
	spawner: Arc<dyn SpawnNamed + Send + Sync>,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
}

impl<Block: BlockT> WaitToAnnounce<Block> {
	/// Create the `WaitToAnnounce` object
	pub fn new(
		spawner: Arc<dyn SpawnNamed + Send + Sync>,
		announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	) -> WaitToAnnounce<Block> {
		WaitToAnnounce { spawner, announce_block }
	}

	/// Wait for a candidate message for the block, then announce the block. The candidate
	/// message will be added as justification to the block announcement.
	pub fn wait_to_announce(
		&mut self,
		block_hash: <Block as BlockT>::Hash,
		signed_stmt_recv: oneshot::Receiver<CollationSecondedSignal>,
	) {
		let announce_block = self.announce_block.clone();

		self.spawner.spawn(
			"cumulus-wait-to-announce",
			None,
			async move {
				tracing::debug!(
					target: "cumulus-network",
					"waiting for announce block in a background task...",
				);

				wait_to_announce::<Block>(block_hash, announce_block, signed_stmt_recv).await;

				tracing::debug!(
					target: "cumulus-network",
					"block announcement finished",
				);
			}
			.boxed(),
		);
	}
}

async fn wait_to_announce<Block: BlockT>(
	block_hash: <Block as BlockT>::Hash,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	signed_stmt_recv: oneshot::Receiver<CollationSecondedSignal>,
) {
	let signal = match signed_stmt_recv.await {
		Ok(s) => s,
		Err(_) => {
			tracing::debug!(
				target: "cumulus-network",
				block = ?block_hash,
				"Wait to announce stopped, because sender was dropped.",
			);
			return
		},
	};

	if let Ok(data) = BlockAnnounceData::try_from(&signal) {
		announce_block(block_hash, Some(data.encode()));
	} else {
		tracing::debug!(
			target: "cumulus-network",
			?signal,
			block = ?block_hash,
			"Received invalid statement while waiting to announce block.",
		);
	}
}
