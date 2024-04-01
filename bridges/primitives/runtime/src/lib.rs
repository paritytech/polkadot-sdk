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

//! Primitives that may be used at (bridges) runtime level.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{
	pallet_prelude::DispatchResult, weights::Weight, PalletError, StorageHasher, StorageValue,
};
use frame_system::RawOrigin;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::storage::StorageKey;
use sp_runtime::{
	traits::{BadOrigin, Header as HeaderT, UniqueSaturatedInto},
	RuntimeDebug,
};
use sp_std::{convert::TryFrom, fmt::Debug, ops::RangeInclusive, vec, vec::Vec};

pub use chain::{
	AccountIdOf, AccountPublicOf, BalanceOf, BlockNumberOf, Chain, EncodedOrDecodedCall, HashOf,
	HasherOf, HeaderOf, NonceOf, Parachain, ParachainIdOf, SignatureOf, TransactionEraOf,
	UnderlyingChainOf, UnderlyingChainProvider,
};
pub use frame_support::storage::storage_prefix as storage_value_final_key;
use num_traits::{CheckedAdd, CheckedSub, One, SaturatingAdd, Zero};
pub use storage_proof::{
	record_all_keys as record_all_trie_keys, Error as StorageProofError,
	ProofSize as StorageProofSize, RawStorageProof, StorageProofChecker,
};
pub use storage_types::BoundedStorageValue;

#[cfg(feature = "std")]
pub use storage_proof::craft_valid_storage_proof;

pub mod extensions;
pub mod messages;

mod chain;
mod storage_proof;
mod storage_types;

// Re-export macro to avoid include paste dependency everywhere
pub use sp_runtime::paste;

/// Use this when something must be shared among all instances.
pub const NO_INSTANCE_ID: ChainId = [0, 0, 0, 0];

/// Generic header Id.
#[derive(
	RuntimeDebug,
	Default,
	Clone,
	Encode,
	Decode,
	Copy,
	Eq,
	Hash,
	MaxEncodedLen,
	PartialEq,
	PartialOrd,
	Ord,
	TypeInfo,
)]
pub struct HeaderId<Hash, Number>(pub Number, pub Hash);

impl<Hash: Copy, Number: Copy> HeaderId<Hash, Number> {
	/// Return header number.
	pub fn number(&self) -> Number {
		self.0
	}

	/// Return header hash.
	pub fn hash(&self) -> Hash {
		self.1
	}
}

/// Header id used by the chain.
pub type HeaderIdOf<C> = HeaderId<HashOf<C>, BlockNumberOf<C>>;

/// Generic header id provider.
pub trait HeaderIdProvider<Header: HeaderT> {
	/// Get the header id.
	fn id(&self) -> HeaderId<Header::Hash, Header::Number>;

	/// Get the header id for the parent block.
	fn parent_id(&self) -> Option<HeaderId<Header::Hash, Header::Number>>;
}

impl<Header: HeaderT> HeaderIdProvider<Header> for Header {
	fn id(&self) -> HeaderId<Header::Hash, Header::Number> {
		HeaderId(*self.number(), self.hash())
	}

	fn parent_id(&self) -> Option<HeaderId<Header::Hash, Header::Number>> {
		self.number()
			.checked_sub(&One::one())
			.map(|parent_number| HeaderId(parent_number, *self.parent_hash()))
	}
}

/// Unique identifier of the chain.
///
/// In addition to its main function (identifying the chain), this type may also be used to
/// identify module instance. We have a bunch of pallets that may be used in different bridges. E.g.
/// messages pallet may be deployed twice in the same runtime to bridge ThisChain with Chain1 and
/// Chain2. Sometimes we need to be able to identify deployed instance dynamically. This type may be
/// used for that.
pub type ChainId = [u8; 4];

/// Anything that has size.
pub trait Size {
	/// Return size of this object (in bytes).
	fn size(&self) -> u32;
}

impl Size for () {
	fn size(&self) -> u32 {
		0
	}
}

impl Size for Vec<u8> {
	fn size(&self) -> u32 {
		self.len() as _
	}
}

/// Pre-computed size.
pub struct PreComputedSize(pub usize);

impl Size for PreComputedSize {
	fn size(&self) -> u32 {
		u32::try_from(self.0).unwrap_or(u32::MAX)
	}
}

