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
use sp_core::Hasher as HasherT;
use sp_runtime::{
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	FixedU128, MultiSignature, MultiSigner, Perbill,
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
// TODO: https://github.com/paritytech/parity-bridges-common/issues/1543 - remove `set_proof_size`
pub const MAXIMUM_BLOCK_WEIGHT: Weight = WEIGHT_PER_SECOND.set_proof_size(1_000).saturating_mul(2);

/// Represents the portion of a block that will be used by Normal extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// Maximal number of unrewarded relayer entries in Rialto confirmation transaction.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 1024;

/// Maximal number of unconfirmed messages in Rialto confirmation transaction.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1024;

/// Weight of single regular message delivery transaction on Rialto chain.
///
/// This value is a result of `pallet_bridge_messages::Pallet::receive_messages_proof_weight()` call
/// for the case when single message of `pallet_bridge_messages::EXPECTED_DEFAULT_MESSAGE_LENGTH`
/// bytes is delivered. The message must have dispatch weight set to zero. The result then must be
/// rounded up to account possible future runtime upgrades.
pub const DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT: Weight = Weight::from_ref_time(1_500_000_000);

/// Increase of delivery transaction weight on Rialto chain with every additional message byte.
///
/// This value is a result of
/// `pallet_bridge_messages::WeightInfoExt::storage_proof_size_overhead(1)` call. The result then
/// must be rounded up to account possible future runtime upgrades.
pub const ADDITIONAL_MESSAGE_BYTE_DELIVERY_WEIGHT: Weight = Weight::from_ref_time(25_000);

/// Maximal weight of single message delivery confirmation transaction on Rialto chain.
///
/// This value is a result of `pallet_bridge_messages::Pallet::receive_messages_delivery_proof`
/// weight formula computation for the case when single message is confirmed. The result then must
/// be rounded up to account possible future runtime upgrades.
pub const MAX_SINGLE_MESSAGE_DELIVERY_CONFIRMATION_TX_WEIGHT: Weight =
	Weight::from_ref_time(2_000_000_000);

/// Weight of pay-dispatch-fee operation for inbound messages at Rialto chain.
///
/// This value corresponds to the result of
/// `pallet_bridge_messages::WeightInfoExt::pay_inbound_dispatch_fee_overhead()` call for your
/// chain. Don't put too much reserve there, because it is used to **decrease**
/// `DEFAULT_MESSAGE_DELIVERY_TX_WEIGHT` cost. So putting large reserve would make delivery
/// transactions cheaper.
pub const PAY_INBOUND_DISPATCH_FEE_WEIGHT: Weight = Weight::from_ref_time(700_000_000);

/// The target length of a session (how often authorities change) on Rialto measured in of number of
/// blocks.
///
/// Note that since this is a target sessions may change before/after this time depending on network
/// conditions.
pub const SESSION_LENGTH: BlockNumber = 4;

/// Maximal number of GRANDPA authorities at Rialto.
pub const MAX_AUTHORITIES_COUNT: u32 = 5;

/// Maximal SCALE-encoded header size (in bytes) at Rialto.
pub const MAX_HEADER_SIZE: u32 = 1024;

/// Maximal SCALE-encoded size of parachains headers that are stored at Rialto `Paras` pallet.
pub const MAX_NESTED_PARACHAIN_HEAD_SIZE: u32 = MAX_HEADER_SIZE;

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

/// Name of the Millau->Rialto (actually KSM->DOT) conversion rate stored in the Rialto runtime.
pub const MILLAU_TO_RIALTO_CONVERSION_RATE_PARAMETER_NAME: &str = "MillauToRialtoConversionRate";

/// Name of the parachain registrar pallet in the Rialto runtime.
pub const PARAS_REGISTRAR_PALLET_NAME: &str = "Registrar";

/// Name of the parachains pallet in the Rialto runtime.
pub const PARAS_PALLET_NAME: &str = "Paras";

decl_bridge_runtime_apis!(rialto);
