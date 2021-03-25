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

#![cfg_attr(not(feature = "std"), no_std)]

use bp_messages::MessageNonce;
use bp_runtime::Chain;
use frame_support::{
	dispatch::Dispatchable,
	parameter_types,
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight, WEIGHT_PER_SECOND},
		DispatchClass, Weight,
	},
	RuntimeDebug,
};
use frame_system::limits;
use sp_core::Hasher as HasherT;
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	MultiSignature, OpaqueExtrinsic, Perbill,
};

// Re-export's to avoid extra substrate dependencies in chain-specific crates.
pub use frame_support::Parameter;
pub use sp_runtime::traits::Convert;

/// Number of extra bytes (excluding size of storage value itself) of storage proof, built at
/// Polkadot-like chain. This mostly depends on number of entries in the storage trie.
/// Some reserve is reserved to account future chain growth.
///
/// To compute this value, we've synced Kusama chain blocks [0; 6545733] to see if there were
/// any significant changes of the storage proof size (NO):
///
/// - at block 3072 the storage proof size overhead was 579 bytes;
/// - at block 2479616 it was 578 bytes;
/// - at block 4118528 it was 711 bytes;
/// - at block 6540800 it was 779 bytes.
///
/// The number of storage entries at the block 6546170 was 351207 and number of trie nodes in
/// the storage proof was 5 (log(16, 351207) ~ 4.6).
///
/// So the assumption is that the storage proof size overhead won't be larger than 1024 in the
/// nearest future. If it'll ever break this barrier, then we'll need to update this constant
/// at next runtime upgrade.
pub const EXTRA_STORAGE_PROOF_SIZE: u32 = 1024;

/// Maximal size (in bytes) of encoded (using `Encode::encode()`) account id.
///
/// All polkadot-like chains are using same crypto.
pub const MAXIMAL_ENCODED_ACCOUNT_ID_SIZE: u32 = 32;

/// All Polkadot-like chains allow normal extrinsics to fill block up to 75%.
///
/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// All Polkadot-like chains allow 2 seconds of compute with a 6 second average block time.
///
/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
pub const MAXIMUM_BLOCK_WEIGHT: Weight = 2 * WEIGHT_PER_SECOND;

/// All Polkadot-like chains assume that an on-initialize consumes 1% of the weight on average,
/// hence a single extrinsic will not be allowed to consume more than `AvailableBlockRatio - 1%`.
///
/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(1);

parameter_types! {
	/// All Polkadot-like chains have maximal block size set to 5MB.
	///
	/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
	pub BlockLength: limits::BlockLength = limits::BlockLength::max_with_normal_ratio(
		5 * 1024 * 1024,
		NORMAL_DISPATCH_RATIO,
	);
	/// All Polkadot-like chains have the same block weights.
	///
	/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
	pub BlockWeights: limits::BlockWeights = limits::BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Operational transactions have an extra reserved space, so that they
			// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT,
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic();
}

/// Get the maximum weight (compute time) that a Normal extrinsic on the Polkadot-like chain can use.
pub fn max_extrinsic_weight() -> Weight {
	BlockWeights::get()
		.get(DispatchClass::Normal)
		.max_extrinsic
		.unwrap_or(Weight::MAX)
}

/// Get the maximum length in bytes that a Normal extrinsic on the Polkadot-like chain requires.
pub fn max_extrinsic_size() -> u32 {
	*BlockLength::get().max.get(DispatchClass::Normal)
}

// TODO [#78] may need to be updated after https://github.com/paritytech/parity-bridges-common/issues/78
/// Maximal number of messages in single delivery transaction.
pub const MAX_MESSAGES_IN_DELIVERY_TRANSACTION: MessageNonce = 128;

/// Maximal number of unrewarded relayer entries at inbound lane.
pub const MAX_UNREWARDED_RELAYER_ENTRIES_AT_INBOUND_LANE: MessageNonce = 128;

// TODO [#438] should be selected keeping in mind:
// finality delay on both chains + reward payout cost + messages throughput.
/// Maximal number of unconfirmed messages at inbound lane.
pub const MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE: MessageNonce = 8192;

/// Block number type used in Polkadot-like chains.
pub type BlockNumber = u32;

/// Hash type used in Polkadot-like chains.
pub type Hash = <BlakeTwo256 as HasherT>::Out;

/// Account Index (a.k.a. nonce).
pub type Index = u32;

/// Hashing type.
pub type Hashing = BlakeTwo256;

