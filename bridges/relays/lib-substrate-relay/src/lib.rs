// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! The library of substrate relay. contains some public codes to provide to substrate relay.

#![warn(missing_docs)]

use relay_substrate_client::{Chain, ChainWithUtilityPallet, UtilityPallet};

use std::marker::PhantomData;

// to avoid `finality_relay` dependency in other crates
pub use finality_relay::HeadersToRelay;

pub mod cli;
pub mod equivocation;
pub mod error;
pub mod finality;
pub mod finality_base;
pub mod messages;
pub mod on_demand;
pub mod parachains;

/// Transaction creation parameters.
#[derive(Clone, Debug)]
pub struct TransactionParams<TS> {
	/// Transactions author.
	pub signer: TS,
	/// Transactions mortality.
	pub mortality: Option<u32>,
}

/// Tagged relay account, which balance may be exposed as metrics by the relay.
#[derive(Clone, Debug)]
pub enum TaggedAccount<AccountId> {
	/// Account, used to sign message (also headers and parachains) relay transactions from given
	/// bridged chain.
	Messages {
		/// Account id.
		id: AccountId,
		/// Name of the bridged chain, which sends us messages or delivery confirmations.
		bridged_chain: String,
	},
}

impl<AccountId> TaggedAccount<AccountId> {
	/// Returns reference to the account id.
	pub fn id(&self) -> &AccountId {
		match *self {
			TaggedAccount::Messages { ref id, .. } => id,
		}
	}

	/// Returns stringified account tag.
	pub fn tag(&self) -> String {
		match *self {
			TaggedAccount::Messages { ref bridged_chain, .. } => {
				format!("{bridged_chain}Messages")
			},
		}
	}
}

/// Batch call builder.
pub trait BatchCallBuilder<Call>: Clone + Send + Sync {
	/// Create batch call from given calls vector.
	fn build_batch_call(&self, _calls: Vec<Call>) -> Call;
}

/// Batch call builder constructor.
pub trait BatchCallBuilderConstructor<Call>: Clone {
	/// Call builder, used by this constructor.
	type CallBuilder: BatchCallBuilder<Call>;
	/// Create a new instance of a batch call builder.
	fn new_builder() -> Option<Self::CallBuilder>;
}

/// Batch call builder based on `pallet-utility`.
#[derive(Clone)]
pub struct UtilityPalletBatchCallBuilder<C: Chain>(PhantomData<C>);

impl<C: Chain> BatchCallBuilder<C::Call> for UtilityPalletBatchCallBuilder<C>
where
	C: ChainWithUtilityPallet,
{
	fn build_batch_call(&self, calls: Vec<C::Call>) -> C::Call {
		C::UtilityPallet::build_batch_call(calls)
	}
}

impl<C: Chain> BatchCallBuilderConstructor<C::Call> for UtilityPalletBatchCallBuilder<C>
where
	C: ChainWithUtilityPallet,
{
	type CallBuilder = Self;

	fn new_builder() -> Option<Self::CallBuilder> {
		Some(Self(Default::default()))
	}
}

// A `BatchCallBuilderConstructor` that always returns `None`.
impl<Call> BatchCallBuilderConstructor<Call> for () {
	type CallBuilder = ();
	fn new_builder() -> Option<Self::CallBuilder> {
		None
	}
}

// Dummy `BatchCallBuilder` implementation that must never be used outside
// of the `impl BatchCallBuilderConstructor for ()` code.
impl<Call> BatchCallBuilder<Call> for () {
	fn build_batch_call(&self, _calls: Vec<Call>) -> Call {
		unreachable!("never called, because ()::new_builder() returns None; qed")
	}
}

/// Module for handling storage proofs compatibility.
pub mod proofs {
	use bp_messages::{LaneId, MessageNonce};
	use bp_runtime::{HashOf, HasherOf, RawStorageProof, UnverifiedStorageProof};
	use frame_support::pallet_prelude::{Decode, Encode, TypeInfo};
	use relay_substrate_client::Chain;
	use sp_core::storage::StorageKey;
	use sp_trie::StorageProof;

