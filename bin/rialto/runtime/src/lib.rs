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

//! The Rialto runtime. This can be compiled with `#[no_std]`, ready for Wasm.

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

pub mod millau_messages;
pub mod parachains;
pub mod xcm_config;

use bp_runtime::HeaderId;
use pallet_grandpa::{
	fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use pallet_transaction_payment::{FeeDetails, Multiplier, RuntimeDispatchInfo};
use sp_api::impl_runtime_apis;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_beefy::{crypto::AuthorityId as BeefyId, mmr::MmrLeafVersion, ValidatorSet};
use sp_core::OpaqueMetadata;
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{AccountIdLookup, Block as BlockT, Keccak256, NumberFor, OpaqueKeys},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, FixedPointNumber, Perquintill,
};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// A few exports that help ease life for downstream crates.
pub use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	parameter_types,
	traits::{
		ConstU32, ConstU64, ConstU8, Currency, ExistenceRequirement, Imbalance, KeyOwnerProofSystem,
	},
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, IdentityFee, RuntimeDbWeight, Weight},
	StorageValue,
};

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;
pub use pallet_bridge_beefy::Call as BridgeBeefyCall;
pub use pallet_bridge_grandpa::Call as BridgeGrandpaCall;
pub use pallet_bridge_messages::Call as MessagesCall;
pub use pallet_sudo::Call as SudoCall;
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_xcm::Call as XcmCall;

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
pub use sp_runtime::{Perbill, Permill};

/// An index to a block.
pub type BlockNumber = bp_rialto::BlockNumber;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = bp_rialto::Signature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = bp_rialto::AccountId;

/// The type for looking up accounts. We don't expect more than 4 billion of them, but you
/// never know...
pub type AccountIndex = u32;

/// Balance of an account.
pub type Balance = bp_rialto::Balance;

/// Nonce of a transaction in the chain.
pub type Nonce = bp_rialto::Nonce;

/// A hash of some data used by the chain.
pub type Hash = bp_rialto::Hash;

/// Hashing algorithm used by the chain.
pub type Hashing = bp_rialto::Hasher;

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
		pub babe: Babe,
		pub grandpa: Grandpa,
		pub beefy: Beefy,
		pub para_validator: Initializer,
		pub para_assignment: SessionInfo,
		pub authority_discovery: AuthorityDiscovery,
	}
}

/// This runtime version.
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("rialto-runtime"),
	impl_name: create_runtime_str!("rialto-runtime"),
	authoring_version: 1,
	spec_version: 1,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
	state_version: 1,
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
	pub const SS58Prefix: u8 = 48;
}

impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = frame_support::traits::Everything;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// The index type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = Hashing;
	/// The header type.
	type Block = Block;
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
	type BlockWeights = bp_rialto::BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = bp_rialto::BlockLength;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = DbWeight;
	/// The designated SS58 prefix of this chain.
	type SS58Prefix = SS58Prefix;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

/// The BABE epoch configuration at genesis.
pub const BABE_GENESIS_EPOCH_CONFIG: sp_consensus_babe::BabeEpochConfiguration =
	sp_consensus_babe::BabeEpochConfiguration {
		c: bp_rialto::time_units::PRIMARY_PROBABILITY,
		allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryVRFSlots,
	};

parameter_types! {
	pub const EpochDuration: u64 = bp_rialto::EPOCH_DURATION_IN_SLOTS as u64;
	pub const ExpectedBlockTime: bp_rialto::Moment = bp_rialto::time_units::MILLISECS_PER_BLOCK;
}

impl pallet_babe::Config for Runtime {
	type EpochDuration = EpochDuration;
	type ExpectedBlockTime = ExpectedBlockTime;
	// session module is the trigger
	type EpochChangeTrigger = pallet_babe::ExternalTrigger;

	type DisabledValidators = ();

	type WeightInfo = ();

	type MaxAuthorities = ConstU32<10>;

	// equivocation related configuration - we don't expect any equivocations in our testnets
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();

	type MaxNominators = ConstU32<256>;
}

impl pallet_beefy::Config for Runtime {
	type BeefyId = BeefyId;
	type MaxAuthorities = ConstU32<10>;
	type MaxSetIdSessionEntries = ConstU64<0>;
	type OnNewValidatorSet = MmrLeaf;
	type WeightInfo = ();
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
	type MaxNominators = ConstU32<256>;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<10>;
	type MaxSetIdSessionEntries = ConstU64<0>;
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
	type MaxNominators = ConstU32<256>;
}

