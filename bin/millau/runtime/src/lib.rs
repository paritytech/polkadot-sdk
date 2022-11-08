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

//! The Millau runtime. This can be compiled with `#[no_std]`, ready for Wasm.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]
// From construct_runtime macro
#![allow(clippy::from_over_into)]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod rialto_messages;
pub mod rialto_parachain_messages;
pub mod xcm_config;

use crate::{
	rialto_messages::{ToRialtoMessagePayload, WithRialtoMessageBridge},
	rialto_parachain_messages::{
		ToRialtoParachainMessagePayload, WithRialtoParachainMessageBridge,
	},
};

use beefy_primitives::{crypto::AuthorityId as BeefyId, mmr::MmrLeafVersion, ValidatorSet};
use bp_runtime::{HeaderId, HeaderIdProvider};
use bridge_runtime_common::messages::{
	source::estimate_message_dispatch_and_delivery_fee, MessageBridge,
};
use codec::Decode;
use pallet_grandpa::{
	fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use pallet_transaction_payment::{FeeDetails, Multiplier, RuntimeDispatchInfo};
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_mmr_primitives::{DataOrHash, EncodableOpaqueLeaf, Error as MmrError, Proof as MmrProof};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{Block as BlockT, IdentityLookup, Keccak256, NumberFor, OpaqueKeys},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, FixedPointNumber, FixedU128, Perquintill,
};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// to be able to use Millau runtime in `bridge-runtime-common` tests
pub use bridge_runtime_common;

// A few exports that help ease life for downstream crates.
pub use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	parameter_types,
	traits::{Currency, ExistenceRequirement, Imbalance, KeyOwnerProofSystem},
	weights::{
		constants::WEIGHT_PER_SECOND, ConstantMultiplier, IdentityFee, RuntimeDbWeight, Weight,
	},
	RuntimeDebug, StorageValue,
};

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;
pub use pallet_bridge_grandpa::Call as BridgeGrandpaCall;
pub use pallet_bridge_messages::Call as MessagesCall;
pub use pallet_bridge_parachains::Call as BridgeParachainsCall;
pub use pallet_sudo::Call as SudoCall;
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_xcm::Call as XcmCall;

use bridge_runtime_common::generate_bridge_reject_obsolete_headers_and_messages;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
pub use sp_runtime::{Perbill, Permill};

/// An index to a block.
pub type BlockNumber = bp_millau::BlockNumber;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = bp_millau::Signature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = bp_millau::AccountId;

/// The type for looking up accounts. We don't expect more than 4 billion of them, but you
/// never know...
pub type AccountIndex = u32;

/// Balance of an account.
pub type Balance = bp_millau::Balance;

/// Index of a transaction in the chain.
pub type Index = bp_millau::Index;

/// A hash of some data used by the chain.
pub type Hash = bp_millau::Hash;

/// Hashing algorithm used by the chain.
pub type Hashing = bp_millau::Hasher;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	use super::*;

	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, Hashing>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
		pub beefy: Beefy,
		pub grandpa: Grandpa,
	}
}

/// This runtime version.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("millau-runtime"),
	impl_name: create_runtime_str!("millau-runtime"),
	authoring_version: 1,
	spec_version: 1,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
	state_version: 0,
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

parameter_types! {
	pub const BlockHashCount: BlockNumber = 250;
	pub const Version: RuntimeVersion = VERSION;
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 60_000_000, // ~0.06 ms = ~60 µs
		write: 200_000_000, // ~0.2 ms = 200 µs
	};
	pub const SS58Prefix: u8 = 60;
}

impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = frame_support::traits::Everything;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = IdentityLookup<AccountId>;
	/// The index type for storing how many extrinsics an account has signed.
	type Index = Index;
	/// The index type for blocks.
	type BlockNumber = BlockNumber;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = Hashing;
	/// The header type.
	type Header = generic::Header<BlockNumber, Hashing>;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	/// The ubiquitous origin type.
	type RuntimeOrigin = RuntimeOrigin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// Version of the runtime.
	type Version = Version;
	/// Provides information about the pallet setup in the runtime.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = ();
	/// Block and extrinsics weights: base values and limits.
	type BlockWeights = bp_millau::BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = bp_millau::BlockLength;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = DbWeight;
	/// The designated SS58 prefix of this chain.
	type SS58Prefix = SS58Prefix;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_randomness_collective_flip::Config for Runtime {}

