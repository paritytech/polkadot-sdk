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

use crate::calls::UtilityCall;

use crate::SimpleRuntimeVersion;
use bp_header_chain::ChainWithGrandpa as ChainWithGrandpaBase;
use bp_messages::ChainWithMessages as ChainWithMessagesBase;
use bp_runtime::{
	Chain as ChainBase, EncodedOrDecodedCall, HashOf, Parachain as ParachainBase, TransactionEra,
	TransactionEraOf, UnderlyingChainProvider,
};
use codec::{Codec, Decode, Encode};
use jsonrpsee::core::{DeserializeOwned, Serialize};
use num_traits::Zero;
use sc_transaction_pool_api::TransactionStatus;
use scale_info::TypeInfo;
use sp_core::{storage::StorageKey, Pair};
use sp_runtime::{
	generic::SignedBlock,
	traits::{Block as BlockT, Member},
	ConsensusEngineId, EncodedJustification,
};
use std::{fmt::Debug, time::Duration};

/// Substrate-based chain from minimal relay-client point of view.
pub trait Chain: ChainBase + Clone {
	/// Chain name.
	const NAME: &'static str;
	/// Name of the runtime API method that is returning best known finalized header number
	/// and hash (as tuple).
	///
	/// Keep in mind that this method is normally provided by the other chain, which is
	/// bridged with this chain.
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str;

	/// Average block interval.
	///
	/// How often blocks are produced on that chain. It's suggested to set this value
	/// to match the block time of the chain.
	const AVERAGE_BLOCK_INTERVAL: Duration;

	/// Block type.
	type SignedBlock: Member + Serialize + DeserializeOwned + BlockWithJustification<Self::Header>;
	/// The aggregated `Call` type.
	type Call: Clone + Codec + Debug + Send + Sync;
}

/// Bridge-supported network definition.
///
/// Used to abstract away CLI commands.
pub trait ChainWithRuntimeVersion: Chain {
	/// Current version of the chain runtime, known to relay.
	///
	/// can be `None` if relay is not going to submit transactions to that chain.
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion>;
}

/// Substrate-based relay chain that supports parachains.
///
/// We assume that the parachains are supported using `runtime_parachains::paras` pallet.
pub trait RelayChain: Chain {
	/// Name of the `runtime_parachains::paras` pallet in the runtime of this chain.
	const PARAS_PALLET_NAME: &'static str;
}

/// Substrate-based chain that is using direct GRANDPA finality from minimal relay-client point of
/// view.
///
/// Keep in mind that parachains are relying on relay chain GRANDPA, so they should not implement
/// this trait.
pub trait ChainWithGrandpa: Chain + ChainWithGrandpaBase {
	/// Name of the runtime API method that is returning the GRANDPA info associated with the
	/// headers accepted by the `submit_finality_proofs` extrinsic in the queried block.
	///
	/// Keep in mind that this method is normally provided by the other chain, which is
	/// bridged with this chain.
	const SYNCED_HEADERS_GRANDPA_INFO_METHOD: &'static str;

	/// The type of the key owner proof used by the grandpa engine.
	type KeyOwnerProof: Decode + TypeInfo + Send;
}

/// Substrate-based parachain from minimal relay-client point of view.
pub trait Parachain: Chain + ParachainBase {}

impl<T> Parachain for T where T: UnderlyingChainProvider + Chain + ParachainBase {}

/// Substrate-based chain with messaging support from minimal relay-client point of view.
pub trait ChainWithMessages: Chain + ChainWithMessagesBase {
	// TODO (https://github.com/paritytech/parity-bridges-common/issues/1692): check all the names
	// after the issue is fixed - all names must be changed

	/// Name of the bridge relayers pallet (used in `construct_runtime` macro call) that is deployed
	/// at some other chain to bridge with this `ChainWithMessages`.
	///
	/// We assume that all chains that are bridging with this `ChainWithMessages` are using
	/// the same name.
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str>;

	/// Name of the `To<ChainWithMessages>OutboundLaneApi::message_details` runtime API method.
	/// The method is provided by the runtime that is bridged with this `ChainWithMessages`.
	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str;

	/// Name of the `From<ChainWithMessages>InboundLaneApi::message_details` runtime API method.
	/// The method is provided by the runtime that is bridged with this `ChainWithMessages`.
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str;
}

/// Call type used by the chain.
pub type CallOf<C> = <C as Chain>::Call;
/// Transaction status of the chain.
pub type TransactionStatusOf<C> = TransactionStatus<HashOf<C>, HashOf<C>>;

/// Substrate-based chain with `AccountData` generic argument of `frame_system::AccountInfo` set to
/// the `pallet_balances::AccountData<Balance>`.
pub trait ChainWithBalances: Chain {
	/// Return runtime storage key for getting `frame_system::AccountInfo` of given account.
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey;
}

/// SCALE-encoded extrinsic.
pub type EncodedExtrinsic = Vec<u8>;