/// Era of specific transaction.
#[derive(RuntimeDebug, Clone, Copy, PartialEq)]
pub enum TransactionEra<BlockNumber, BlockHash> {
	/// Transaction is immortal.
	Immortal,
	/// Transaction is valid for a given number of blocks, starting from given block.
	Mortal(HeaderId<BlockHash, BlockNumber>, u32),
}

impl<BlockNumber: Copy + UniqueSaturatedInto<u64>, BlockHash: Copy>
	TransactionEra<BlockNumber, BlockHash>
{
	/// Prepare transaction era, based on mortality period and current best block number.
	pub fn new(
		best_block_id: HeaderId<BlockHash, BlockNumber>,
		mortality_period: Option<u32>,
	) -> Self {
		mortality_period
			.map(|mortality_period| TransactionEra::Mortal(best_block_id, mortality_period))
			.unwrap_or(TransactionEra::Immortal)
	}

	/// Create new immortal transaction era.
	pub fn immortal() -> Self {
		TransactionEra::Immortal
	}

	/// Returns mortality period if transaction is mortal.
	pub fn mortality_period(&self) -> Option<u32> {
		match *self {
			TransactionEra::Immortal => None,
			TransactionEra::Mortal(_, period) => Some(period),
		}
	}

	/// Returns era that is used by FRAME-based runtimes.
	pub fn frame_era(&self) -> sp_runtime::generic::Era {
		match *self {
			TransactionEra::Immortal => sp_runtime::generic::Era::immortal(),
			// `unique_saturated_into` is fine here - mortality `u64::MAX` is not something we
			// expect to see on any chain
			TransactionEra::Mortal(header_id, period) =>
				sp_runtime::generic::Era::mortal(period as _, header_id.0.unique_saturated_into()),
		}
	}

	/// Returns header hash that needs to be included in the signature payload.
	pub fn signed_payload(&self, genesis_hash: BlockHash) -> BlockHash {
		match *self {
			TransactionEra::Immortal => genesis_hash,
			TransactionEra::Mortal(header_id, _) => header_id.1,
		}
	}
}

/// This is a copy of the
/// `frame_support::storage::generator::StorageMap::storage_map_final_key` for maps based
/// on selected hasher.
///
/// We're using it because to call `storage_map_final_key` directly, we need access to the runtime
/// and pallet instance, which (sometimes) is impossible.
pub fn storage_map_final_key<H: StorageHasher>(
	pallet_prefix: &str,
	map_name: &str,
	key: &[u8],
) -> StorageKey {
	let key_hashed = H::hash(key);
	let pallet_prefix_hashed = frame_support::Twox128::hash(pallet_prefix.as_bytes());
	let storage_prefix_hashed = frame_support::Twox128::hash(map_name.as_bytes());

	let mut final_key = Vec::with_capacity(
		pallet_prefix_hashed.len() + storage_prefix_hashed.len() + key_hashed.as_ref().len(),
	);

	final_key.extend_from_slice(&pallet_prefix_hashed[..]);
	final_key.extend_from_slice(&storage_prefix_hashed[..]);
	final_key.extend_from_slice(key_hashed.as_ref());

	StorageKey(final_key)
}

/// This is how a storage key of storage value is computed.
///
/// Copied from `frame_support::storage::storage_prefix`.
pub fn storage_value_key(pallet_prefix: &str, value_name: &str) -> StorageKey {
	let pallet_hash = sp_io::hashing::twox_128(pallet_prefix.as_bytes());
	let storage_hash = sp_io::hashing::twox_128(value_name.as_bytes());

	let mut final_key = vec![0u8; 32];
	final_key[..16].copy_from_slice(&pallet_hash);
	final_key[16..].copy_from_slice(&storage_hash);

	StorageKey(final_key)
}

/// Can be use to access the runtime storage key of a `StorageMap`.
pub trait StorageMapKeyProvider {
	/// The name of the variable that holds the `StorageMap`.
	const MAP_NAME: &'static str;

	/// The same as `StorageMap::Hasher1`.
	type Hasher: StorageHasher;
	/// The same as `StorageMap::Key1`.
	type Key: FullCodec;
	/// The same as `StorageMap::Value`.
	type Value: FullCodec;

	/// This is a copy of the
	/// `frame_support::storage::generator::StorageMap::storage_map_final_key`.
	///
	/// We're using it because to call `storage_map_final_key` directly, we need access
	/// to the runtime and pallet instance, which (sometimes) is impossible.
	fn final_key(pallet_prefix: &str, key: &Self::Key) -> StorageKey {
		storage_map_final_key::<Self::Hasher>(pallet_prefix, Self::MAP_NAME, &key.encode())
	}
}