parameter_types! {
	pub const MaxAuthorities: u32 = 10;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type MaxAuthorities = MaxAuthorities;
	type DisabledValidators = ();
}

impl pallet_beefy::Config for Runtime {
	type BeefyId = BeefyId;
	type MaxAuthorities = MaxAuthorities;
	type OnNewValidatorSet = MmrLeaf;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type KeyOwnerProofSystem = ();
	type KeyOwnerProof =
		<Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::Proof;
	type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		GrandpaId,
	)>>::IdentificationTuple;
	type HandleEquivocation = ();
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
	type MaxAuthorities = MaxAuthorities;
}

/// MMR helper types.
mod mmr {
	use super::Runtime;
	pub use pallet_mmr::primitives::*;
	use sp_runtime::traits::Keccak256;

	pub type Leaf = <<Runtime as pallet_mmr::Config>::LeafData as LeafDataProvider>::LeafData;
	pub type Hash = <Keccak256 as sp_runtime::traits::Hash>::Output;
	pub type Hashing = <Runtime as pallet_mmr::Config>::Hashing;
}

impl pallet_mmr::Config for Runtime {
	const INDEXING_PREFIX: &'static [u8] = b"mmr";
	type Hashing = Keccak256;
	type Hash = mmr::Hash;
	type OnNewRoot = pallet_beefy_mmr::DepositBeefyDigest<Runtime>;
	type WeightInfo = ();
	type LeafData = pallet_beefy_mmr::Pallet<Runtime>;
}

parameter_types! {
	/// Version of the produced MMR leaf.
	///
	/// The version consists of two parts;
	/// - `major` (3 bits)
	/// - `minor` (5 bits)
	///
	/// `major` should be updated only if decoding the previous MMR Leaf format from the payload
	/// is not possible (i.e. backward incompatible change).
	/// `minor` should be updated if fields are added to the previous MMR Leaf, which given SCALE
	/// encoding does not prevent old leafs from being decoded.
	///
	/// Hence we expect `major` to be changed really rarely (think never).
	/// See [`MmrLeafVersion`] type documentation for more details.
	pub LeafVersion: MmrLeafVersion = MmrLeafVersion::new(0, 0);
}

pub struct BeefyDummyDataProvider;

impl beefy_primitives::mmr::BeefyDataProvider<()> for BeefyDummyDataProvider {
	fn extra_data() {}
}

impl pallet_beefy_mmr::Config for Runtime {
	type LeafVersion = LeafVersion;
	type BeefyAuthorityToMerkleLeaf = pallet_beefy_mmr::BeefyEcdsaToEthereum;
	type LeafExtra = ();
	type BeefyDataProvider = BeefyDummyDataProvider;
}

parameter_types! {
	pub const MinimumPeriod: u64 = bp_millau::SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the UNIX epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = MinimumPeriod;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

parameter_types! {
	pub const ExistentialDeposit: bp_millau::Balance = 500;
	// For weight estimation, we assume that the most locks on an individual account will be 50.
	// This number may need to be adjusted in the future if this assumption no longer holds true.
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
}

parameter_types! {
	pub const TransactionBaseFee: Balance = 0;
	pub const TransactionByteFee: Balance = 1;
	pub const OperationalFeeMultiplier: u8 = 5;
	// values for following parameters are copied from polkadot repo, but it is fine
	// not to sync them - we're not going to make Rialto a full copy of one of Polkadot-like chains
	pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
	pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(3, 100_000);
	pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000u128);
	pub MaximumMultiplier: Multiplier = sp_runtime::traits::Bounded::max_value();
}

impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, ()>;
	type OperationalFeeMultiplier = OperationalFeeMultiplier;
	type WeightToFee = bp_millau::WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = pallet_transaction_payment::TargetedFeeAdjustment<
		Runtime,
		TargetBlockFullness,
		AdjustmentVariable,
		MinimumMultiplier,
		MaximumMultiplier,
	>;
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
}

