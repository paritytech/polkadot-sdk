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

use bp_header_chain::ChainWithGrandpa;
use bp_messages::{
	InboundMessageDetails, LaneId, MessageNonce, MessagePayload, OutboundMessageDetails,
};
use bp_runtime::{decl_bridge_runtime_apis, Chain};
use frame_support::{
	dispatch::DispatchClass,
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, IdentityFee, Weight},
	RuntimeDebug,
};
use frame_system::limits;
use sp_core::Hasher as HasherT;
use sp_runtime::{
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	MultiSignature, MultiSigner, Perbill,
};
use sp_std::prelude::*;

/// Number of extra bytes (excluding size of storage value itself) of storage proof, built at
/// Rialto chain. This mostly depends on number of entries (and their density) in the storage trie.
/// Some reserve is reserved to account future chain growth.
pub const EXTRA_STORAGE_PROOF_SIZE: u32 = 1024;

/// Number of bytes, included in the signed Rialto transaction apart from the encoded call itself.
///
/// Can be computed by subtracting encoded call size from raw transaction size.
pub const TX_EXTRA_BYTES: u32 = 104;

/// Maximal weight of single Rialto block.
///
/// This represents two seconds of compute assuming a target block time of six seconds.
///
/// Max PoV size is set to max value, since it isn't important for relay/standalone chains.
pub const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2), u64::MAX);

/// Represents the portion of a block that will be used by Normal extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// Maximal number of unrewarded relayer entries in Rialto confirmation transaction.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 1024;

/// Maximal number of unconfirmed messages in Rialto confirmation transaction.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1024;

/// The target length of a session (how often authorities change) on Rialto measured in of number of
/// blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 4;

/// Maximal number of GRANDPA authorities at Rialto.
pub const MAX_AUTHORITIES_COUNT: u32 = 5;

/// Reasonable number of headers in the `votes_ancestries` on Rialto chain.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
pub const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 8;

/// Approximate average header size in `votes_ancestries` field of justification on Rialto chain.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
pub const AVERAGE_HEADER_SIZE_IN_JUSTIFICATION: u32 = 256;

/// Approximate maximal header size on Rialto chain.
///
/// We expect maximal header to have digest item with the new authorities set for every consensus
/// engine (GRANDPA, Babe, BEEFY, ...) - so we multiply it by 3. And also
/// `AVERAGE_HEADER_SIZE_IN_JUSTIFICATION` bytes for other stuff.
///
/// See [`bp-header-chain::ChainWithGrandpa`] for more details.
pub const MAX_HEADER_SIZE: u32 = MAX_AUTHORITIES_COUNT
	.saturating_mul(3)
	.saturating_add(AVERAGE_HEADER_SIZE_IN_JUSTIFICATION);

/// Maximal size of encoded `bp_parachains::ParaStoredHeaderData` structure among all Rialto
/// parachains.
///
/// It includes the block number and state root, so it shall be near 40 bytes, but let's have some
/// reserve.
pub const MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE: u32 = 128;

/// Re-export `time_units` to make usage easier.
pub use time_units::*;

/// Human readable time units defined in terms of number of blocks.
pub mod time_units {
	use super::{BlockNumber, SESSION_LENGTH};

	pub const MILLISECS_PER_BLOCK: u64 = 6000;
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;

	pub const EPOCH_DURATION_IN_SLOTS: BlockNumber = SESSION_LENGTH;

	// 1 in 4 blocks (on average, not counting collisions) will be primary babe blocks.
	pub const PRIMARY_PROBABILITY: (u64, u64) = (1, 4);
}

/// Block number type used in Rialto.
pub type BlockNumber = u32;

/// Hash type used in Rialto.
pub type Hash = <BlakeTwo256 as HasherT>::Out;

/// The type of object that can produce hashes on Rialto.
pub type Hasher = BlakeTwo256;

/// The header type used by Rialto.
pub type Header = sp_runtime::generic::Header<BlockNumber, Hasher>;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// Balance of an account.
pub type Balance = u128;

/// An instant or duration in time.
pub type Moment = u64;

/// Index of a transaction in the chain.
pub type Index = u32;

/// Weight-to-Fee type used by Rialto.
pub type WeightToFee = IdentityFee<Balance>;

/// Rialto chain.
#[derive(RuntimeDebug)]
pub struct Rialto;

impl Chain for Rialto {
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Index = Index;
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

impl ChainWithGrandpa for Rialto {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = WITH_RIALTO_GRANDPA_PALLET_NAME;
	const MAX_AUTHORITIES_COUNT: u32 = MAX_AUTHORITIES_COUNT;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 =
		REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY;
	const MAX_HEADER_SIZE: u32 = MAX_HEADER_SIZE;
	const AVERAGE_HEADER_SIZE_IN_JUSTIFICATION: u32 = AVERAGE_HEADER_SIZE_IN_JUSTIFICATION;
}

frame_support::parameter_types! {
	pub BlockLength: limits::BlockLength =
		limits::BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub BlockWeights: limits::BlockWeights =
		limits::BlockWeights::with_sensible_defaults(MAXIMUM_BLOCK_WEIGHT, NORMAL_DISPATCH_RATIO);
}

/// Name of the With-Rialto GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_RIALTO_GRANDPA_PALLET_NAME: &str = "BridgeRialtoGrandpa";
/// Name of the With-Rialto messages pallet instance that is deployed at bridged chains.
pub const WITH_RIALTO_MESSAGES_PALLET_NAME: &str = "BridgeRialtoMessages";
/// Name of the With-Rialto parachains bridge pallet instance that is deployed at bridged chains.
pub const WITH_RIALTO_BRIDGE_PARAS_PALLET_NAME: &str = "BridgeRialtoParachains";

/// Name of the parachain registrar pallet in the Rialto runtime.
pub const PARAS_REGISTRAR_PALLET_NAME: &str = "Registrar";

/// Name of the parachains pallet in the Rialto runtime.
pub const PARAS_PALLET_NAME: &str = "Paras";

decl_bridge_runtime_apis!(rialto);
