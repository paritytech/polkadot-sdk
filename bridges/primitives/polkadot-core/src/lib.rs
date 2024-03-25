// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Primitives of the Polkadot-like chains.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use bp_messages::MessageNonce;
use bp_runtime::{
	self,
	extensions::{
		ChargeTransactionPayment, CheckEra, CheckGenesis, CheckNonZeroSender, CheckNonce,
		CheckSpecVersion, CheckTxVersion, CheckWeight, GenericSignedExtension,
		SignedExtensionSchema,
	},
	EncodedOrDecodedCall, StorageMapKeyProvider, TransactionEra,
};
use frame_support::{
	dispatch::DispatchClass,
	parameter_types,
	weights::{
		constants::{BlockExecutionWeight, WEIGHT_REF_TIME_PER_SECOND},
		Weight,
	},
	Blake2_128Concat,
};
use frame_system::limits;
use sp_core::{storage::StorageKey, Hasher as HasherT};
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	MultiAddress, MultiSignature, OpaqueExtrinsic,
};
use sp_std::prelude::Vec;

// Re-export's to avoid extra substrate dependencies in chain-specific crates.
pub use frame_support::{weights::constants::ExtrinsicBaseWeight, Parameter};
pub use sp_runtime::{traits::Convert, Perbill};

pub mod parachains;

/// Maximal number of GRANDPA authorities at Polkadot-like chains.
///
/// Ideally, we would set it to the value of `MaxAuthorities` constant from bridged runtime
/// configurations. But right now it is set to the `100_000`, which makes PoV size for
/// our bridge hub parachains huge. So let's stick to the real-world value here.
///
/// Right now both Kusama and Polkadot aim to have around 1000 validators. Let's be safe here and
/// take a bit more here.
pub const MAX_AUTHORITIES_COUNT: u32 = 1_256;

/// Reasonable number of headers in the `votes_ancestries` on Polkadot-like chains.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
///
/// This value comes from recent (December, 2023) Kusama and Polkadot headers. There are no
/// justifications with any additional headers in votes ancestry, so reasonable headers may
/// be set to zero. But we assume that there may be small GRANDPA lags, so we're leaving some
/// reserve here.
pub const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 2;

/// Average header size in `votes_ancestries` field of justification on Polkadot-like
/// chains.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
///
/// This value comes from recent (December, 2023) Kusama headers. Most of headers are `327` bytes
/// there, but let's have some reserve and make it 1024.
pub const AVERAGE_HEADER_SIZE: u32 = 1024;

/// Approximate maximal header size on Polkadot-like chains.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
///
/// This value comes from recent (December, 2023) Kusama headers. Maximal header is a mandatory
/// header. In its SCALE-encoded form it is `113407` bytes. Let's have some reserve here.
pub const MAX_MANDATORY_HEADER_SIZE: u32 = 120 * 1024;

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

/// All Polkadot-like chains allow normal extrinsics to fill block up to 75 percent.
///
/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// All Polkadot-like chains allow 2 seconds of compute with a 6-second average block time.
///
/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
pub const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2), u64::MAX);

/// All Polkadot-like chains assume that an on-initialize consumes 1 percent of the weight on
/// average, hence a single extrinsic will not be allowed to consume more than
/// `AvailableBlockRatio - 1 percent`.
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

// TODO [#78] may need to be updated after https://github.com/paritytech/parity-bridges-common/issues/78
/// Maximal number of messages in single delivery transaction.
pub const MAX_MESSAGES_IN_DELIVERY_TRANSACTION: MessageNonce = 128;

/// Maximal number of bytes, included in the signed Polkadot-like transaction apart from the encoded
/// call itself.
///
/// Can be computed by subtracting encoded call size from raw transaction size.
pub const TX_EXTRA_BYTES: u32 = 256;

/// Re-export `time_units` to make usage easier.
pub use time_units::*;

/// Human readable time units defined in terms of number of blocks.
pub mod time_units {
	use super::BlockNumber;

	/// Milliseconds between Polkadot-like chain blocks.
	pub const MILLISECS_PER_BLOCK: u64 = 6000;
	/// Slot duration in Polkadot-like chain consensus algorithms.
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	/// A minute, expressed in Polkadot-like chain blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	/// A hour, expressed in Polkadot-like chain blocks.
	pub const HOURS: BlockNumber = MINUTES * 60;
	/// A day, expressed in Polkadot-like chain blocks.
	pub const DAYS: BlockNumber = HOURS * 24;
}

/// Block number type used in Polkadot-like chains.
pub type BlockNumber = u32;

/// Hash type used in Polkadot-like chains.
pub type Hash = <BlakeTwo256 as HasherT>::Out;

/// Hashing type.
pub type Hashing = BlakeTwo256;

/// The type of object that can produce hashes on Polkadot-like chains.
pub type Hasher = BlakeTwo256;

/// The header type used by Polkadot-like chains.
pub type Header = generic::Header<BlockNumber, Hasher>;

/// Signature type used by Polkadot-like chains.
pub type Signature = MultiSignature;

/// Public key of account on Polkadot-like chains.
pub type AccountPublic = <Signature as Verify>::Signer;

/// Id of account on Polkadot-like chains.
pub type AccountId = <AccountPublic as IdentifyAccount>::AccountId;