parameter_types! {
	/// Authorities are changing every 5 minutes.
	pub const Period: BlockNumber = bp_millau::SESSION_LENGTH;
	pub const Offset: BlockNumber = 0;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ();
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = pallet_shift_session_manager::Pallet<Runtime>;
	type SessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

parameter_types! {
	// This is a pretty unscientific cap.
	//
	// Note that once this is hit the pallet will essentially throttle incoming requests down to one
	// call per block.
	pub const MaxRequests: u32 = 50;
}

impl pallet_bridge_relayers::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure = bp_relayers::MintReward<pallet_balances::Pallet<Runtime>, AccountId>;
	type WeightInfo = ();
}

#[cfg(feature = "runtime-benchmarks")]
parameter_types! {
	/// Number of headers to keep in benchmarks.
	///
	/// In benchmarks we always populate with full number of `HeadersToKeep` to make sure that
	/// pruning is taken into account.
	///
	/// Note: This is lower than regular value, to speed up benchmarking setup.
	pub const HeadersToKeep: u32 = 1024;
	/// Maximal number of authorities at Rialto.
	///
	/// In benchmarks we're using sets of up to `1024` authorities to prepare for possible
	/// upgrades in the future and see if performance degrades when number of authorities
	/// grow.
	pub const MaxAuthoritiesAtRialto: u32 = pallet_bridge_grandpa::benchmarking::MAX_VALIDATOR_SET_SIZE;
}

#[cfg(not(feature = "runtime-benchmarks"))]
parameter_types! {
	/// Number of headers to keep.
	///
	/// Assuming the worst case of every header being finalized, we will keep headers at least for a
	/// week.
	pub const HeadersToKeep: u32 = 7 * bp_rialto::DAYS;
	/// Maximal number of authorities at Rialto.
	pub const MaxAuthoritiesAtRialto: u32 = bp_rialto::MAX_AUTHORITIES_COUNT;
}

parameter_types! {
	/// Maximal size of SCALE-encoded Rialto header.
	pub const MaxRialtoHeaderSize: u32 = bp_rialto::MAX_HEADER_SIZE;

	/// Maximal number of authorities at Westend.
	pub const MaxAuthoritiesAtWestend: u32 = bp_westend::MAX_AUTHORITIES_COUNT;
	/// Maximal size of SCALE-encoded Westend header.
	pub const MaxWestendHeaderSize: u32 = bp_westend::MAX_HEADER_SIZE;
}

pub type RialtoGrandpaInstance = ();
impl pallet_bridge_grandpa::Config for Runtime {
	type BridgedChain = bp_rialto::Rialto;
	type MaxRequests = MaxRequests;
	type HeadersToKeep = HeadersToKeep;
	type MaxBridgedAuthorities = MaxAuthoritiesAtRialto;
	type MaxBridgedHeaderSize = MaxRialtoHeaderSize;

	type WeightInfo = pallet_bridge_grandpa::weights::BridgeWeight<Runtime>;
}

pub type WestendGrandpaInstance = pallet_bridge_grandpa::Instance1;
impl pallet_bridge_grandpa::Config<WestendGrandpaInstance> for Runtime {
	type BridgedChain = bp_westend::Westend;
	type MaxRequests = MaxRequests;
	type HeadersToKeep = HeadersToKeep;
	type MaxBridgedAuthorities = MaxAuthoritiesAtWestend;
	type MaxBridgedHeaderSize = MaxWestendHeaderSize;

	type WeightInfo = pallet_bridge_grandpa::weights::BridgeWeight<Runtime>;
}

impl pallet_shift_session_manager::Config for Runtime {}

parameter_types! {
	pub const MaxMessagesToPruneAtOnce: bp_messages::MessageNonce = 8;
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_rialto::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_rialto::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	// `IdentityFee` is used by Millau => we may use weight directly
	pub const GetDeliveryConfirmationTransactionFee: Balance =
		bp_millau::MAX_SINGLE_MESSAGE_DELIVERY_CONFIRMATION_TX_WEIGHT.ref_time() as _;
	pub const RootAccountForPayments: Option<AccountId> = None;
	pub const RialtoChainId: bp_runtime::ChainId = bp_runtime::RIALTO_CHAIN_ID;
	pub const RialtoParachainChainId: bp_runtime::ChainId = bp_runtime::RIALTO_PARACHAIN_CHAIN_ID;
}

/// Instance of the messages pallet used to relay messages to/from Rialto chain.
pub type WithRialtoMessagesInstance = ();