/// The type of an object that can produce hashes on Polkadot-like chains.
pub type Hasher = BlakeTwo256;

/// The header type used by Polkadot-like chains.
pub type Header = generic::Header<BlockNumber, Hasher>;

/// Signature type used by Polkadot-like chains.
pub type Signature = MultiSignature;

/// Public key of account on Polkadot-like chains.
pub type AccountPublic = <Signature as Verify>::Signer;

/// Id of account on Polkadot-like chains.
pub type AccountId = <AccountPublic as IdentifyAccount>::AccountId;

/// Index of a transaction on the Polkadot-like chains.
pub type Nonce = u32;

/// Block type of Polkadot-like chains.
pub type Block = generic::Block<Header, OpaqueExtrinsic>;

/// Polkadot-like block signed with a Justification.
pub type SignedBlock = generic::SignedBlock<Block>;

/// The balance of an account on Polkadot-like chain.
pub type Balance = u128;

/// Unchecked Extrinsic type.
pub type UncheckedExtrinsic<Call> = generic::UncheckedExtrinsic<AccountId, Call, Signature, SignedExtensions<Call>>;

/// A type of the data encoded as part of the transaction.
pub type SignedExtra = ((), (), (), sp_runtime::generic::Era, Nonce, (), Balance);

/// Parameters which are part of the payload used to produce transaction signature,
/// but don't end up in the transaction itself (i.e. inherent part of the runtime).
pub type AdditionalSigned = (u32, u32, Hash, Hash, (), (), ());

/// A simplified version of signed extensions meant for producing signed transactions
/// and signed payload in the client code.
#[derive(PartialEq, Eq, Clone, RuntimeDebug)]
pub struct SignedExtensions<Call> {
	encode_payload: SignedExtra,
	additional_signed: AdditionalSigned,
	_data: sp_std::marker::PhantomData<Call>,
}

impl<Call> parity_scale_codec::Encode for SignedExtensions<Call> {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		self.encode_payload.using_encoded(f)
	}
}

impl<Call> parity_scale_codec::Decode for SignedExtensions<Call> {
	fn decode<I: parity_scale_codec::Input>(_input: &mut I) -> Result<Self, parity_scale_codec::Error> {
		unimplemented!("SignedExtensions are never meant to be decoded, they are only used to create transaction");
	}
}

impl<Call> SignedExtensions<Call> {
	pub fn new(
		version: sp_version::RuntimeVersion,
		era: sp_runtime::generic::Era,
		genesis_hash: Hash,
		nonce: Nonce,
		tip: Balance,
	) -> Self {
		Self {
			encode_payload: (
				(),    // spec version
				(),    // tx version
				(),    // genesis
				era,   // era
				nonce, // nonce (compact encoding)
				(),    // Check weight
				tip,   // transaction payment / tip (compact encoding)
			),
			additional_signed: (
				version.spec_version,
				version.transaction_version,
				genesis_hash,
				genesis_hash,
				(),
				(),
				(),
			),
			_data: Default::default(),
		}
	}
}

impl<Call> sp_runtime::traits::SignedExtension for SignedExtensions<Call>
where
	Call: parity_scale_codec::Codec + sp_std::fmt::Debug + Sync + Send + Clone + Eq + PartialEq,
	Call: Dispatchable,
{
	const IDENTIFIER: &'static str = "Not needed.";

	type AccountId = AccountId;
	type Call = Call;
	type AdditionalSigned = AdditionalSigned;
	type Pre = ();

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, frame_support::unsigned::TransactionValidityError> {
		Ok(self.additional_signed)
	}
}

/// Polkadot-like chain.
#[derive(RuntimeDebug)]
pub struct PolkadotLike;

impl Chain for PolkadotLike {
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;
}

/// Convert a 256-bit hash into an AccountId.
pub struct AccountIdConverter;

impl Convert<sp_core::H256, AccountId> for AccountIdConverter {
	fn convert(hash: sp_core::H256) -> AccountId {
		hash.to_fixed_bytes().into()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::codec::Encode;

	#[test]
	fn maximal_encoded_account_id_size_is_correct() {
		let actual_size = AccountId::default().encode().len();
		assert!(
			actual_size <= MAXIMAL_ENCODED_ACCOUNT_ID_SIZE as usize,
			"Actual size of encoded account id for Polkadot-like chains ({}) is larger than expected {}",
			actual_size,
			MAXIMAL_ENCODED_ACCOUNT_ID_SIZE,
		);
	}
}