/// Block with justification.
pub trait BlockWithJustification<Header> {
	/// Return block header.
	fn header(&self) -> Header;
	/// Return encoded block extrinsics.
	fn extrinsics(&self) -> Vec<EncodedExtrinsic>;
	/// Return block justification, if known.
	fn justification(&self, engine_id: ConsensusEngineId) -> Option<&EncodedJustification>;
}

/// Transaction before it is signed.
#[derive(Clone, Debug, PartialEq)]
pub struct UnsignedTransaction<C: Chain> {
	/// Runtime call of this transaction.
	pub call: EncodedOrDecodedCall<C::Call>,
	/// Transaction nonce.
	pub nonce: C::Nonce,
	/// Tip included into transaction.
	pub tip: C::Balance,
	/// Transaction era used by the chain.
	pub era: TransactionEraOf<C>,
}

impl<C: Chain> UnsignedTransaction<C> {
	/// Create new unsigned transaction with given call, nonce, era and zero tip.
	pub fn new(call: EncodedOrDecodedCall<C::Call>, nonce: C::Nonce) -> Self {
		Self { call, nonce, era: TransactionEra::Immortal, tip: Zero::zero() }
	}

	/// Convert to the transaction of the other compatible chain.
	pub fn switch_chain<Other>(self) -> UnsignedTransaction<Other>
	where
		Other: Chain<
			Nonce = C::Nonce,
			Balance = C::Balance,
			BlockNumber = C::BlockNumber,
			Hash = C::Hash,
		>,
	{
		UnsignedTransaction {
			call: EncodedOrDecodedCall::Encoded(self.call.into_encoded()),
			nonce: self.nonce,
			tip: self.tip,
			era: self.era,
		}
	}

	/// Set transaction tip.
	#[must_use]
	pub fn tip(mut self, tip: C::Balance) -> Self {
		self.tip = tip;
		self
	}

	/// Set transaction era.
	#[must_use]
	pub fn era(mut self, era: TransactionEraOf<C>) -> Self {
		self.era = era;
		self
	}
}

/// Account key pair used by transactions signing scheme.
pub type AccountKeyPairOf<S> = <S as ChainWithTransactions>::AccountKeyPair;

/// Substrate-based chain transactions signing scheme.
pub trait ChainWithTransactions: Chain {
	/// Type of key pairs used to sign transactions.
	type AccountKeyPair: Pair + Clone + Send + Sync;
	/// Signed transaction.
	type SignedTransaction: Clone + Debug + Codec + Send + 'static;

	/// Create transaction for given runtime call, signed by given account.
	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, crate::Error>
	where
		Self: Sized;
}

/// Sign transaction parameters
pub struct SignParam<C: ChainWithTransactions> {
	/// Version of the runtime specification.
	pub spec_version: u32,
	/// Transaction version
	pub transaction_version: u32,
	/// Hash of the genesis block.
	pub genesis_hash: HashOf<C>,
	/// Signer account
	pub signer: AccountKeyPairOf<C>,
}

impl<Block: BlockT> BlockWithJustification<Block::Header> for SignedBlock<Block> {
	fn header(&self) -> Block::Header {
		self.block.header().clone()
	}

	fn extrinsics(&self) -> Vec<EncodedExtrinsic> {
		self.block.extrinsics().iter().map(Encode::encode).collect()
	}

	fn justification(&self, engine_id: ConsensusEngineId) -> Option<&EncodedJustification> {
		self.justifications.as_ref().and_then(|j| j.get(engine_id))
	}
}

/// Trait that provides functionality defined inside `pallet-utility`
pub trait UtilityPallet<C: Chain> {
	/// Create batch call from given calls vector.
	fn build_batch_call(calls: Vec<C::Call>) -> C::Call;
}

/// Structure that implements `UtilityPalletProvider` based on a full runtime.
pub struct FullRuntimeUtilityPallet<R> {
	_phantom: std::marker::PhantomData<R>,
}

impl<C, R> UtilityPallet<C> for FullRuntimeUtilityPallet<R>
where
	C: Chain,
	R: pallet_utility::Config<RuntimeCall = C::Call>,
	<R as pallet_utility::Config>::RuntimeCall: From<pallet_utility::Call<R>>,
{
	fn build_batch_call(calls: Vec<C::Call>) -> C::Call {
		pallet_utility::Call::batch_all { calls }.into()
	}
}

/// Structure that implements `UtilityPalletProvider` based on a call conversion.
pub struct MockedRuntimeUtilityPallet<Call> {
	_phantom: std::marker::PhantomData<Call>,
}

impl<C, Call> UtilityPallet<C> for MockedRuntimeUtilityPallet<Call>
where
	C: Chain,
	C::Call: From<UtilityCall<C::Call>>,
{
	fn build_batch_call(calls: Vec<C::Call>) -> C::Call {
		UtilityCall::batch_all(calls).into()
	}
}

/// Substrate-based chain that uses `pallet-utility`.
pub trait ChainWithUtilityPallet: Chain {
	/// The utility pallet provider.
	type UtilityPallet: UtilityPallet<Self>;
}