impl pallet_bridge_messages::Config<WithRialtoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_bridge_messages::weights::BridgeWeight<Runtime>;
	type Parameter = rialto_messages::MillauToRialtoMessagesParameter;
	type MaxMessagesToPruneAtOnce = MaxMessagesToPruneAtOnce;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = crate::rialto_messages::ToRialtoMaximalOutboundPayloadSize;
	type OutboundPayload = crate::rialto_messages::ToRialtoMessagePayload;
	type OutboundMessageFee = Balance;

	type InboundPayload = crate::rialto_messages::FromRialtoMessagePayload;
	type InboundMessageFee = bp_rialto::Balance;
	type InboundRelayer = bp_rialto::AccountId;

	type TargetHeaderChain = crate::rialto_messages::Rialto;
	type LaneMessageVerifier = crate::rialto_messages::ToRialtoMessageVerifier;
	type MessageDeliveryAndDispatchPayment =
		pallet_bridge_relayers::MessageDeliveryAndDispatchPaymentAdapter<
			Runtime,
			WithRialtoMessagesInstance,
			GetDeliveryConfirmationTransactionFee,
		>;
	type OnMessageAccepted = ();
	type OnDeliveryConfirmed = ();

	type SourceHeaderChain = crate::rialto_messages::Rialto;
	type MessageDispatch = crate::rialto_messages::FromRialtoMessageDispatch;
	type BridgedChainId = RialtoChainId;
}

/// Instance of the messages pallet used to relay messages to/from RialtoParachain chain.
pub type WithRialtoParachainMessagesInstance = pallet_bridge_messages::Instance1;

impl pallet_bridge_messages::Config<WithRialtoParachainMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_bridge_messages::weights::BridgeWeight<Runtime>;
	type Parameter = rialto_parachain_messages::MillauToRialtoParachainMessagesParameter;
	type MaxMessagesToPruneAtOnce = MaxMessagesToPruneAtOnce;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize =
		crate::rialto_parachain_messages::ToRialtoParachainMaximalOutboundPayloadSize;
	type OutboundPayload = crate::rialto_parachain_messages::ToRialtoParachainMessagePayload;
	type OutboundMessageFee = Balance;

	type InboundPayload = crate::rialto_parachain_messages::FromRialtoParachainMessagePayload;
	type InboundMessageFee = bp_rialto_parachain::Balance;
	type InboundRelayer = bp_rialto_parachain::AccountId;

	type TargetHeaderChain = crate::rialto_parachain_messages::RialtoParachain;
	type LaneMessageVerifier = crate::rialto_parachain_messages::ToRialtoParachainMessageVerifier;
	type MessageDeliveryAndDispatchPayment =
		pallet_bridge_relayers::MessageDeliveryAndDispatchPaymentAdapter<
			Runtime,
			WithRialtoParachainMessagesInstance,
			GetDeliveryConfirmationTransactionFee,
		>;
	type OnMessageAccepted = ();
	type OnDeliveryConfirmed = ();

	type SourceHeaderChain = crate::rialto_parachain_messages::RialtoParachain;
	type MessageDispatch = crate::rialto_parachain_messages::FromRialtoParachainMessageDispatch;
	type BridgedChainId = RialtoParachainChainId;
}

parameter_types! {
	pub const RialtoParasPalletName: &'static str = bp_rialto::PARAS_PALLET_NAME;
	pub const WestendParasPalletName: &'static str = bp_westend::PARAS_PALLET_NAME;
	pub const MaxRialtoParaHeadSize: u32 = bp_rialto::MAX_NESTED_PARACHAIN_HEAD_SIZE;
	pub const MaxWestendParaHeadSize: u32 = bp_westend::MAX_NESTED_PARACHAIN_HEAD_SIZE;
}

/// Instance of the with-Rialto parachains pallet.
pub type WithRialtoParachainsInstance = ();

impl pallet_bridge_parachains::Config<WithRialtoParachainsInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_bridge_parachains::weights::BridgeWeight<Runtime>;
	type BridgesGrandpaPalletInstance = RialtoGrandpaInstance;
	type ParasPalletName = RialtoParasPalletName;
	type TrackedParachains = frame_support::traits::Everything;
	type HeadsToKeep = HeadersToKeep;
	type MaxParaHeadSize = MaxRialtoParaHeadSize;
}