/// Can be use to access the runtime storage key of a `StorageDoubleMap`.
pub trait StorageDoubleMapKeyProvider {
	/// The name of the variable that holds the `StorageDoubleMap`.
	const MAP_NAME: &'static str;

	/// The same as `StorageDoubleMap::Hasher1`.
	type Hasher1: StorageHasher;
	/// The same as `StorageDoubleMap::Key1`.
	type Key1: FullCodec;
	/// The same as `StorageDoubleMap::Hasher2`.
	type Hasher2: StorageHasher;
	/// The same as `StorageDoubleMap::Key2`.
	type Key2: FullCodec;
	/// The same as `StorageDoubleMap::Value`.
	type Value: FullCodec;

	/// This is a copy of the
	/// `frame_support::storage::generator::StorageDoubleMap::storage_double_map_final_key`.
	///
	/// We're using it because to call `storage_double_map_final_key` directly, we need access
	/// to the runtime and pallet instance, which (sometimes) is impossible.
	fn final_key(pallet_prefix: &str, key1: &Self::Key1, key2: &Self::Key2) -> StorageKey {
		let key1_hashed = Self::Hasher1::hash(&key1.encode());
		let key2_hashed = Self::Hasher2::hash(&key2.encode());
		let pallet_prefix_hashed = frame_support::Twox128::hash(pallet_prefix.as_bytes());
		let storage_prefix_hashed = frame_support::Twox128::hash(Self::MAP_NAME.as_bytes());

		let mut final_key = Vec::with_capacity(
			pallet_prefix_hashed.len() +
				storage_prefix_hashed.len() +
				key1_hashed.as_ref().len() +
				key2_hashed.as_ref().len(),
		);

		final_key.extend_from_slice(&pallet_prefix_hashed[..]);
		final_key.extend_from_slice(&storage_prefix_hashed[..]);
		final_key.extend_from_slice(key1_hashed.as_ref());
		final_key.extend_from_slice(key2_hashed.as_ref());

		StorageKey(final_key)
	}
}

/// Error generated by the `OwnedBridgeModule` trait.
#[derive(Encode, Decode, PartialEq, Eq, TypeInfo, PalletError)]
pub enum OwnedBridgeModuleError {
	/// All pallet operations are halted.
	Halted,
}

/// Operating mode for a bridge module.
pub trait OperatingMode: Send + Copy + Debug + FullCodec {
	/// Returns true if the bridge module is halted.
	fn is_halted(&self) -> bool;
}

/// Basic operating modes for a bridges module (Normal/Halted).
#[derive(
	Encode,
	Decode,
	Clone,
	Copy,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub enum BasicOperatingMode {
	/// Normal mode, when all operations are allowed.
	Normal,
	/// The pallet is halted. All operations (except operating mode change) are prohibited.
	Halted,
}

impl Default for BasicOperatingMode {
	fn default() -> Self {
		Self::Normal
	}
}

impl OperatingMode for BasicOperatingMode {
	fn is_halted(&self) -> bool {
		*self == BasicOperatingMode::Halted
	}
}

/// Bridge module that has owner and operating mode
pub trait OwnedBridgeModule<T: frame_system::Config> {
	/// The target that will be used when publishing logs related to this module.
	const LOG_TARGET: &'static str;

	/// A storage entry that holds the module `Owner` account.
	type OwnerStorage: StorageValue<T::AccountId, Query = Option<T::AccountId>>;
	/// Operating mode type of the pallet.
	type OperatingMode: OperatingMode;
	/// A storage value that holds the pallet operating mode.
	type OperatingModeStorage: StorageValue<Self::OperatingMode, Query = Self::OperatingMode>;

	/// Check if the module is halted.
	fn is_halted() -> bool {
		Self::OperatingModeStorage::get().is_halted()
	}

	/// Ensure that the origin is either root, or `PalletOwner`.
	fn ensure_owner_or_root(origin: T::RuntimeOrigin) -> Result<(), BadOrigin> {
		match origin.into() {
			Ok(RawOrigin::Root) => Ok(()),
			Ok(RawOrigin::Signed(ref signer))
				if Self::OwnerStorage::get().as_ref() == Some(signer) =>
				Ok(()),
			_ => Err(BadOrigin),
		}
	}