/// Address of account on Polkadot-like chains.
pub type AccountAddress = MultiAddress<AccountId, ()>;

/// Nonce of a transaction on the Polkadot-like chains.
pub type Nonce = u32;

/// Block type of Polkadot-like chains.
pub type Block = generic::Block<Header, OpaqueExtrinsic>;

/// Polkadot-like block signed with a Justification.
pub type SignedBlock = generic::SignedBlock<Block>;

/// The balance of an account on Polkadot-like chain.
pub type Balance = u128;

/// Unchecked Extrinsic type.
pub type UncheckedExtrinsic<Call, SignedExt> =
	generic::UncheckedExtrinsic<AccountAddress, EncodedOrDecodedCall<Call>, Signature, SignedExt>;

/// Account address, used by the Polkadot-like chain.
pub type Address = MultiAddress<AccountId, ()>;

/// Returns maximal extrinsic size on all Polkadot-like chains.
pub fn max_extrinsic_size() -> u32 {
	*BlockLength::get().max.get(DispatchClass::Normal)
}

/// Returns maximal extrinsic weight on all Polkadot-like chains.
pub fn max_extrinsic_weight() -> Weight {
	BlockWeights::get()
		.get(DispatchClass::Normal)
		.max_extrinsic
		.unwrap_or(Weight::MAX)
}

/// Provides a storage key for account data.
///
/// We need to use this approach when we don't have access to the runtime.
/// The equivalent command to invoke in case full `Runtime` is known is this:
/// `let key = frame_system::Account::<Runtime>::storage_map_final_key(&account_id);`
pub struct AccountInfoStorageMapKeyProvider;

impl StorageMapKeyProvider for AccountInfoStorageMapKeyProvider {
	const MAP_NAME: &'static str = "Account";
	type Hasher = Blake2_128Concat;
	type Key = AccountId;
	// This should actually be `AccountInfo`, but we don't use this property in order to decode the
	// data. So we use `Vec<u8>` as if we would work with encoded data.
	type Value = Vec<u8>;
}

impl AccountInfoStorageMapKeyProvider {
	/// Name of the system pallet.
	const PALLET_NAME: &'static str = "System";

	/// Return storage key for given account data.
	pub fn final_key(id: &AccountId) -> StorageKey {
		<Self as StorageMapKeyProvider>::final_key(Self::PALLET_NAME, id)
	}
}

/// Extra signed extension data that is used by most chains.
pub type CommonSignedExtra = (
	CheckNonZeroSender,
	CheckSpecVersion,
	CheckTxVersion,
	CheckGenesis<Hash>,
	CheckEra<Hash>,
	CheckNonce<Nonce>,
	CheckWeight,
	ChargeTransactionPayment<Balance>,
);

/// Extra signed extension data that starts with `CommonSignedExtra`.
pub type SuffixedCommonSignedExtension<Suffix> =
	GenericSignedExtension<(CommonSignedExtra, Suffix)>;

/// Helper trait to define some extra methods on `SuffixedCommonSignedExtension`.
pub trait SuffixedCommonSignedExtensionExt<Suffix: SignedExtensionSchema> {
	/// Create signed extension from its components.
	fn from_params(
		spec_version: u32,
		transaction_version: u32,
		era: TransactionEra<BlockNumber, Hash>,
		genesis_hash: Hash,
		nonce: Nonce,
		tip: Balance,
		extra: (Suffix::Payload, Suffix::AdditionalSigned),
	) -> Self;

	/// Return transaction nonce.
	fn nonce(&self) -> Nonce;

	/// Return transaction tip.
	fn tip(&self) -> Balance;
}

impl<Suffix> SuffixedCommonSignedExtensionExt<Suffix> for SuffixedCommonSignedExtension<Suffix>
where
	Suffix: SignedExtensionSchema,
{
	fn from_params(
		spec_version: u32,
		transaction_version: u32,
		era: TransactionEra<BlockNumber, Hash>,
		genesis_hash: Hash,
		nonce: Nonce,
		tip: Balance,
		extra: (Suffix::Payload, Suffix::AdditionalSigned),
	) -> Self {
		GenericSignedExtension::new(
			(
				(
					(),              // non-zero sender
					(),              // spec version
					(),              // tx version
					(),              // genesis
					era.frame_era(), // era
					nonce.into(),    // nonce (compact encoding)
					(),              // Check weight
					tip.into(),      // transaction payment / tip (compact encoding)
				),
				extra.0,
			),
			Some((
				(
					(),
					spec_version,
					transaction_version,
					genesis_hash,
					era.signed_payload(genesis_hash),
					(),
					(),
					(),
				),
				extra.1,
			)),
		)
	}

	fn nonce(&self) -> Nonce {
		let common_payload = self.payload.0;
		common_payload.5 .0
	}

	fn tip(&self) -> Balance {
		let common_payload = self.payload.0;
		common_payload.7 .0
	}
}

/// Signed extension that is used by most chains.
pub type CommonSignedExtension = SuffixedCommonSignedExtension<()>;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_generate_storage_key() {
		let acc = [
			1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
			25, 26, 27, 28, 29, 30, 31, 32,
		]
		.into();
		let key = AccountInfoStorageMapKeyProvider::final_key(&acc);
		assert_eq!(hex::encode(key), "26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da92dccd599abfe1920a1cff8a7358231430102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20");
	}
}