/// Instance of the with-Westend parachains pallet.
pub type WithWestendParachainsInstance = pallet_bridge_parachains::Instance1;

impl pallet_bridge_parachains::Config<WithWestendParachainsInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_bridge_parachains::weights::BridgeWeight<Runtime>;
	type BridgesGrandpaPalletInstance = WestendGrandpaInstance;
	type ParasPalletName = WestendParasPalletName;
	type TrackedParachains = frame_support::traits::Everything;
	type HeadsToKeep = HeadersToKeep;
	type MaxParaHeadSize = MaxWestendParaHeadSize;
}

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Sudo: pallet_sudo::{Pallet, Call, Config<T>, Storage, Event<T>},

		// Must be before session.
		Aura: pallet_aura::{Pallet, Config<T>},

		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>},

		// Consensus support.
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
		Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config, Event},
		ShiftSessionManager: pallet_shift_session_manager::{Pallet},
		RandomnessCollectiveFlip: pallet_randomness_collective_flip::{Pallet, Storage},

		// BEEFY Bridges support.
		Beefy: pallet_beefy::{Pallet, Storage, Config<T>},
		Mmr: pallet_mmr::{Pallet, Storage},
		MmrLeaf: pallet_beefy_mmr::{Pallet, Storage},

		// Rialto bridge modules.
		BridgeRelayers: pallet_bridge_relayers::{Pallet, Call, Storage, Event<T>},
		BridgeRialtoGrandpa: pallet_bridge_grandpa::{Pallet, Call, Storage},
		BridgeRialtoMessages: pallet_bridge_messages::{Pallet, Call, Storage, Event<T>, Config<T>},

		// Westend bridge modules.
		BridgeWestendGrandpa: pallet_bridge_grandpa::<Instance1>::{Pallet, Call, Config<T>, Storage},
		BridgeWestendParachains: pallet_bridge_parachains::<Instance1>::{Pallet, Call, Storage, Event<T>},

		// RialtoParachain bridge modules.
		BridgeRialtoParachains: pallet_bridge_parachains::{Pallet, Call, Storage, Event<T>},
		BridgeRialtoParachainMessages: pallet_bridge_messages::<Instance1>::{Pallet, Call, Storage, Event<T>, Config<T>},

		// Pallet for sending XCM.
		XcmPallet: pallet_xcm::{Pallet, Call, Storage, Event<T>, Origin, Config} = 99,
	}
);

generate_bridge_reject_obsolete_headers_and_messages! {
	RuntimeCall, AccountId,
	// Grandpa
	BridgeRialtoGrandpa, BridgeWestendGrandpa,
	// Parachains
	BridgeRialtoParachains,
	//Messages
	BridgeRialtoMessages, BridgeRialtoParachainMessages
}

/// The address format for describing accounts.
pub type Address = AccountId;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, Hashing>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	BridgeRejectObsoleteHeadersAndMessages,
);
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, RuntimeCall, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
>;

impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block);
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
		fn account_nonce(account: AccountId) -> Index {
			System::account_nonce(account)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities().to_vec()
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<
		Block,
		Balance,
	> for Runtime {
		fn query_info(uxt: <Block as BlockT>::Extrinsic, len: u32) -> RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(uxt: <Block as BlockT>::Extrinsic, len: u32) -> FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, sp_core::crypto::KeyTypeId)>> {
			SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl beefy_primitives::BeefyApi<Block> for Runtime {
		fn validator_set() -> Option<ValidatorSet<BeefyId>> {
			Beefy::validator_set()
		}
	}

	impl sp_mmr_primitives::MmrApi<Block, mmr::Hash, BlockNumber> for Runtime {
		fn generate_proof(block_number: BlockNumber)
			-> Result<(EncodableOpaqueLeaf, MmrProof<mmr::Hash>), MmrError>
		{
			Mmr::generate_batch_proof(vec![block_number])
				.and_then(|(leaves, proof)| Ok((
					mmr::EncodableOpaqueLeaf::from_leaf(&leaves[0]),
					mmr::BatchProof::into_single_leaf_proof(proof)?
				)))
		}

		fn verify_proof(leaf: EncodableOpaqueLeaf, proof: MmrProof<mmr::Hash>)
			-> Result<(), MmrError>
		{
			let leaf: mmr::Leaf = leaf
				.into_opaque_leaf()
				.try_decode()
				.ok_or(MmrError::Verify)?;
			Mmr::verify_leaves(vec![leaf], mmr::Proof::into_batch_proof(proof))
		}

		fn verify_proof_stateless(
			root: mmr::Hash,
			leaf: EncodableOpaqueLeaf,
			proof: MmrProof<mmr::Hash>
		) -> Result<(), MmrError> {
			let node = DataOrHash::Data(leaf.into_opaque_leaf());
			pallet_mmr::verify_leaves_proof::<mmr::Hashing, _>(
				root,
				vec![node],
				pallet_mmr::primitives::Proof::into_batch_proof(proof),
			)
		}

		fn mmr_root() -> Result<mmr::Hash, MmrError> {
			Ok(Mmr::mmr_root())
		}

		fn generate_batch_proof(block_numbers: Vec<BlockNumber>)
			-> Result<(Vec<mmr::EncodableOpaqueLeaf>, mmr::BatchProof<mmr::Hash>), mmr::Error>
		{
			Mmr::generate_batch_proof(block_numbers)
				.map(|(leaves, proof)| (leaves.into_iter().map(|leaf| mmr::EncodableOpaqueLeaf::from_leaf(&leaf)).collect(), proof))
		}

		fn generate_historical_batch_proof(
			block_numbers: Vec<BlockNumber>,
			best_known_block_number: BlockNumber
		) -> Result<(Vec<mmr::EncodableOpaqueLeaf>, mmr::BatchProof<mmr::Hash>), mmr::Error> {
			Mmr::generate_historical_batch_proof(block_numbers, best_known_block_number).map(
				|(leaves, proof)| {
					(
						leaves
							.into_iter()
							.map(|leaf| mmr::EncodableOpaqueLeaf::from_leaf(&leaf))
							.collect(),
						proof,
					)
				},
			)
		}

		fn verify_batch_proof(leaves: Vec<mmr::EncodableOpaqueLeaf>, proof: mmr::BatchProof<mmr::Hash>)
			-> Result<(), mmr::Error>
		{
			let leaves = leaves.into_iter().map(|leaf|
				leaf.into_opaque_leaf()
				.try_decode()
				.ok_or(mmr::Error::Verify)).collect::<Result<Vec<mmr::Leaf>, mmr::Error>>()?;
			Mmr::verify_leaves(leaves, proof)
		}

		fn verify_batch_proof_stateless(
			root: mmr::Hash,
			leaves: Vec<mmr::EncodableOpaqueLeaf>,
			proof: mmr::BatchProof<mmr::Hash>
		) -> Result<(), mmr::Error> {
			let nodes = leaves.into_iter().map(|leaf|mmr::DataOrHash::Data(leaf.into_opaque_leaf())).collect();
			pallet_mmr::verify_leaves_proof::<mmr::Hashing, _>(root, nodes, proof)
		}
	}

	impl fg_primitives::GrandpaApi<Block> for Runtime {
		fn current_set_id() -> fg_primitives::SetId {
			Grandpa::current_set_id()
		}

		fn grandpa_authorities() -> GrandpaAuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: fg_primitives::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Grandpa::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
			)
		}

		fn generate_key_ownership_proof(
			_set_id: fg_primitives::SetId,
			_authority_id: GrandpaId,
		) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
			// NOTE: this is the only implementation possible since we've
			// defined our key owner proof type as a bottom type (i.e. a type
			// with no values).
			None
		}
	}

	impl bp_rialto::RialtoFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_rialto::Hash, bp_rialto::BlockNumber>> {
			BridgeRialtoGrandpa::best_finalized().map(|header| header.id())
		}
	}

	impl bp_westend::WestendFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_westend::Hash, bp_westend::BlockNumber>> {
			BridgeWestendGrandpa::best_finalized().map(|header| header.id())
		}
	}

	impl bp_westend::WestmintFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_westend::Hash, bp_westend::BlockNumber>> {
			// the parachains finality pallet is never decoding parachain heads, so it is
			// only done in the integration code
			use bp_westend::WESTMINT_PARACHAIN_ID;
			let encoded_head = pallet_bridge_parachains::Pallet::<
				Runtime,
				WithWestendParachainsInstance,
			>::best_parachain_head(WESTMINT_PARACHAIN_ID.into())?;
			let head = bp_westend::Header::decode(&mut &encoded_head.0[..]).ok()?;
			Some(head.id())
		}
	}

	impl bp_rialto_parachain::RialtoParachainFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_rialto::Hash, bp_rialto::BlockNumber>> {
			// the parachains finality pallet is never decoding parachain heads, so it is
			// only done in the integration code
			let encoded_head = pallet_bridge_parachains::Pallet::<
				Runtime,
				WithRialtoParachainsInstance,
			>::best_parachain_head(bp_rialto_parachain::RIALTO_PARACHAIN_ID.into())?;
			let head = bp_rialto_parachain::Header::decode(&mut &encoded_head.0[..]).ok()?;
			Some(head.id())
		}
	}

	impl bp_rialto::ToRialtoOutboundLaneApi<Block, Balance, ToRialtoMessagePayload> for Runtime {
		fn estimate_message_delivery_and_dispatch_fee(
			_lane_id: bp_messages::LaneId,
			payload: ToRialtoMessagePayload,
			rialto_to_this_conversion_rate: Option<FixedU128>,
		) -> Option<Balance> {
			estimate_message_dispatch_and_delivery_fee::<WithRialtoMessageBridge>(
				&payload,
				WithRialtoMessageBridge::RELAYER_FEE_PERCENT,
				rialto_to_this_conversion_rate,
			).ok()
		}

		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails<Balance>> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				WithRialtoMessagesInstance,
			>(lane, begin, end)
		}
	}

	impl bp_rialto::FromRialtoInboundLaneApi<Block, bp_rialto::Balance> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails<bp_rialto::Balance>)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				WithRialtoMessagesInstance,
			>(lane, messages)
		}
	}

	impl bp_rialto_parachain::ToRialtoParachainOutboundLaneApi<Block, Balance, ToRialtoParachainMessagePayload> for Runtime {
		fn estimate_message_delivery_and_dispatch_fee(
			_lane_id: bp_messages::LaneId,
			payload: ToRialtoParachainMessagePayload,
			rialto_parachain_to_this_conversion_rate: Option<FixedU128>,
		) -> Option<Balance> {
			estimate_message_dispatch_and_delivery_fee::<WithRialtoParachainMessageBridge>(
				&payload,
				WithRialtoParachainMessageBridge::RELAYER_FEE_PERCENT,
				rialto_parachain_to_this_conversion_rate,
			).ok()
		}

		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails<Balance>> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				WithRialtoParachainMessagesInstance,
			>(lane, begin, end)
		}
	}

	impl bp_rialto_parachain::FromRialtoParachainInboundLaneApi<Block, bp_rialto_parachain::Balance> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails<bp_rialto_parachain::Balance>)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				WithRialtoParachainMessagesInstance,
			>(lane, messages)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{list_benchmark, Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;

			use pallet_bridge_messages::benchmarking::Pallet as MessagesBench;
			use pallet_bridge_parachains::benchmarking::Pallet as ParachainsBench;

			let mut list = Vec::<BenchmarkList>::new();

			list_benchmark!(list, extra, pallet_bridge_messages, MessagesBench::<Runtime, WithRialtoMessagesInstance>);
			list_benchmark!(list, extra, pallet_bridge_grandpa, BridgeRialtoGrandpa);
			list_benchmark!(list, extra, pallet_bridge_parachains, ParachainsBench::<Runtime, WithRialtoMessagesInstance>);
			list_benchmark!(list, extra, pallet_bridge_relayers, BridgeRelayers);

			let storage_info = AllPalletsWithSystem::storage_info();

			return (list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig,
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, TrackedStorageKey, add_benchmark};

			let whitelist: Vec<TrackedStorageKey> = vec![
				// Block Number
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
				// Execution Phase
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
				// Event Count
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
				// System Events
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
				// Caller 0 Account
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da946c154ffd9992e395af90b5b13cc6f295c77033fce8a9045824a6690bbf99c6db269502f0a8d1d2a008542d5690a0749").to_vec().into(),
			];

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);

			use bridge_runtime_common::messages_benchmarking::{prepare_message_delivery_proof, prepare_message_proof, prepare_outbound_message};
			use bridge_runtime_common::messages;
			use pallet_bridge_messages::benchmarking::{
				Pallet as MessagesBench,
				Config as MessagesConfig,
				MessageDeliveryProofParams,
				MessageParams,
				MessageProofParams,
			};
			use pallet_bridge_parachains::benchmarking::{
				Pallet as ParachainsBench,
				Config as ParachainsConfig,
			};
			use rialto_messages::WithRialtoMessageBridge;

			impl MessagesConfig<WithRialtoMessagesInstance> for Runtime {
				fn maximal_message_size() -> u32 {
					messages::source::maximal_message_size::<WithRialtoMessageBridge>()
				}

				fn bridged_relayer_id() -> Self::InboundRelayer {
					[0u8; 32].into()
				}

				fn account_balance(account: &Self::AccountId) -> Self::OutboundMessageFee {
					pallet_balances::Pallet::<Runtime>::free_balance(account)
				}

				fn endow_account(account: &Self::AccountId) {
					pallet_balances::Pallet::<Runtime>::make_free_balance_be(
						account,
						Balance::MAX / 100,
					);
				}

				fn prepare_outbound_message(
					params: MessageParams<Self::AccountId>,
				) -> (rialto_messages::ToRialtoMessagePayload, Balance) {
					(prepare_outbound_message::<WithRialtoMessageBridge>(params), Self::message_fee())
				}

				fn prepare_message_proof(
					params: MessageProofParams,
				) -> (rialto_messages::FromRialtoMessagesProof, Weight) {
					prepare_message_proof::<Runtime, (), (), WithRialtoMessageBridge, bp_rialto::Header, bp_rialto::Hasher>(
						params,
					)
				}

				fn prepare_message_delivery_proof(
					params: MessageDeliveryProofParams<Self::AccountId>,
				) -> rialto_messages::ToRialtoMessagesDeliveryProof {
					prepare_message_delivery_proof::<Runtime, (), WithRialtoMessageBridge, bp_rialto::Header, bp_rialto::Hasher>(
						params,
					)
				}

				fn is_message_dispatched(_nonce: bp_messages::MessageNonce) -> bool {
					true
				}
			}

			impl ParachainsConfig<WithRialtoParachainsInstance> for Runtime {
				fn prepare_parachain_heads_proof(
					parachains: &[bp_polkadot_core::parachains::ParaId],
					parachain_head_size: u32,
					proof_size: bp_runtime::StorageProofSize,
				) -> (
					pallet_bridge_parachains::RelayBlockNumber,
					pallet_bridge_parachains::RelayBlockHash,
					bp_polkadot_core::parachains::ParaHeadsProof,
					Vec<(bp_polkadot_core::parachains::ParaId, bp_polkadot_core::parachains::ParaHash)>,
				) {
					bridge_runtime_common::parachains_benchmarking::prepare_parachain_heads_proof::<Runtime, WithRialtoParachainsInstance>(
						parachains,
						parachain_head_size,
						proof_size,
					)
				}
			}

			add_benchmark!(
				params,
				batches,
				pallet_bridge_messages,
				MessagesBench::<Runtime, WithRialtoMessagesInstance>
			);
			add_benchmark!(params, batches, pallet_bridge_grandpa, BridgeRialtoGrandpa);
			add_benchmark!(
				params,
				batches,
				pallet_bridge_parachains,
				ParachainsBench::<Runtime, WithRialtoParachainsInstance>
			);
			add_benchmark!(params, batches, pallet_bridge_relayers, BridgeRelayers);

			Ok(batches)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn call_size() {
		const BRIDGES_PALLETS_MAX_CALL_SIZE: usize = 200;
		assert!(
			core::mem::size_of::<pallet_bridge_grandpa::Call<Runtime>>() <=
				BRIDGES_PALLETS_MAX_CALL_SIZE
		);
		assert!(
			core::mem::size_of::<pallet_bridge_messages::Call<Runtime>>() <=
				BRIDGES_PALLETS_MAX_CALL_SIZE
		);
		const MAX_CALL_SIZE: usize = 230; // value from polkadot-runtime tests
		assert!(core::mem::size_of::<RuntimeCall>() <= MAX_CALL_SIZE);
	}
}