	/// Ensure that the module is not halted.
	fn ensure_not_halted() -> Result<(), OwnedBridgeModuleError> {
		match Self::is_halted() {
			true => Err(OwnedBridgeModuleError::Halted),
			false => Ok(()),
		}
	}

	/// Change the owner of the module.
	fn set_owner(origin: T::RuntimeOrigin, maybe_owner: Option<T::AccountId>) -> DispatchResult {
		Self::ensure_owner_or_root(origin)?;
		match maybe_owner {
			Some(owner) => {
				Self::OwnerStorage::put(&owner);
				log::info!(target: Self::LOG_TARGET, "Setting pallet Owner to: {:?}", owner);
			},
			None => {
				Self::OwnerStorage::kill();
				log::info!(target: Self::LOG_TARGET, "Removed Owner of pallet.");
			},
		}

		Ok(())
	}

	/// Halt or resume all/some module operations.
	fn set_operating_mode(
		origin: T::RuntimeOrigin,
		operating_mode: Self::OperatingMode,
	) -> DispatchResult {
		Self::ensure_owner_or_root(origin)?;
		Self::OperatingModeStorage::put(operating_mode);
		log::info!(target: Self::LOG_TARGET, "Setting operating mode to {:?}.", operating_mode);
		Ok(())
	}
}

/// All extra operations with weights that we need in bridges.
pub trait WeightExtraOps {
	/// Checked division of individual components of two weights.
	///
	/// Divides components and returns minimal division result. Returns `None` if one
	/// of `other` weight components is zero.
	fn min_components_checked_div(&self, other: Weight) -> Option<u64>;
}

impl WeightExtraOps for Weight {
	fn min_components_checked_div(&self, other: Weight) -> Option<u64> {
		Some(sp_std::cmp::min(
			self.ref_time().checked_div(other.ref_time())?,
			self.proof_size().checked_div(other.proof_size())?,
		))
	}
}

/// Trait that provides a static `str`.
pub trait StaticStrProvider {
	/// Static string.
	const STR: &'static str;
}

/// A macro that generates `StaticStrProvider` with the string set to its stringified argument.
#[macro_export]
macro_rules! generate_static_str_provider {
	($str:expr) => {
		$crate::paste::item! {
			pub struct [<Str $str>];

			impl $crate::StaticStrProvider for [<Str $str>] {
				const STR: &'static str = stringify!($str);
			}
		}
	};
}

/// Error message that is only displayable in `std` environment.
#[derive(Encode, Decode, Clone, Eq, PartialEq, PalletError, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct StrippableError<T> {
	_phantom_data: sp_std::marker::PhantomData<T>,
	#[codec(skip)]
	#[cfg(feature = "std")]
	message: String,
}

impl<T: Debug> From<T> for StrippableError<T> {
	fn from(_err: T) -> Self {
		Self {
			_phantom_data: Default::default(),
			#[cfg(feature = "std")]
			message: format!("{:?}", _err),
		}
	}
}

impl<T> Debug for StrippableError<T> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		f.write_str(&self.message)
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		f.write_str("Stripped error")
	}
}

/// A trait defining helper methods for `RangeInclusive` (start..=end)
pub trait RangeInclusiveExt<Idx> {
	/// Computes the length of the `RangeInclusive`, checking for underflow and overflow.
	fn checked_len(&self) -> Option<Idx>;
	/// Computes the length of the `RangeInclusive`, saturating in case of underflow or overflow.
	fn saturating_len(&self) -> Idx;
}

impl<Idx> RangeInclusiveExt<Idx> for RangeInclusive<Idx>
where
	Idx: CheckedSub + CheckedAdd + SaturatingAdd + One + Zero,
{
	fn checked_len(&self) -> Option<Idx> {
		self.end()
			.checked_sub(self.start())
			.and_then(|len| len.checked_add(&Idx::one()))
	}

	fn saturating_len(&self) -> Idx {
		let len = match self.end().checked_sub(self.start()) {
			Some(len) => len,
			None => return Idx::zero(),
		};
		len.saturating_add(&Idx::one())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn storage_value_key_works() {
		assert_eq!(
			storage_value_key("PalletTransactionPayment", "NextFeeMultiplier"),
			StorageKey(
				hex_literal::hex!(
					"f0e954dfcca51a255ab12c60c789256a3f2edf3bdf381debe331ab7446addfdc"
				)
				.to_vec()
			),
		);
	}

	#[test]
	fn generate_static_str_provider_works() {
		generate_static_str_provider!(Test);
		assert_eq!(StrTest::STR, "Test");
	}
}
