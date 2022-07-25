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

//! Primitives that may be used at (bridges) runtime level.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{
	log, pallet_prelude::DispatchResult, PalletError, RuntimeDebug, StorageHasher, StorageValue,
};
use frame_system::RawOrigin;
use scale_info::TypeInfo;
use sp_core::{hash::H256, storage::StorageKey};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::{BadOrigin, Header as HeaderT};
use sp_std::{convert::TryFrom, fmt::Debug, vec, vec::Vec};

pub use chain::{
	AccountIdOf, AccountPublicOf, BalanceOf, BlockNumberOf, Chain, EncodedOrDecodedCall, HashOf,
	HasherOf, HeaderOf, IndexOf, SignatureOf, TransactionEraOf,
};
pub use frame_support::storage::storage_prefix as storage_value_final_key;
use num_traits::{CheckedSub, One};
use sp_runtime::transaction_validity::TransactionValidity;
pub use storage_proof::{
	Error as StorageProofError, ProofSize as StorageProofSize, StorageProofChecker,
};

#[cfg(feature = "std")]
pub use storage_proof::craft_valid_storage_proof;

pub mod messages;

mod chain;
mod storage_proof;

/// Use this when something must be shared among all instances.
pub const NO_INSTANCE_ID: ChainId = [0, 0, 0, 0];

/// Bridge-with-Rialto instance id.
pub const RIALTO_CHAIN_ID: ChainId = *b"rlto";

/// Bridge-with-RialtoParachain instance id.
pub const RIALTO_PARACHAIN_CHAIN_ID: ChainId = *b"rlpa";

/// Bridge-with-Millau instance id.
pub const MILLAU_CHAIN_ID: ChainId = *b"mlau";

/// Bridge-with-Polkadot instance id.
pub const POLKADOT_CHAIN_ID: ChainId = *b"pdot";

/// Bridge-with-Kusama instance id.
pub const KUSAMA_CHAIN_ID: ChainId = *b"ksma";

/// Bridge-with-Rococo instance id.
pub const ROCOCO_CHAIN_ID: ChainId = *b"roco";

/// Bridge-with-Wococo instance id.
pub const WOCOCO_CHAIN_ID: ChainId = *b"woco";

/// Call-dispatch module prefix.
pub const CALL_DISPATCH_MODULE_PREFIX: &[u8] = b"pallet-bridge/dispatch";

/// A unique prefix for entropy when generating cross-chain account IDs.
pub const ACCOUNT_DERIVATION_PREFIX: &[u8] = b"pallet-bridge/account-derivation/account";

/// A unique prefix for entropy when generating a cross-chain account ID for the Root account.
pub const ROOT_ACCOUNT_DERIVATION_PREFIX: &[u8] = b"pallet-bridge/account-derivation/root";

/// Generic header Id.
#[derive(RuntimeDebug, Default, Clone, Copy, Eq, Hash, PartialEq)]
pub struct HeaderId<Hash, Number>(pub Number, pub Hash);

/// Generic header id provider.
pub trait HeaderIdProvider<Header: HeaderT> {
	// Get the header id.
	fn id(&self) -> HeaderId<Header::Hash, Header::Number>;

	// Get the header id for the parent block.
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

/// Type of accounts on the source chain.
pub enum SourceAccount<T> {
	/// An account that belongs to Root (privileged origin).
	Root,
	/// A non-privileged account.
	///
	/// The embedded account ID may or may not have a private key depending on the "owner" of the
	/// account (private key, pallet, proxy, etc.).
	Account(T),
}

/// Derive an account ID from a foreign account ID.
///
/// This function returns an encoded Blake2 hash. It is the responsibility of the caller to ensure
/// this can be successfully decoded into an AccountId.
///
/// The `bridge_id` is used to provide extra entropy when producing account IDs. This helps prevent
/// AccountId collisions between different bridges on a single target chain.
///
/// Note: If the same `bridge_id` is used across different chains (for example, if one source chain
/// is bridged to multiple target chains), then all the derived accounts would be the same across
/// the different chains. This could negatively impact users' privacy across chains.
pub fn derive_account_id<AccountId>(bridge_id: ChainId, id: SourceAccount<AccountId>) -> H256
where
	AccountId: Encode,
{
	match id {
		SourceAccount::Root =>
			(ROOT_ACCOUNT_DERIVATION_PREFIX, bridge_id).using_encoded(blake2_256),
		SourceAccount::Account(id) =>
			(ACCOUNT_DERIVATION_PREFIX, bridge_id, id).using_encoded(blake2_256),
	}
	.into()
}

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
#[derive(RuntimeDebug, Clone, Copy)]
pub enum TransactionEra<BlockNumber, BlockHash> {
	/// Transaction is immortal.
	Immortal,
	/// Transaction is valid for a given number of blocks, starting from given block.
	Mortal(HeaderId<BlockHash, BlockNumber>, u32),
}

impl<BlockNumber: Copy + Into<u64>, BlockHash: Copy> TransactionEra<BlockNumber, BlockHash> {
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