	/// Represents generic proof, that can be converted to different storage proof types.
	#[derive(Clone, Debug)]
	pub struct Proof<SourceChain: Chain> {
		/// Storage proof itself.
		pub storage_proof: StorageProof,
		/// Storage proof keys.
		pub storage_keys: Vec<StorageKey>,
		/// State root
		pub state_root: HashOf<SourceChain>,
	}

	impl<SourceChain: Chain> From<(StorageProof, Vec<StorageKey>, HashOf<SourceChain>)>
		for Proof<SourceChain>
	{
		fn from(value: (StorageProof, Vec<StorageKey>, HashOf<SourceChain>)) -> Self {
			Self { storage_proof: value.0, storage_keys: value.1, state_root: value.2 }
		}
	}

	impl<SourceChain: Chain> TryInto<RawStorageProof> for Proof<SourceChain> {
		type Error = ();

		fn try_into(self) -> Result<RawStorageProof, Self::Error> {
			Ok(self.storage_proof.into_iter_nodes().collect())
		}
	}

	impl<SourceChain: Chain> TryInto<UnverifiedStorageProof> for Proof<SourceChain> {
		type Error = ();

		fn try_into(self) -> Result<UnverifiedStorageProof, Self::Error> {
			let Self { storage_proof, storage_keys, state_root } = self;

			UnverifiedStorageProof::try_new::<HasherOf<SourceChain>>(
				storage_proof,
				state_root,
				storage_keys,
			)
			.map_err(|e| {
				log::error!(
					target: "bridge",
					"Failed to create `UnverifiedStorageProof` with error: {e:?}"
				);
				()
			})
		}
	}

	/// Stub that represents `bp_messages::target_chain::FromBridgedChainMessagesProof` but with
	/// a generic storage proof, allowing the `ReceiveMessagesProofCallBuilder` implementation to
	/// decide what kind of storage proof to use.
	#[derive(Clone, Debug)]
	pub struct FromBridgedChainMessagesProof<C: Chain> {
		/// Hash of the finalized bridged header the proof is for.
		pub bridged_header_hash: HashOf<C>,
		/// A storage trie proof of messages being delivered.
		pub storage_proof: Proof<C>,
		/// Messages in this proof are sent over this lane.
		pub lane: LaneId,
		/// Nonce of the first message being delivered.
		pub nonces_start: MessageNonce,
		/// Nonce of the last message being delivered.
		pub nonces_end: MessageNonce,
	}

	impl<C: Chain> TryFrom<FromBridgedChainMessagesProof<C>>
		for bp_messages::target_chain::FromBridgedChainMessagesProof<HashOf<C>>
	{
		type Error = ();

		fn try_from(value: FromBridgedChainMessagesProof<C>) -> Result<Self, Self::Error> {
			Ok(bp_messages::target_chain::FromBridgedChainMessagesProof {
				bridged_header_hash: value.bridged_header_hash,
				storage_proof: value.storage_proof.try_into()?,
				lane: value.lane,
				nonces_start: value.nonces_start,
				nonces_end: value.nonces_end,
			})
		}
	}

	/// Stub that represents `bp_messages::source_chain::FromBridgedChainMessagesDeliveryProof` but
	/// with a generic storage proof, allowing the `ReceiveMessagesDeliveryProofCallBuilder`
	/// implementation to decide what kind of storage proof to use.
	#[derive(Clone, Debug)]
	pub struct FromBridgedChainMessagesDeliveryProof<C: Chain> {
		/// Hash of the bridge header the proof is for.
		pub bridged_header_hash: HashOf<C>,
		/// Storage trie proof generated for [`Self::bridged_header_hash`].
		pub storage_proof: Proof<C>,
		/// Lane id of which messages were delivered and the proof is for.
		pub lane: LaneId,
	}

	impl<C: Chain> TryFrom<FromBridgedChainMessagesDeliveryProof<C>>
		for bp_messages::source_chain::FromBridgedChainMessagesDeliveryProof<HashOf<C>>
	{
		type Error = ();

		fn try_from(value: FromBridgedChainMessagesDeliveryProof<C>) -> Result<Self, Self::Error> {
			Ok(bp_messages::source_chain::FromBridgedChainMessagesDeliveryProof {
				bridged_header_hash: value.bridged_header_hash,
				storage_proof: value.storage_proof.try_into()?,
				lane: value.lane,
			})
		}
	}
}