impl pallet_mmr::Config for Runtime {
	const INDEXING_PREFIX: &'static [u8] = b"mmr";
	type Hashing = Keccak256;
	type LeafData = pallet_beefy_mmr::Pallet<Runtime>;
	type OnNewRoot = pallet_beefy_mmr::DepositBeefyDigest<Runtime>;
	type WeightInfo = ();
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

impl sp_consensus_beefy::mmr::BeefyDataProvider<()> for BeefyDummyDataProvider {
	fn extra_data() {}
}

impl pallet_beefy_mmr::Config for Runtime {
	type LeafVersion = LeafVersion;
	type BeefyAuthorityToMerkleLeaf = pallet_beefy_mmr::BeefyEcdsaToEthereum;
	type LeafExtra = ();
	type BeefyDataProvider = BeefyDummyDataProvider;
}

parameter_types! {
	pub const MinimumPeriod: u64 = bp_rialto::SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the UNIX epoch.
	type Moment = bp_rialto::Moment;
	type OnTimestampSet = Babe;
	type MinimumPeriod = MinimumPeriod;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

parameter_types! {
	pub const ExistentialDeposit: bp_rialto::Balance = 500;
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
	// For weight estimation, we assume that the most locks on an individual account will be 50.
	// This number may need to be adjusted in the future if this assumption no longer holds true.
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type FreezeIdentifier = ();
	type MaxHolds = ConstU32<0>;
	type MaxFreezes = ConstU32<0>;
}

parameter_types! {
	pub const TransactionBaseFee: Balance = 0;
	pub const TransactionByteFee: Balance = 1;
	// values for following parameters are copied from polkadot repo, but it is fine
	// not to sync them - we're not going to make Rialto a full copy of one of Polkadot-like chains
	pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
	pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(3, 100_000);
	pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000u128);
	pub MaximumMultiplier: Multiplier = sp_runtime::traits::Bounded::max_value();
}

impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = bp_rialto::WeightToFee;
	type LengthToFee = bp_rialto::WeightToFee;
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
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ();
	type ShouldEndSession = Babe;
	type NextSessionRotation = Babe;
	type SessionManager = pallet_shift_session_manager::Pallet<Runtime>;
	type SessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

impl pallet_authority_discovery::Config for Runtime {
	type MaxAuthorities = ConstU32<10>;
}

impl pallet_bridge_relayers::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure =
		bp_relayers::PayRewardFromAccount<pallet_balances::Pallet<Runtime>, AccountId>;
	type StakeAndSlash = ();
	type WeightInfo = ();
}

pub type MillauGrandpaInstance = ();
impl pallet_bridge_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_millau::Millau;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = ConstU32<{ bp_millau::DAYS as u32 }>;
	type WeightInfo = pallet_bridge_grandpa::weights::BridgeWeight<Runtime>;
}

impl pallet_shift_session_manager::Config for Runtime {}

parameter_types! {
	pub const MaxMessagesToPruneAtOnce: bp_messages::MessageNonce = 8;
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_millau::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_millau::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	pub const RootAccountForPayments: Option<AccountId> = None;
	pub const BridgedChainId: bp_runtime::ChainId = bp_runtime::MILLAU_CHAIN_ID;
	pub ActiveOutboundLanes: &'static [bp_messages::LaneId] = &[millau_messages::XCM_LANE];
}

/// Instance of the messages pallet used to relay messages to/from Millau chain.
pub type WithMillauMessagesInstance = ();

impl pallet_bridge_messages::Config<WithMillauMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_bridge_messages::weights::BridgeWeight<Runtime>;
	type ActiveOutboundLanes = ActiveOutboundLanes;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = crate::millau_messages::ToMillauMaximalOutboundPayloadSize;
	type OutboundPayload = crate::millau_messages::ToMillauMessagePayload;

	type InboundPayload = crate::millau_messages::FromMillauMessagePayload;
	type InboundRelayer = bp_millau::AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = crate::millau_messages::MillauAsTargetHeaderChain;
	type LaneMessageVerifier = crate::millau_messages::ToMillauMessageVerifier;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithMillauMessagesInstance,
		frame_support::traits::ConstU128<100_000>,
	>;

	type SourceHeaderChain = crate::millau_messages::MillauAsSourceHeaderChain;
	type MessageDispatch = crate::millau_messages::FromMillauMessageDispatch;
	type BridgedChainId = BridgedChainId;
}

