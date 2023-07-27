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
// RuntimeApi generated functions
#![allow(clippy::too_many_arguments)]

mod millau_hash;

use bp_beefy::ChainWithBeefy;
use bp_header_chain::ChainWithGrandpa;
use bp_messages::{
	InboundMessageDetails, LaneId, MessageNonce, MessagePayload, OutboundMessageDetails,
};
use bp_runtime::{decl_bridge_finality_runtime_apis, decl_bridge_runtime_apis, Chain};
use frame_support::{
	dispatch::DispatchClass,
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, IdentityFee, Weight},
	RuntimeDebug,
};
use frame_system::limits;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{storage::StateVersion, Hasher as HasherT};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, MultiSigner, Perbill,
};
use sp_std::prelude::*;
use sp_trie::{LayoutV0, LayoutV1, TrieConfiguration};

use sp_runtime::traits::Keccak256;

pub use millau_hash::MillauHash;

/// Number of extra bytes (excluding size of storage value itself) of storage proof, built at
/// Millau chain. This mostly depends on number of entries (and their density) in the storage trie.
/// Some reserve is reserved to account future chain growth.
pub const EXTRA_STORAGE_PROOF_SIZE: u32 = 1024;

/// Number of bytes, included in the signed Millau transaction apart from the encoded call itself.
///
/// Can be computed by subtracting encoded call size from raw transaction size.
pub const TX_EXTRA_BYTES: u32 = 103;

/// Maximum weight of single Millau block.
///
/// This represents 0.5 seconds of compute assuming a target block time of six seconds.
///
/// Max PoV size is set to max value, since it isn't important for relay/standalone chains.
pub const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_div(2), u64::MAX);

/// Represents the portion of a block that will be used by Normal extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// Maximal number of unrewarded relayer entries in Millau confirmation transaction.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 128;

/// Maximal number of unconfirmed messages in Millau confirmation transaction.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 128;

/// The target length of a session (how often authorities change) on Millau measured in of number of
/// blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 5 * time_units::MINUTES;

/// Maximal number of GRANDPA authorities at Millau.
pub const MAX_AUTHORITIES_COUNT: u32 = 5;

/// Reasonable number of headers in the `votes_ancestries` on Millau chain.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
pub const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 8;

/// Approximate average header size in `votes_ancestries` field of justification on Millau chain.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
pub const AVERAGE_HEADER_SIZE_IN_JUSTIFICATION: u32 = 256;

/// Approximate maximal header size on Millau chain.
///
/// We expect maximal header to have digest item with the new authorities set for every consensus
/// engine (GRANDPA, Babe, BEEFY, ...) - so we multiply it by 3. And also
/// `AVERAGE_HEADER_SIZE_IN_JUSTIFICATION` bytes for other stuff.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
pub const MAX_HEADER_SIZE: u32 = MAX_AUTHORITIES_COUNT
	.saturating_mul(3)
	.saturating_add(AVERAGE_HEADER_SIZE_IN_JUSTIFICATION);

/// Re-export `time_units` to make usage easier.
pub use time_units::*;

/// Human readable time units defined in terms of number of blocks.
pub mod time_units {
	use super::BlockNumber;

	pub const MILLISECS_PER_BLOCK: u64 = 6000;
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;
}

/// Block number type used in Millau.
pub type BlockNumber = u64;

/// Hash type used in Millau.
pub type Hash = <BlakeTwoAndKeccak256 as HasherT>::Out;

/// Type of object that can produce hashes on Millau.
pub type Hasher = BlakeTwoAndKeccak256;

/// The header type used by Millau.
pub type Header = sp_runtime::generic::Header<BlockNumber, Hasher>;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// Balance of an account.
pub type Balance = u64;

/// Nonce of a transaction in the chain.
pub type Nonce = u32;

/// Weight-to-Fee type used by Millau.
pub type WeightToFee = IdentityFee<Balance>;

/// Millau chain.
#[derive(RuntimeDebug)]
pub struct Millau;

impl Chain for Millau {
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		*BlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		BlockWeights::get()
			.get(DispatchClass::Normal)
			.max_extrinsic
			.unwrap_or(Weight::MAX)
	}
}

impl ChainWithGrandpa for Millau {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = WITH_MILLAU_GRANDPA_PALLET_NAME;
	const MAX_AUTHORITIES_COUNT: u32 = MAX_AUTHORITIES_COUNT;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 =
		REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY;
	const MAX_HEADER_SIZE: u32 = MAX_HEADER_SIZE;
	const AVERAGE_HEADER_SIZE_IN_JUSTIFICATION: u32 = AVERAGE_HEADER_SIZE_IN_JUSTIFICATION;
}

impl ChainWithBeefy for Millau {
	type CommitmentHasher = Keccak256;
	type MmrHashing = Keccak256;
	type MmrHash = <Keccak256 as sp_runtime::traits::Hash>::Output;
	type BeefyMmrLeafExtra = ();
	type AuthorityId = bp_beefy::EcdsaValidatorId;
	type AuthorityIdToMerkleLeaf = bp_beefy::BeefyEcdsaToEthereum;
}

/// Millau Hasher (Blake2-256 ++ Keccak-256) implementation.
#[derive(PartialEq, Eq, Clone, Copy, RuntimeDebug, TypeInfo, Serialize, Deserialize)]
pub struct BlakeTwoAndKeccak256;

impl sp_core::Hasher for BlakeTwoAndKeccak256 {
	type Out = MillauHash;
	type StdHasher = hash256_std_hasher::Hash256StdHasher;
	const LENGTH: usize = 64;

	fn hash(s: &[u8]) -> Self::Out {
		let mut combined_hash = MillauHash::default();
		combined_hash.as_mut()[..32].copy_from_slice(&sp_io::hashing::blake2_256(s));
		combined_hash.as_mut()[32..].copy_from_slice(&sp_io::hashing::keccak_256(s));
		combined_hash
	}
}

impl sp_runtime::traits::Hash for BlakeTwoAndKeccak256 {
	type Output = MillauHash;

	fn trie_root(input: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> Self::Output {
		match state_version {
			StateVersion::V0 => LayoutV0::<BlakeTwoAndKeccak256>::trie_root(input),
			StateVersion::V1 => LayoutV1::<BlakeTwoAndKeccak256>::trie_root(input),
		}
	}

	fn ordered_trie_root(input: Vec<Vec<u8>>, state_version: StateVersion) -> Self::Output {
		match state_version {
			StateVersion::V0 => LayoutV0::<BlakeTwoAndKeccak256>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<BlakeTwoAndKeccak256>::ordered_trie_root(input),
		}
	}
}

frame_support::parameter_types! {
	pub BlockLength: limits::BlockLength =
		limits::BlockLength::max_with_normal_ratio(2 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub BlockWeights: limits::BlockWeights =
		limits::BlockWeights::with_sensible_defaults(MAXIMUM_BLOCK_WEIGHT, NORMAL_DISPATCH_RATIO);
}

/// Name of the With-Millau GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_MILLAU_GRANDPA_PALLET_NAME: &str = "BridgeMillauGrandpa";
/// Name of the With-Millau messages pallet instance that is deployed at bridged chains.
pub const WITH_MILLAU_MESSAGES_PALLET_NAME: &str = "BridgeMillauMessages";
/// Name of the transaction payment pallet at the Millau runtime.
pub const TRANSACTION_PAYMENT_PALLET_NAME: &str = "TransactionPayment";

decl_bridge_runtime_apis!(millau, grandpa);
