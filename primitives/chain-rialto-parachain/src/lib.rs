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
use bp_runtime::{decl_bridge_runtime_apis, Chain, Parachain};
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
use sp_std::vec::Vec;

/// Identifier of RialtoParachain in the Rialto relay chain.
///
/// This identifier is not something that is declared either by Rialto or RialtoParachain. This
/// is an identifier of registration. So in theory it may be changed. But since bridge is going
/// to be deployed after parachain registration AND since parachain de-registration is highly
/// likely impossible, it is fine to declare this constant here.
pub const RIALTO_PARACHAIN_ID: u32 = 2000;

/// Number of extra bytes (excluding size of storage value itself) of storage proof, built at
/// RialtoParachain chain. This mostly depends on number of entries (and their density) in the
/// storage trie. Some reserve is reserved to account future chain growth.
pub const EXTRA_STORAGE_PROOF_SIZE: u32 = 1024;

/// Can be computed by subtracting encoded call size from raw transaction size.
pub const TX_EXTRA_BYTES: u32 = 104;

/// Maximal weight of single RialtoParachain block.
///
/// This represents two seconds of compute assuming a target block time of six seconds.
///
/// Max PoV size is set to `5Mb` as all Cumulus-based parachains do.
pub const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2), 5 * 1024 * 1024);

/// Represents the portion of a block that will be used by Normal extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// Maximal number of unrewarded relayer entries in Rialto confirmation transaction.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 1024;

/// Maximal number of unconfirmed messages in Rialto confirmation transaction.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 1024;

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

/// Nonce of a transaction in the parachain.
pub type Nonce = u32;

/// Weight-to-Fee type used by Rialto parachain.
pub type WeightToFee = IdentityFee<Balance>;

/// Rialto parachain.
#[derive(RuntimeDebug)]
pub struct RialtoParachain;

impl Chain for RialtoParachain {
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

impl Parachain for RialtoParachain {
	const PARACHAIN_ID: u32 = RIALTO_PARACHAIN_ID;
}

// Technically this is incorrect, because rialto-parachain isn't a bridge hub, but we're
// trying to keep it close to the bridge hubs code (at least in this aspect).
pub use bp_bridge_hub_cumulus::SignedExtension;

frame_support::parameter_types! {
	pub BlockLength: limits::BlockLength =
		limits::BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub BlockWeights: limits::BlockWeights =
		limits::BlockWeights::with_sensible_defaults(MAXIMUM_BLOCK_WEIGHT, NORMAL_DISPATCH_RATIO);
}

/// Name of the With-Rialto-Parachain messages pallet instance that is deployed at bridged chains.
pub const WITH_RIALTO_PARACHAIN_MESSAGES_PALLET_NAME: &str = "BridgeRialtoParachainMessages";
/// Name of the transaction payment pallet at the Rialto parachain runtime.
pub const TRANSACTION_PAYMENT_PALLET_NAME: &str = "TransactionPayment";

decl_bridge_runtime_apis!(rialto_parachain);