pub type MillauBeefyInstance = ();
impl pallet_bridge_beefy::Config<MillauBeefyInstance> for Runtime {
	type MaxRequests = frame_support::traits::ConstU32<16>;
	type CommitmentsToKeep = frame_support::traits::ConstU32<8>;
	type BridgedChain = bp_millau::Millau;
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Sudo: pallet_sudo::{Pallet, Call, Config<T>, Storage, Event<T>},

		// Must be before session.
		Babe: pallet_babe::{Pallet, Call, Storage, Config<T>, ValidateUnsigned},

		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>},

		// Consensus support.
		AuthorityDiscovery: pallet_authority_discovery::{Pallet, Config<T>},
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
		Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config<T>, Event},
		ShiftSessionManager: pallet_shift_session_manager::{Pallet},

		// BEEFY Bridges support.
		Beefy: pallet_beefy::{Pallet, Storage, Config<T>},
		Mmr: pallet_mmr::{Pallet, Storage},
		MmrLeaf: pallet_beefy_mmr::{Pallet, Storage},

		// Millau bridge modules.
		BridgeRelayers: pallet_bridge_relayers::{Pallet, Call, Storage, Event<T>},
		BridgeMillauGrandpa: pallet_bridge_grandpa::{Pallet, Call, Storage, Event<T>},
		BridgeMillauMessages: pallet_bridge_messages::{Pallet, Call, Storage, Event<T>, Config<T>},

		// Millau bridge modules (BEEFY based).
		BridgeMillauBeefy: pallet_bridge_beefy::{Pallet, Call, Storage},

		// Parachain modules.
		ParachainsOrigin: polkadot_runtime_parachains::origin::{Pallet, Origin},
		Configuration: polkadot_runtime_parachains::configuration::{Pallet, Call, Storage, Config<T>},
		Shared: polkadot_runtime_parachains::shared::{Pallet, Call, Storage},
		Inclusion: polkadot_runtime_parachains::inclusion::{Pallet, Call, Storage, Event<T>},
		ParasInherent: polkadot_runtime_parachains::paras_inherent::{Pallet, Call, Storage, Inherent},
		Scheduler: polkadot_runtime_parachains::scheduler::{Pallet, Storage},
		Paras: polkadot_runtime_parachains::paras::{Pallet, Call, Storage, Event, Config<T>, ValidateUnsigned},
		Initializer: polkadot_runtime_parachains::initializer::{Pallet, Call, Storage},
		Dmp: polkadot_runtime_parachains::dmp::{Pallet, Storage},
		Hrmp: polkadot_runtime_parachains::hrmp::{Pallet, Call, Storage, Event<T>, Config<T>},
		SessionInfo: polkadot_runtime_parachains::session_info::{Pallet, Storage},
		ParaSessionInfo: polkadot_runtime_parachains::session_info::{Pallet, Storage},
		ParasDisputes: polkadot_runtime_parachains::disputes::{Pallet, Call, Storage, Event<T>},
		ParasSlashing: polkadot_runtime_parachains::disputes::slashing::{Pallet, Call, Storage, ValidateUnsigned},
		MessageQueue: pallet_message_queue::{Pallet, Call, Storage, Event<T>},

		// Parachain Onboarding Pallets
		Registrar: polkadot_runtime_common::paras_registrar::{Pallet, Call, Storage, Event<T>},
		Slots: polkadot_runtime_common::slots::{Pallet, Call, Storage, Event<T>},
		ParasSudoWrapper: polkadot_runtime_common::paras_sudo_wrapper::{Pallet, Call},

		// Pallet for sending XCM.
		XcmPallet: pallet_xcm::{Pallet, Call, Storage, Event<T>, Origin, Config<T>} = 99,
	}
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
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

/// MMR helper types.
mod mmr {
	use super::Runtime;
	pub use pallet_mmr::primitives::*;

