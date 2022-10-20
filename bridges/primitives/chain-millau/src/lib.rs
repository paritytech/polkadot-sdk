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

use bp_messages::{
	InboundMessageDetails, LaneId, MessageNonce, MessagePayload, OutboundMessageDetails,
};
use bp_runtime::{decl_bridge_runtime_apis, Chain};
use frame_support::{
	dispatch::DispatchClass,
	weights::{constants::WEIGHT_PER_SECOND, IdentityFee, Weight},
	Parameter, RuntimeDebug,
};
use frame_system::limits;
use scale_info::TypeInfo;
use sp_core::{storage::StateVersion, Hasher as HasherT};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	FixedU128, MultiSignature, MultiSigner, Perbill,
};
use sp_std::prelude::*;
use sp_trie::{LayoutV0, LayoutV1, TrieConfiguration};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

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
// TODO: https://github.com/paritytech/parity-bridges-common/issues/1543 - remove `set_proof_size`
pub const MAXIMUM_BLOCK_WEIGHT: Weight = WEIGHT_PER_SECOND.set_proof_size(1_000).saturating_div(2);

/// Represents the portion of a block that will be used by Normal extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// Maximal number of unrewarded relayer entries in Millau confirmation transaction.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 128;

/// Maximal number of unconfirmed messages in Millau confirmation transaction.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 128;

/// Weight of single regular message delivery transaction on Millau chain.
///
/// This value is a result of `pallet_bridge_messages::Pallet::receive_messages_proof_weight()` call
/// for the case when single message of `pallet_bridge_messages::EXPECTED_DEFAULT_MESSAGE_LENGTH`
/// bytes is delivered. The message must have dispatch weight set to zero. The result then must be
/// rounded up to account possible future runtime upgrades.
pub const DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT: Weight = Weight::from_ref_time(1_500_000_000);

/// Increase of delivery transaction weight on Millau chain with every additional message byte.
///
/// This value is a result of
/// `pallet_bridge_messages::WeightInfoExt::storage_proof_size_overhead(1)` call. The result then
/// must be rounded up to account possible future runtime upgrades.
pub const ADDITIONAL_MESSAGE_BYTE_DELIVERY_WEIGHT: Weight = Weight::from_ref_time(25_000);

/// Maximal weight of single message delivery confirmation transaction on Millau chain.
///
/// This value is a result of `pallet_bridge_messages::Pallet::receive_messages_delivery_proof`
/// weight formula computation for the case when single message is confirmed. The result then must
/// be rounded up to account possible future runtime upgrades.
pub const MAX_SINGLE_MESSAGE_DELIVERY_CONFIRMATION_TX_WEIGHT: Weight =
	Weight::from_ref_time(2_000_000_000);

/// Weight of pay-dispatch-fee operation for inbound messages at Millau chain.
///
/// This value corresponds to the result of
/// `pallet_bridge_messages::WeightInfoExt::pay_inbound_dispatch_fee_overhead()` call for your
/// chain. Don't put too much reserve there, because it is used to **decrease**
/// `DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT` cost. So putting large reserve would make delivery
/// transactions cheaper.
pub const PAY_INBOUND_DISPATCH_FEE_WEIGHT: Weight = Weight::from_ref_time(700_000_000);

/// The target length of a session (how often authorities change) on Millau measured in of number of
/// blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 5 * time_units::MINUTES;

/// Maximal number of GRANDPA authorities at Millau.
pub const MAX_AUTHORITIES_COUNT: u32 = 5;

/// Maximal SCALE-encoded header size (in bytes) at Millau.
pub const MAX_HEADER_SIZE: u32 = 1024;

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

/// Index of a transaction in the chain.
pub type Index = u32;

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

/// Millau Hasher (Blake2-256 ++ Keccak-256) implementation.
#[derive(PartialEq, Eq, Clone, Copy, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
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

/// Name of the Rialto->Millau (actually DOT->KSM) conversion rate stored in the Millau runtime.
pub const RIALTO_TO_MILLAU_CONVERSION_RATE_PARAMETER_NAME: &str = "RialtoToMillauConversionRate";
/// Name of the RialtoParachain->Millau (actually DOT->KSM) conversion rate stored in the Millau
/// runtime.
pub const RIALTO_PARACHAIN_TO_MILLAU_CONVERSION_RATE_PARAMETER_NAME: &str =
	"RialtoParachainToMillauConversionRate";
/// Name of the RialtoParachain fee multiplier parameter, stored in the Millau runtime.
pub const RIALTO_PARACHAIN_FEE_MULTIPLIER_PARAMETER_NAME: &str = "RialtoParachainFeeMultiplier";

decl_bridge_runtime_apis!(millau);