	/// Returns era that is used by FRAME-based runtimes.
	pub fn frame_era(&self) -> sp_runtime::generic::Era {
		match *self {
			TransactionEra::Immortal => sp_runtime::generic::Era::immortal(),
			TransactionEra::Mortal(header_id, period) =>
				sp_runtime::generic::Era::mortal(period as _, header_id.0.into()),
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

/// This is a copy of the
/// `frame_support::storage::generator::StorageDoubleMap::storage_double_map_final_key` for maps
/// based on selected hashers.
///
/// We're using it because to call `storage_double_map_final_key` directly, we need access to the
/// runtime and pallet instance, which (sometimes) is impossible.
pub fn storage_double_map_final_key<H1: StorageHasher, H2: StorageHasher>(
	pallet_prefix: &str,
	map_name: &str,
	key1: &[u8],
	key2: &[u8],
) -> StorageKey {
	let key1_hashed = H1::hash(key1);
	let key2_hashed = H2::hash(key2);
	let pallet_prefix_hashed = frame_support::Twox128::hash(pallet_prefix.as_bytes());
	let storage_prefix_hashed = frame_support::Twox128::hash(map_name.as_bytes());

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

/// This is how a storage key of storage parameter (`parameter_types! { storage Param: bool = false;
/// }`) is computed.
///
/// Copied from `frame_support::parameter_types` macro.
pub fn storage_parameter_key(parameter_name: &str) -> StorageKey {
	let mut buffer = Vec::with_capacity(1 + parameter_name.len() + 1);
	buffer.push(b':');
	buffer.extend_from_slice(parameter_name.as_bytes());
	buffer.push(b':');
	StorageKey(sp_io::hashing::twox_128(&buffer).to_vec())
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

/// Error generated by the `OwnedBridgeModule` trait.
#[derive(Encode, Decode, TypeInfo, PalletError)]
pub enum OwnedBridgeModuleError {
	/// All pallet operations are halted.
	Halted,
}

/// Operating mode for a bridge module.
pub trait OperatingMode: Send + Copy + Debug + FullCodec {
	// Returns true if the bridge module is halted.
	fn is_halted(&self) -> bool;
}

/// Basic operating modes for a bridges module (Normal/Halted).
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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

	type OwnerStorage: StorageValue<T::AccountId, Query = Option<T::AccountId>>;
	type OperatingMode: OperatingMode;
	type OperatingModeStorage: StorageValue<Self::OperatingMode, Query = Self::OperatingMode>;

	/// Check if the module is halted.
	fn is_halted() -> bool {
		Self::OperatingModeStorage::get().is_halted()
	}

	/// Ensure that the origin is either root, or `PalletOwner`.
	fn ensure_owner_or_root(origin: T::Origin) -> Result<(), BadOrigin> {
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
	fn set_owner(origin: T::Origin, maybe_owner: Option<T::AccountId>) -> DispatchResult {
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
		origin: T::Origin,
		operating_mode: Self::OperatingMode,
	) -> DispatchResult {
		Self::ensure_owner_or_root(origin)?;
		Self::OperatingModeStorage::put(operating_mode);
		log::info!(target: Self::LOG_TARGET, "Setting operating mode to {:?}.", operating_mode);
		Ok(())
	}
}

/// A trait for querying whether a runtime call is valid.
pub trait FilterCall<Call> {
	/// Checks if a runtime call is valid.
	fn validate(call: &Call) -> TransactionValidity;
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn storage_parameter_key_works() {
		assert_eq!(
			storage_parameter_key("MillauToRialtoConversionRate"),
			StorageKey(hex_literal::hex!("58942375551bb0af1682f72786b59d04").to_vec()),
		);
	}

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
}