	pub type Leaf = <<Runtime as pallet_mmr::Config>::LeafData as LeafDataProvider>::LeafData;
	pub type Hashing = <Runtime as pallet_mmr::Config>::Hashing;
	pub type Hash = <Hashing as sp_runtime::traits::Hash>::Output;
}

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

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
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

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	impl sp_consensus_beefy::BeefyApi<Block> for Runtime {
		fn beefy_genesis() -> Option<BlockNumber> {
			Beefy::genesis_block()
		}

		fn validator_set() -> Option<ValidatorSet<BeefyId>> {
			Beefy::validator_set()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_beefy::EquivocationProof<
				NumberFor<Block>,
				sp_consensus_beefy::crypto::AuthorityId,
				sp_consensus_beefy::crypto::Signature
			>,
			_key_owner_proof: sp_consensus_beefy::OpaqueKeyOwnershipProof,
		) -> Option<()> { None }

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_beefy::ValidatorSetId,
			_authority_id: sp_consensus_beefy::crypto::AuthorityId,
		) -> Option<sp_consensus_beefy::OpaqueKeyOwnershipProof> { None }
	}

	impl pallet_mmr::primitives::MmrApi<
		Block,
		mmr::Hash,
		BlockNumber,
	> for Runtime {
		fn mmr_root() -> Result<mmr::Hash, mmr::Error> {
			Ok(Mmr::mmr_root())
		}

		fn mmr_leaf_count() -> Result<mmr::LeafIndex, mmr::Error> {
			Ok(Mmr::mmr_leaves())
		}

		fn generate_proof(
			block_numbers: Vec<BlockNumber>,
			best_known_block_number: Option<BlockNumber>,
		) -> Result<(Vec<mmr::EncodableOpaqueLeaf>, mmr::Proof<mmr::Hash>), mmr::Error> {
			Mmr::generate_proof(block_numbers, best_known_block_number).map(
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

		fn verify_proof(leaves: Vec<mmr::EncodableOpaqueLeaf>, proof: mmr::Proof<mmr::Hash>)
			-> Result<(), mmr::Error>
		{
			let leaves = leaves.into_iter().map(|leaf|
				leaf.into_opaque_leaf()
				.try_decode()
				.ok_or(mmr::Error::Verify)).collect::<Result<Vec<mmr::Leaf>, mmr::Error>>()?;
			Mmr::verify_leaves(leaves, proof)
		}

		fn verify_proof_stateless(
			root: mmr::Hash,
			leaves: Vec<mmr::EncodableOpaqueLeaf>,
			proof: mmr::Proof<mmr::Hash>
		) -> Result<(), mmr::Error> {
			let nodes = leaves.into_iter().map(|leaf|mmr::DataOrHash::Data(leaf.into_opaque_leaf())).collect();
			pallet_mmr::verify_leaves_proof::<mmr::Hashing, _>(root, nodes, proof)
		}
	}

	impl bp_millau::MillauFinalityApi<Block> for Runtime {
		fn best_finalized() -> Option<HeaderId<bp_millau::Hash, bp_millau::BlockNumber>> {
			BridgeMillauGrandpa::best_finalized()
		}

		fn synced_headers_grandpa_info(
		) -> Vec<bp_header_chain::HeaderGrandpaInfo<bp_millau::Header>> {
			BridgeMillauGrandpa::synced_headers_grandpa_info()
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

	impl sp_consensus_babe::BabeApi<Block> for Runtime {
		fn configuration() -> sp_consensus_babe::BabeConfiguration {
			// The choice of `c` parameter (where `1 - c` represents the
			// probability of a slot being empty), is done in accordance to the
			// slot duration and expected target block time, for safely
			// resisting network delays of maximum two seconds.
			// <https://research.web3.foundation/en/latest/polkadot/BABE/Babe/#6-practical-results>
			sp_consensus_babe::BabeConfiguration {
				slot_duration: Babe::slot_duration(),
				epoch_length: EpochDuration::get(),
				c: BABE_GENESIS_EPOCH_CONFIG.c,
				authorities: Babe::authorities().to_vec(),
				randomness: Babe::randomness(),
				allowed_slots: BABE_GENESIS_EPOCH_CONFIG.allowed_slots,
			}
		}

		fn current_epoch_start() -> sp_consensus_babe::Slot {
			Babe::current_epoch_start()
		}

		fn current_epoch() -> sp_consensus_babe::Epoch {
			Babe::current_epoch()
		}

		fn next_epoch() -> sp_consensus_babe::Epoch {
			Babe::next_epoch()
		}

		fn generate_key_ownership_proof(
			_slot: sp_consensus_babe::Slot,
			_authority_id: sp_consensus_babe::AuthorityId,
		) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
			None
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
			key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Babe::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
			)
		}
	}

	impl polkadot_primitives::runtime_api::ParachainHost<Block, Hash, BlockNumber> for Runtime {
		fn validators() -> Vec<polkadot_primitives::v5::ValidatorId> {
			polkadot_runtime_parachains::runtime_api_impl::v5::validators::<Runtime>()
		}

		fn validator_groups() -> (Vec<Vec<polkadot_primitives::v5::ValidatorIndex>>, polkadot_primitives::v5::GroupRotationInfo<BlockNumber>) {
			polkadot_runtime_parachains::runtime_api_impl::v5::validator_groups::<Runtime>()
		}

		fn availability_cores() -> Vec<polkadot_primitives::v5::CoreState<Hash, BlockNumber>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::availability_cores::<Runtime>()
		}

		fn persisted_validation_data(para_id: polkadot_primitives::v5::Id, assumption: polkadot_primitives::v5::OccupiedCoreAssumption)
			-> Option<polkadot_primitives::v5::PersistedValidationData<Hash, BlockNumber>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::persisted_validation_data::<Runtime>(para_id, assumption)
		}

		fn assumed_validation_data(
			para_id: polkadot_primitives::v5::Id,
			expected_persisted_validation_data_hash: Hash,
		) -> Option<(polkadot_primitives::v5::PersistedValidationData<Hash, BlockNumber>, polkadot_primitives::v5::ValidationCodeHash)> {
			polkadot_runtime_parachains::runtime_api_impl::v5::assumed_validation_data::<Runtime>(
				para_id,
				expected_persisted_validation_data_hash,
			)
		}

		fn check_validation_outputs(
			para_id: polkadot_primitives::v5::Id,
			outputs: polkadot_primitives::v5::CandidateCommitments,
		) -> bool {
			polkadot_runtime_parachains::runtime_api_impl::v5::check_validation_outputs::<Runtime>(para_id, outputs)
		}

		fn session_index_for_child() -> polkadot_primitives::v5::SessionIndex {
			polkadot_runtime_parachains::runtime_api_impl::v5::session_index_for_child::<Runtime>()
		}

		fn validation_code(para_id: polkadot_primitives::v5::Id, assumption: polkadot_primitives::v5::OccupiedCoreAssumption)
			-> Option<polkadot_primitives::v5::ValidationCode> {
			polkadot_runtime_parachains::runtime_api_impl::v5::validation_code::<Runtime>(para_id, assumption)
		}

		fn candidate_pending_availability(para_id: polkadot_primitives::v5::Id) -> Option<polkadot_primitives::v5::CommittedCandidateReceipt<Hash>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::candidate_pending_availability::<Runtime>(para_id)
		}

		fn candidate_events() -> Vec<polkadot_primitives::v5::CandidateEvent<Hash>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::candidate_events::<Runtime, _>(|ev| {
				match ev {
					RuntimeEvent::Inclusion(ev) => {
						Some(ev)
					}
					_ => None,
				}
			})
		}

		fn session_info(index: polkadot_primitives::v5::SessionIndex) -> Option<polkadot_primitives::v5::SessionInfo> {
			polkadot_runtime_parachains::runtime_api_impl::v5::session_info::<Runtime>(index)
		}

		fn dmq_contents(recipient: polkadot_primitives::v5::Id) -> Vec<polkadot_primitives::v5::InboundDownwardMessage<BlockNumber>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::dmq_contents::<Runtime>(recipient)
		}

		fn inbound_hrmp_channels_contents(
			recipient: polkadot_primitives::v5::Id
		) -> BTreeMap<polkadot_primitives::v5::Id, Vec<polkadot_primitives::v5::InboundHrmpMessage<BlockNumber>>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::inbound_hrmp_channels_contents::<Runtime>(recipient)
		}

		fn validation_code_by_hash(hash: polkadot_primitives::v5::ValidationCodeHash) -> Option<polkadot_primitives::v5::ValidationCode> {
			polkadot_runtime_parachains::runtime_api_impl::v5::validation_code_by_hash::<Runtime>(hash)
		}

		fn on_chain_votes() -> Option<polkadot_primitives::v5::ScrapedOnChainVotes<Hash>> {
			polkadot_runtime_parachains::runtime_api_impl::v5::on_chain_votes::<Runtime>()
		}

		fn submit_pvf_check_statement(stmt: polkadot_primitives::v5::PvfCheckStatement, signature: polkadot_primitives::v5::ValidatorSignature) {
			polkadot_runtime_parachains::runtime_api_impl::v5::submit_pvf_check_statement::<Runtime>(stmt, signature)
		}

		fn pvfs_require_precheck() -> Vec<polkadot_primitives::v5::ValidationCodeHash> {
			polkadot_runtime_parachains::runtime_api_impl::v5::pvfs_require_precheck::<Runtime>()
		}

		fn validation_code_hash(para_id: polkadot_primitives::v5::Id, assumption: polkadot_primitives::v5::OccupiedCoreAssumption)
			-> Option<polkadot_primitives::v5::ValidationCodeHash>
		{
			polkadot_runtime_parachains::runtime_api_impl::v5::validation_code_hash::<Runtime>(para_id, assumption)
		}

		fn disputes() -> Vec<(polkadot_primitives::v5::SessionIndex, polkadot_primitives::v5::CandidateHash, polkadot_primitives::v5::DisputeState<BlockNumber>)> {
			polkadot_runtime_parachains::runtime_api_impl::v5::get_session_disputes::<Runtime>()
		}

		fn session_executor_params(session_index: polkadot_primitives::v5::SessionIndex) -> Option<polkadot_primitives::v5::ExecutorParams> {
			polkadot_runtime_parachains::runtime_api_impl::v5::session_executor_params::<Runtime>(session_index)
		}

		fn unapplied_slashes(
		) -> Vec<(polkadot_primitives::v5::SessionIndex, polkadot_primitives::v5::CandidateHash, polkadot_primitives::v5::slashing::PendingSlashes)> {
			polkadot_runtime_parachains::runtime_api_impl::v5::unapplied_slashes::<Runtime>()
		}

		fn key_ownership_proof(
			_validator_id: polkadot_primitives::v5::ValidatorId,
		) -> Option<polkadot_primitives::v5::slashing::OpaqueKeyOwnershipProof> {
			unimplemented!("Not used at Rialto")
		}

		fn submit_report_dispute_lost(
			dispute_proof: polkadot_primitives::v5::slashing::DisputeProof,
			key_ownership_proof: polkadot_primitives::v5::slashing::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			polkadot_runtime_parachains::runtime_api_impl::v5::submit_unsigned_slashing_report::<Runtime>(
				dispute_proof,
				key_ownership_proof,
			)
		}
	}

	impl sp_authority_discovery::AuthorityDiscoveryApi<Block> for Runtime {
		fn authorities() -> Vec<AuthorityDiscoveryId> {
			polkadot_runtime_parachains::runtime_api_impl::v5::relevant_authority_ids::<Runtime>()
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
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
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

	impl bp_millau::ToMillauOutboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			begin: bp_messages::MessageNonce,
			end: bp_messages::MessageNonce,
		) -> Vec<bp_messages::OutboundMessageDetails> {
			bridge_runtime_common::messages_api::outbound_message_details::<
				Runtime,
				WithMillauMessagesInstance,
			>(lane, begin, end)
		}
	}

	impl bp_millau::FromMillauInboundLaneApi<Block> for Runtime {
		fn message_details(
			lane: bp_messages::LaneId,
			messages: Vec<(bp_messages::MessagePayload, bp_messages::OutboundMessageDetails)>,
		) -> Vec<bp_messages::InboundMessageDetails> {
			bridge_runtime_common::messages_api::inbound_message_details::<
				Runtime,
				WithMillauMessagesInstance,
			>(lane, messages)
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
		// Largest inner Call is `pallet_session::Call` with a size of 224 bytes. This size is a
		// result of large `SessionKeys` struct.
		// Total size of Rialto runtime Call is 232.
		const MAX_CALL_SIZE: usize = 232;
		assert!(core::mem::size_of::<RuntimeCall>() <= MAX_CALL_SIZE);
	}
}
