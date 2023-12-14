// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! The Rococo runtime for v1 parachains.

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit.
#![recursion_limit = "512"]

use pallet_nis::WithMaximumOf;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use primitives::{
	slashing,
	vstaging::{ApprovalVotingParams, NodeFeatures},
	AccountId, AccountIndex, Balance, BlockNumber, CandidateEvent, CandidateHash,
	CommittedCandidateReceipt, CoreState, DisputeState, ExecutorParams, GroupRotationInfo, Hash,
	Id as ParaId, InboundDownwardMessage, InboundHrmpMessage, Moment, Nonce,
	OccupiedCoreAssumption, PersistedValidationData, ScrapedOnChainVotes, SessionInfo, Signature,
	ValidationCode, ValidationCodeHash, ValidatorId, ValidatorIndex, PARACHAIN_KEY_TYPE_ID,
};
use runtime_common::{
	assigned_slots, auctions, claims, crowdloan, identity_migrator, impl_runtime_weights,
	impls::{
		LocatableAssetConverter, ToAuthor, VersionedLocatableAsset, VersionedMultiLocationConverter,
	},
	paras_registrar, paras_sudo_wrapper, prod_or_fast, slots, BlockHashCount, BlockLength,
	SlowAdjustingFeeUpdate,
};
use scale_info::TypeInfo;
use sp_std::{cmp::Ordering, collections::btree_map::BTreeMap, prelude::*};

use runtime_parachains::{
	assigner as parachains_assigner, assigner_on_demand as parachains_assigner_on_demand,
	assigner_parachains as parachains_assigner_parachains,
	configuration as parachains_configuration, disputes as parachains_disputes,
	disputes::slashing as parachains_slashing,
	dmp as parachains_dmp, hrmp as parachains_hrmp, inclusion as parachains_inclusion,
	inclusion::{AggregateMessageOrigin, UmpQueueId},
	initializer as parachains_initializer, origin as parachains_origin, paras as parachains_paras,
	paras_inherent as parachains_paras_inherent,
	runtime_api_impl::{
		v7 as parachains_runtime_api_impl, vstaging as parachains_staging_runtime_api_impl,
	},
	scheduler as parachains_scheduler, session_info as parachains_session_info,
	shared as parachains_shared,
};

use authority_discovery_primitives::AuthorityId as AuthorityDiscoveryId;
use beefy_primitives::{
	ecdsa_crypto::{AuthorityId as BeefyId, Signature as BeefySignature},
	mmr::{BeefyDataProvider, MmrLeafVersion},
};

use frame_support::{
	construct_runtime, derive_impl,
	genesis_builder_helper::{build_state, create_default_config},
	parameter_types,
	traits::{
		fungible::HoldConsideration, Contains, EitherOf, EitherOfDiverse, EverythingBut,
		InstanceFilter, KeyOwnerProofSystem, LinearStoragePrice, PrivilegeCmp, ProcessMessage,
		ProcessMessageError, StorageMapShim, WithdrawReasons,
	},
	weights::{ConstantMultiplier, WeightMeter},
	PalletId,
};
use frame_system::EnsureRoot;
use pallet_grandpa::{fg_primitives, AuthorityId as GrandpaId};
use pallet_identity::legacy::IdentityInfo;
use pallet_session::historical as session_historical;
use pallet_transaction_payment::{CurrencyAdapter, FeeDetails, RuntimeDispatchInfo};
use sp_core::{ConstU128, OpaqueMetadata, H256};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{
		BlakeTwo256, Block as BlockT, ConstU32, ConvertInto, Extrinsic as ExtrinsicT,
		IdentityLookup, Keccak256, OpaqueKeys, SaturatedConversion, Verify,
	},
	transaction_validity::{TransactionPriority, TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, BoundToRuntimeAppPublic, FixedU128, KeyTypeId, Perbill, Percent, Permill,
	RuntimeAppPublic, RuntimeDebug,
};
use sp_staking::SessionIndex;
#[cfg(any(feature = "std", test))]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;
use xcm::{
	latest::{InteriorMultiLocation, Junction, Junction::PalletInstance},
	VersionedMultiLocation,
};
use xcm_builder::PayOverXcm;

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;

/// Constant values used within the runtime.
use rococo_runtime_constants::{currency::*, fee::*, time::*};

// Weights used in the runtime.
mod weights;

// XCM configurations.
pub mod xcm_config;

// Implemented types.
mod impls;
use impls::ToParachainIdentityReaper;

// Governance and configurations.
pub mod governance;
use governance::{
	pallet_custom_origins, AuctionAdmin, Fellows, GeneralAdmin, LeaseAdmin, Treasurer,
	TreasurySpender,
};

#[cfg(test)]
mod tests;

mod validator_manager;

impl_runtime_weights!(rococo_runtime_constants);

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Provides the `WASM_BINARY` build with `fast-runtime` feature enabled.
///
/// This is for example useful for local test chains.
#[cfg(feature = "std")]
pub mod fast_runtime_binary {
	include!(concat!(env!("OUT_DIR"), "/fast_runtime_binary.rs"));
}

/// Runtime version (Rococo).
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("rococo"),
	impl_name: create_runtime_str!("parity-rococo-v2.0"),
	authoring_version: 0,
	spec_version: 1_005_000,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 24,
	state_version: 1,
};

/// The BABE epoch configuration at genesis.
pub const BABE_GENESIS_EPOCH_CONFIG: babe_primitives::BabeEpochConfiguration =
	babe_primitives::BabeEpochConfiguration {
		c: PRIMARY_PROBABILITY,
		allowed_slots: babe_primitives::AllowedSlots::PrimaryAndSecondaryVRFSlots,
	};

/// Native version.
#[cfg(any(feature = "std", test))]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

/// A type to identify calls to the Identity pallet. These will be filtered to prevent invocation,
/// locking the state of the pallet and preventing further updates to identities and sub-identities.
/// The locked state will be the genesis state of a new system chain and then removed from the Relay
/// Chain.
pub struct IsIdentityCall;
impl Contains<RuntimeCall> for IsIdentityCall {
	fn contains(c: &RuntimeCall) -> bool {
		matches!(c, RuntimeCall::Identity(_))
	}
}

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::RelayChainDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type BaseCallFilter = EverythingBut<IsIdentityCall>;
	type BlockWeights = BlockWeights;
	type BlockLength = BlockLength;
	type DbWeight = RocksDbWeight;
	type Nonce = Nonce;
	type Hash = Hash;
	type AccountId = AccountId;
	type Block = Block;
	type BlockHashCount = BlockHashCount;
	type Version = Version;
	type AccountData = pallet_balances::AccountData<Balance>;
	type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
		BlockWeights::get().max_block;
	pub const MaxScheduledPerBlock: u32 = 50;
	pub const NoPreimagePostponement: Option<u32> = Some(10);
}

/// Used the compare the privilege of an origin inside the scheduler.
pub struct OriginPrivilegeCmp;

impl PrivilegeCmp<OriginCaller> for OriginPrivilegeCmp {
	fn cmp_privilege(left: &OriginCaller, right: &OriginCaller) -> Option<Ordering> {
		if left == right {
			return Some(Ordering::Equal)
		}

		match (left, right) {
			// Root is greater than anything.
			(OriginCaller::system(frame_system::RawOrigin::Root), _) => Some(Ordering::Greater),
			// For every other origin we don't care, as they are not used for `ScheduleOrigin`.
			_ => None,
		}
	}
}

impl pallet_scheduler::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaximumSchedulerWeight;
	// The goal of having ScheduleOrigin include AuctionAdmin is to allow the auctions track of
	// OpenGov to schedule periodic auctions.
	type ScheduleOrigin = EitherOf<EnsureRoot<AccountId>, AuctionAdmin>;
	type MaxScheduledPerBlock = MaxScheduledPerBlock;
	type WeightInfo = weights::pallet_scheduler::WeightInfo<Runtime>;
	type OriginPrivilegeCmp = OriginPrivilegeCmp;
	type Preimages = Preimage;
}

parameter_types! {
	pub const PreimageBaseDeposit: Balance = deposit(2, 64);
	pub const PreimageByteDeposit: Balance = deposit(0, 1);
	pub const PreimageHoldReason: RuntimeHoldReason = RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage);
}

impl pallet_preimage::Config for Runtime {
	type WeightInfo = weights::pallet_preimage::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<AccountId>;
	type Consideration = HoldConsideration<
		AccountId,
		Balances,
		PreimageHoldReason,
		LinearStoragePrice<PreimageBaseDeposit, PreimageByteDeposit, Balance>,
	>;
}

parameter_types! {
	pub const ExpectedBlockTime: Moment = MILLISECS_PER_BLOCK;
	pub ReportLongevity: u64 = EpochDurationInBlocks::get() as u64 * 10;
}

impl pallet_babe::Config for Runtime {
	type EpochDuration = EpochDurationInBlocks;
	type ExpectedBlockTime = ExpectedBlockTime;
	// session module is the trigger
	type EpochChangeTrigger = pallet_babe::ExternalTrigger;
	type DisabledValidators = Session;
	type WeightInfo = ();
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<0>;
	type KeyOwnerProof = sp_session::MembershipProof;
	type EquivocationReportSystem =
		pallet_babe::EquivocationReportSystem<Self, Offences, Historical, ReportLongevity>;
}

parameter_types! {
	pub const IndexDeposit: Balance = 100 * CENTS;
}

impl pallet_indices::Config for Runtime {
	type AccountIndex = AccountIndex;
	type Currency = Balances;
	type Deposit = IndexDeposit;
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_indices::WeightInfo<Runtime>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxLocks = MaxLocks;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
	type FreezeIdentifier = ();
	type MaxFreezes = ConstU32<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type MaxHolds = ConstU32<2>;
}

parameter_types! {
	pub const TransactionByteFee: Balance = 10 * MILLICENTS;
	/// This value increases the priority of `Operational` transactions by adding
	/// a "virtual tip" that's equal to the `OperationalFeeMultiplier * final_fee`.
	pub const OperationalFeeMultiplier: u8 = 5;
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = CurrencyAdapter<Balances, ToAuthor<Runtime>>;
	type OperationalFeeMultiplier = OperationalFeeMultiplier;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}
impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = Babe;
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = weights::pallet_timestamp::WeightInfo<Runtime>;
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
	type EventHandler = ();
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct OldSessionKeys {
	pub grandpa: <Grandpa as BoundToRuntimeAppPublic>::Public,
	pub babe: <Babe as BoundToRuntimeAppPublic>::Public,
	pub im_online: pallet_im_online::sr25519::AuthorityId,
	pub para_validator: <Initializer as BoundToRuntimeAppPublic>::Public,
	pub para_assignment: <ParaSessionInfo as BoundToRuntimeAppPublic>::Public,
	pub authority_discovery: <AuthorityDiscovery as BoundToRuntimeAppPublic>::Public,
	pub beefy: <Beefy as BoundToRuntimeAppPublic>::Public,
}

impl OpaqueKeys for OldSessionKeys {
	type KeyTypeIdProviders = ();
	fn key_ids() -> &'static [KeyTypeId] {
		&[
			<<Grandpa as BoundToRuntimeAppPublic>::Public>::ID,
			<<Babe as BoundToRuntimeAppPublic>::Public>::ID,
			sp_core::crypto::key_types::IM_ONLINE,
			<<Initializer as BoundToRuntimeAppPublic>::Public>::ID,
			<<ParaSessionInfo as BoundToRuntimeAppPublic>::Public>::ID,
			<<AuthorityDiscovery as BoundToRuntimeAppPublic>::Public>::ID,
			<<Beefy as BoundToRuntimeAppPublic>::Public>::ID,
		]
	}
	fn get_raw(&self, i: KeyTypeId) -> &[u8] {
		match i {
			<<Grandpa as BoundToRuntimeAppPublic>::Public>::ID => self.grandpa.as_ref(),
			<<Babe as BoundToRuntimeAppPublic>::Public>::ID => self.babe.as_ref(),
			sp_core::crypto::key_types::IM_ONLINE => self.im_online.as_ref(),
			<<Initializer as BoundToRuntimeAppPublic>::Public>::ID => self.para_validator.as_ref(),
			<<ParaSessionInfo as BoundToRuntimeAppPublic>::Public>::ID =>
				self.para_assignment.as_ref(),
			<<AuthorityDiscovery as BoundToRuntimeAppPublic>::Public>::ID =>
				self.authority_discovery.as_ref(),
			<<Beefy as BoundToRuntimeAppPublic>::Public>::ID => self.beefy.as_ref(),
			_ => &[],
		}
	}
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub grandpa: Grandpa,
		pub babe: Babe,
		pub para_validator: Initializer,
		pub para_assignment: ParaSessionInfo,
		pub authority_discovery: AuthorityDiscovery,
		pub beefy: Beefy,
	}
}

// remove this when removing `OldSessionKeys`
fn transform_session_keys(_val: AccountId, old: OldSessionKeys) -> SessionKeys {
	SessionKeys {
		grandpa: old.grandpa,
		babe: old.babe,
		para_validator: old.para_validator,
		para_assignment: old.para_assignment,
		authority_discovery: old.authority_discovery,
		beefy: old.beefy,
	}
}

/// Special `ValidatorIdOf` implementation that is just returning the input as result.
pub struct ValidatorIdOf;
impl sp_runtime::traits::Convert<AccountId, Option<AccountId>> for ValidatorIdOf {
	fn convert(a: AccountId) -> Option<AccountId> {
		Some(a)
	}
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type ValidatorIdOf = ValidatorIdOf;
	type ShouldEndSession = Babe;
	type NextSessionRotation = Babe;
	type SessionManager = pallet_session::historical::NoteHistoricalRoot<Self, ValidatorManager>;
	type SessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type WeightInfo = weights::pallet_session::WeightInfo<Runtime>;
}

pub struct FullIdentificationOf;
impl sp_runtime::traits::Convert<AccountId, Option<()>> for FullIdentificationOf {
	fn convert(_: AccountId) -> Option<()> {
		Some(Default::default())
	}
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = ();
	type FullIdentificationOf = FullIdentificationOf;
}

parameter_types! {
	pub const SessionsPerEra: SessionIndex = 6;
	pub const BondingDuration: sp_staking::EraIndex = 28;
}

parameter_types! {
	pub const ProposalBond: Permill = Permill::from_percent(5);
	pub const ProposalBondMinimum: Balance = 2000 * CENTS;
	pub const ProposalBondMaximum: Balance = 1 * GRAND;
	pub const SpendPeriod: BlockNumber = 6 * DAYS;
	pub const Burn: Permill = Permill::from_perthousand(2);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const PayoutSpendPeriod: BlockNumber = 30 * DAYS;
	// The asset's interior location for the paying account. This is the Treasury
	// pallet instance (which sits at index 18).
	pub TreasuryInteriorLocation: InteriorMultiLocation = PalletInstance(18).into();

	pub const TipCountdown: BlockNumber = 1 * DAYS;
	pub const TipFindersFee: Percent = Percent::from_percent(20);
	pub const TipReportDepositBase: Balance = 100 * CENTS;
	pub const DataDepositPerByte: Balance = 1 * CENTS;
	pub const MaxApprovals: u32 = 100;
	pub const MaxAuthorities: u32 = 100_000;
	pub const MaxKeys: u32 = 10_000;
	pub const MaxPeerInHeartbeats: u32 = 10_000;
	pub const MaxBalance: Balance = Balance::max_value();
}

impl pallet_treasury::Config for Runtime {
	type PalletId = TreasuryPalletId;
	type Currency = Balances;
	type ApproveOrigin = EitherOfDiverse<EnsureRoot<AccountId>, Treasurer>;
	type RejectOrigin = EitherOfDiverse<EnsureRoot<AccountId>, Treasurer>;
	type RuntimeEvent = RuntimeEvent;
	type OnSlash = Treasury;
	type ProposalBond = ProposalBond;
	type ProposalBondMinimum = ProposalBondMinimum;
	type ProposalBondMaximum = ProposalBondMaximum;
	type SpendPeriod = SpendPeriod;
	type Burn = Burn;
	type BurnDestination = Society;
	type MaxApprovals = MaxApprovals;
	type WeightInfo = weights::pallet_treasury::WeightInfo<Runtime>;
	type SpendFunds = Bounties;
	type SpendOrigin = TreasurySpender;
	type AssetKind = VersionedLocatableAsset;
	type Beneficiary = VersionedMultiLocation;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = PayOverXcm<
		TreasuryInteriorLocation,
		crate::xcm_config::XcmRouter,
		crate::XcmPallet,
		ConstU32<{ 6 * HOURS }>,
		Self::Beneficiary,
		Self::AssetKind,
		LocatableAssetConverter,
		VersionedMultiLocationConverter,
	>;
	type BalanceConverter = AssetRate;
	type PayoutPeriod = PayoutSpendPeriod;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = runtime_common::impls::benchmarks::TreasuryArguments;
}

parameter_types! {
	pub const BountyDepositBase: Balance = 100 * CENTS;
	pub const BountyDepositPayoutDelay: BlockNumber = 4 * DAYS;
	pub const BountyUpdatePeriod: BlockNumber = 90 * DAYS;
	pub const MaximumReasonLength: u32 = 16384;
	pub const CuratorDepositMultiplier: Permill = Permill::from_percent(50);
	pub const CuratorDepositMin: Balance = 10 * CENTS;
	pub const CuratorDepositMax: Balance = 500 * CENTS;
	pub const BountyValueMinimum: Balance = 200 * CENTS;
}

impl pallet_bounties::Config for Runtime {
	type BountyDepositBase = BountyDepositBase;
	type BountyDepositPayoutDelay = BountyDepositPayoutDelay;
	type BountyUpdatePeriod = BountyUpdatePeriod;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMin = CuratorDepositMin;
	type CuratorDepositMax = CuratorDepositMax;
	type BountyValueMinimum = BountyValueMinimum;
	type ChildBountyManager = ChildBounties;
	type DataDepositPerByte = DataDepositPerByte;
	type RuntimeEvent = RuntimeEvent;
	type MaximumReasonLength = MaximumReasonLength;
	type WeightInfo = weights::pallet_bounties::WeightInfo<Runtime>;
}

parameter_types! {
	pub const MaxActiveChildBountyCount: u32 = 100;
	pub const ChildBountyValueMinimum: Balance = BountyValueMinimum::get() / 10;
}

impl pallet_child_bounties::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxActiveChildBountyCount = MaxActiveChildBountyCount;
	type ChildBountyValueMinimum = ChildBountyValueMinimum;
	type WeightInfo = weights::pallet_child_bounties::WeightInfo<Runtime>;
}

impl pallet_offences::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
	type OnOffenceHandler = ();
}

impl pallet_authority_discovery::Config for Runtime {
	type MaxAuthorities = MaxAuthorities;
}

parameter_types! {
	pub const MaxSetIdSessionEntries: u32 = BondingDuration::get() * SessionsPerEra::get();
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
	type KeyOwnerProof = sp_session::MembershipProof;
	type EquivocationReportSystem =
		pallet_grandpa::EquivocationReportSystem<Self, Offences, Historical, ReportLongevity>;
}

/// Submits a transaction with the node's public and signature type. Adheres to the signed extension
/// format of the chain.
impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
		call: RuntimeCall,
		public: <Signature as Verify>::Signer,
		account: AccountId,
		nonce: <Runtime as frame_system::Config>::Nonce,
	) -> Option<(RuntimeCall, <UncheckedExtrinsic as ExtrinsicT>::SignaturePayload)> {
		use sp_runtime::traits::StaticLookup;
		// take the biggest period possible.
		let period =
			BlockHashCount::get().checked_next_power_of_two().map(|c| c / 2).unwrap_or(2) as u64;

		let current_block = System::block_number()
			.saturated_into::<u64>()
			// The `System::block_number` is initialized with `n+1`,
			// so the actual block number is `n`.
			.saturating_sub(1);
		let tip = 0;
		let extra: SignedExtra = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckMortality::<Runtime>::from(generic::Era::mortal(
				period,
				current_block,
			)),
			frame_system::CheckNonce::<Runtime>::from(nonce),
			frame_system::CheckWeight::<Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
		);
		let raw_payload = SignedPayload::new(call, extra)
			.map_err(|e| {
				log::warn!("Unable to create signed payload: {:?}", e);
			})
			.ok()?;
		let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
		let (call, extra, _) = raw_payload.deconstruct();
		let address = <Runtime as frame_system::Config>::Lookup::unlookup(account);
		Some((call, (address, signature, extra)))
	}
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type OverarchingCall = RuntimeCall;
}

parameter_types! {
	pub Prefix: &'static [u8] = b"Pay ROCs to the Rococo account:";
}

impl claims::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type VestingSchedule = Vesting;
	type Prefix = Prefix;
	type MoveClaimOrigin = EnsureRoot<AccountId>;
	type WeightInfo = weights::runtime_common_claims::WeightInfo<Runtime>;
}

parameter_types! {
	// Minimum 100 bytes/ROC deposited (1 CENT/byte)
	pub const BasicDeposit: Balance = 1000 * CENTS;       // 258 bytes on-chain
	pub const ByteDeposit: Balance = deposit(0, 1);
	pub const SubAccountDeposit: Balance = 200 * CENTS;   // 53 bytes on-chain
	pub const MaxSubAccounts: u32 = 100;
	pub const MaxAdditionalFields: u32 = 100;
	pub const MaxRegistrars: u32 = 20;
}

impl pallet_identity::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BasicDeposit = BasicDeposit;
	type ByteDeposit = ByteDeposit;
	type SubAccountDeposit = SubAccountDeposit;
	type MaxSubAccounts = MaxSubAccounts;
	type IdentityInformation = IdentityInfo<MaxAdditionalFields>;
	type MaxRegistrars = MaxRegistrars;
	type Slashed = Treasury;
	type ForceOrigin = EitherOf<EnsureRoot<Self::AccountId>, GeneralAdmin>;
	type RegistrarOrigin = EitherOf<EnsureRoot<Self::AccountId>, GeneralAdmin>;
	type WeightInfo = weights::pallet_identity::WeightInfo<Runtime>;
}

impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = weights::pallet_utility::WeightInfo<Runtime>;
}

parameter_types! {
	// One storage item; key size is 32; value is size 4+4+16+32 bytes = 56 bytes.
	pub const DepositBase: Balance = deposit(1, 88);
	// Additional storage item size of 32 bytes.
	pub const DepositFactor: Balance = deposit(0, 32);
	pub const MaxSignatories: u32 = 100;
}

impl pallet_multisig::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type DepositBase = DepositBase;
	type DepositFactor = DepositFactor;
	type MaxSignatories = MaxSignatories;
	type WeightInfo = weights::pallet_multisig::WeightInfo<Runtime>;
}

parameter_types! {
	pub const ConfigDepositBase: Balance = 500 * CENTS;
	pub const FriendDepositFactor: Balance = 50 * CENTS;
	pub const MaxFriends: u16 = 9;
	pub const RecoveryDeposit: Balance = 500 * CENTS;
}

impl pallet_recovery::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ConfigDepositBase = ConfigDepositBase;
	type FriendDepositFactor = FriendDepositFactor;
	type MaxFriends = MaxFriends;
	type RecoveryDeposit = RecoveryDeposit;
}

parameter_types! {
	pub const SocietyPalletId: PalletId = PalletId(*b"py/socie");
}

impl pallet_society::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
	type GraceStrikes = ConstU32<1>;
	type PeriodSpend = ConstU128<{ 50_000 * CENTS }>;
	type VotingPeriod = ConstU32<{ 5 * DAYS }>;
	type ClaimPeriod = ConstU32<{ 2 * DAYS }>;
	type MaxLockDuration = ConstU32<{ 36 * 30 * DAYS }>;
	type FounderSetOrigin = EnsureRoot<AccountId>;
	type ChallengePeriod = ConstU32<{ 7 * DAYS }>;
	type MaxPayouts = ConstU32<8>;
	type MaxBids = ConstU32<512>;
	type PalletId = SocietyPalletId;
	type WeightInfo = ();
}

parameter_types! {
	pub const MinVestedTransfer: Balance = 100 * CENTS;
	pub UnvestedFundsAllowedWithdrawReasons: WithdrawReasons =
		WithdrawReasons::except(WithdrawReasons::TRANSFER | WithdrawReasons::RESERVE);
}

impl pallet_vesting::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BlockNumberToBalance = ConvertInto;
	type MinVestedTransfer = MinVestedTransfer;
	type WeightInfo = weights::pallet_vesting::WeightInfo<Runtime>;
	type UnvestedFundsAllowedWithdrawReasons = UnvestedFundsAllowedWithdrawReasons;
	type BlockNumberProvider = System;
	const MAX_VESTING_SCHEDULES: u32 = 28;
}

parameter_types! {
	// One storage item; key size 32, value size 8; .
	pub const ProxyDepositBase: Balance = deposit(1, 8);
	// Additional storage item size of 33 bytes.
	pub const ProxyDepositFactor: Balance = deposit(0, 33);
	pub const MaxProxies: u16 = 32;
	pub const AnnouncementDepositBase: Balance = deposit(1, 8);
	pub const AnnouncementDepositFactor: Balance = deposit(0, 66);
	pub const MaxPending: u16 = 32;
}

/// The type used to represent the kinds of proxying allowed.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum ProxyType {
	Any,
	NonTransfer,
	Governance,
	IdentityJudgement,
	CancelProxy,
	Auction,
	Society,
	OnDemandOrdering,
}
impl Default for ProxyType {
	fn default() -> Self {
		Self::Any
	}
}
impl InstanceFilter<RuntimeCall> for ProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			ProxyType::Any => true,
			ProxyType::NonTransfer => matches!(
				c,
				RuntimeCall::System(..) |
				RuntimeCall::Babe(..) |
				RuntimeCall::Timestamp(..) |
				RuntimeCall::Indices(pallet_indices::Call::claim {..}) |
				RuntimeCall::Indices(pallet_indices::Call::free {..}) |
				RuntimeCall::Indices(pallet_indices::Call::freeze {..}) |
				// Specifically omitting Indices `transfer`, `force_transfer`
				// Specifically omitting the entire Balances pallet
				RuntimeCall::Session(..) |
				RuntimeCall::Grandpa(..) |
				RuntimeCall::Treasury(..) |
				RuntimeCall::Bounties(..) |
				RuntimeCall::ChildBounties(..) |
				RuntimeCall::ConvictionVoting(..) |
				RuntimeCall::Referenda(..) |
				RuntimeCall::FellowshipCollective(..) |
				RuntimeCall::FellowshipReferenda(..) |
				RuntimeCall::Whitelist(..) |
				RuntimeCall::Claims(..) |
				RuntimeCall::Utility(..) |
				RuntimeCall::Identity(..) |
				RuntimeCall::Society(..) |
				RuntimeCall::Recovery(pallet_recovery::Call::as_recovered {..}) |
				RuntimeCall::Recovery(pallet_recovery::Call::vouch_recovery {..}) |
				RuntimeCall::Recovery(pallet_recovery::Call::claim_recovery {..}) |
				RuntimeCall::Recovery(pallet_recovery::Call::close_recovery {..}) |
				RuntimeCall::Recovery(pallet_recovery::Call::remove_recovery {..}) |
				RuntimeCall::Recovery(pallet_recovery::Call::cancel_recovered {..}) |
				// Specifically omitting Recovery `create_recovery`, `initiate_recovery`
				RuntimeCall::Vesting(pallet_vesting::Call::vest {..}) |
				RuntimeCall::Vesting(pallet_vesting::Call::vest_other {..}) |
				// Specifically omitting Vesting `vested_transfer`, and `force_vested_transfer`
				RuntimeCall::Scheduler(..) |
				RuntimeCall::Proxy(..) |
				RuntimeCall::Multisig(..) |
				RuntimeCall::Nis(..) |
				RuntimeCall::Registrar(paras_registrar::Call::register {..}) |
				RuntimeCall::Registrar(paras_registrar::Call::deregister {..}) |
				// Specifically omitting Registrar `swap`
				RuntimeCall::Registrar(paras_registrar::Call::reserve {..}) |
				RuntimeCall::Crowdloan(..) |
				RuntimeCall::Slots(..) |
				RuntimeCall::Auctions(..) // Specifically omitting the entire XCM Pallet
			),
			ProxyType::Governance => matches!(
				c,
				RuntimeCall::Bounties(..) |
					RuntimeCall::Utility(..) |
					RuntimeCall::ChildBounties(..) |
					// OpenGov calls
					RuntimeCall::ConvictionVoting(..) |
					RuntimeCall::Referenda(..) |
					RuntimeCall::FellowshipCollective(..) |
					RuntimeCall::FellowshipReferenda(..) |
					RuntimeCall::Whitelist(..)
			),
			ProxyType::IdentityJudgement => matches!(
				c,
				RuntimeCall::Identity(pallet_identity::Call::provide_judgement { .. }) |
					RuntimeCall::Utility(..)
			),
			ProxyType::CancelProxy => {
				matches!(c, RuntimeCall::Proxy(pallet_proxy::Call::reject_announcement { .. }))
			},
			ProxyType::Auction => matches!(
				c,
				RuntimeCall::Auctions { .. } |
					RuntimeCall::Crowdloan { .. } |
					RuntimeCall::Registrar { .. } |
					RuntimeCall::Multisig(..) |
					RuntimeCall::Slots { .. }
			),
			ProxyType::Society => matches!(c, RuntimeCall::Society(..)),
			ProxyType::OnDemandOrdering => matches!(c, RuntimeCall::OnDemandAssignmentProvider(..)),
		}
	}
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(ProxyType::Any, _) => true,
			(_, ProxyType::Any) => false,
			(ProxyType::NonTransfer, _) => true,
			_ => false,
		}
	}
}

impl pallet_proxy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ProxyDepositBase;
	type ProxyDepositFactor = ProxyDepositFactor;
	type MaxProxies = MaxProxies;
	type WeightInfo = weights::pallet_proxy::WeightInfo<Runtime>;
	type MaxPending = MaxPending;
	type CallHasher = BlakeTwo256;
	type AnnouncementDepositBase = AnnouncementDepositBase;
	type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

impl parachains_origin::Config for Runtime {}

impl parachains_configuration::Config for Runtime {
	type WeightInfo = weights::runtime_parachains_configuration::WeightInfo<Runtime>;
}

impl parachains_shared::Config for Runtime {}

impl parachains_session_info::Config for Runtime {
	type ValidatorSet = Historical;
}

/// Special `RewardValidators` that does nothing ;)
pub struct RewardValidators;
impl runtime_parachains::inclusion::RewardValidators for RewardValidators {
	fn reward_backing(_: impl IntoIterator<Item = ValidatorIndex>) {}
	fn reward_bitfields(_: impl IntoIterator<Item = ValidatorIndex>) {}
}

impl parachains_inclusion::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type DisputesHandler = ParasDisputes;
	type RewardValidators = RewardValidators;
	type MessageQueue = MessageQueue;
	type WeightInfo = weights::runtime_parachains_inclusion::WeightInfo<Runtime>;
}

parameter_types! {
	pub const ParasUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
}

impl parachains_paras::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::runtime_parachains_paras::WeightInfo<Runtime>;
	type UnsignedPriority = ParasUnsignedPriority;
	type QueueFootprinter = ParaInclusion;
	type NextSessionRotation = Babe;
	type OnNewHead = Registrar;
}

parameter_types! {
	/// Amount of weight that can be spent per block to service messages.
	///
	/// # WARNING
	///
	/// This is not a good value for para-chains since the `Scheduler` already uses up to 80% block weight.
	pub MessageQueueServiceWeight: Weight = Perbill::from_percent(20) * BlockWeights::get().max_block;
	pub const MessageQueueHeapSize: u32 = 32 * 1024;
	pub const MessageQueueMaxStale: u32 = 96;
}

/// Message processor to handle any messages that were enqueued into the `MessageQueue` pallet.
pub struct MessageProcessor;
impl ProcessMessage for MessageProcessor {
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let para = match origin {
			AggregateMessageOrigin::Ump(UmpQueueId::Para(para)) => para,
		};
		xcm_builder::ProcessXcmMessage::<
			Junction,
			xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
			RuntimeCall,
		>::process_message(message, Junction::Parachain(para.into()), meter, id)
	}
}

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Size = u32;
	type HeapSize = MessageQueueHeapSize;
	type MaxStale = MessageQueueMaxStale;
	type ServiceWeight = MessageQueueServiceWeight;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MessageProcessor = MessageProcessor;
	#[cfg(feature = "runtime-benchmarks")]
	type MessageProcessor =
		pallet_message_queue::mock_helpers::NoopMessageProcessor<AggregateMessageOrigin>;
	type QueueChangeHandler = ParaInclusion;
	type QueuePausedQuery = ();
	type WeightInfo = weights::pallet_message_queue::WeightInfo<Runtime>;
}

impl parachains_dmp::Config for Runtime {}

impl parachains_hrmp::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type ChannelManager = EnsureRoot<AccountId>;
	type Currency = Balances;
	type WeightInfo = weights::runtime_parachains_hrmp::WeightInfo<Runtime>;
}

impl parachains_paras_inherent::Config for Runtime {
	type WeightInfo = weights::runtime_parachains_paras_inherent::WeightInfo<Runtime>;
}

impl parachains_scheduler::Config for Runtime {
	type AssignmentProvider = ParaAssignmentProvider;
}

parameter_types! {
	pub const OnDemandTrafficDefaultValue: FixedU128 = FixedU128::from_u32(1);
}

impl parachains_assigner_on_demand::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type TrafficDefaultValue = OnDemandTrafficDefaultValue;
	type WeightInfo = weights::runtime_parachains_assigner_on_demand::WeightInfo<Runtime>;
}

impl parachains_assigner_parachains::Config for Runtime {}

impl parachains_assigner::Config for Runtime {
	type OnDemandAssignmentProvider = OnDemandAssignmentProvider;
	type ParachainsAssignmentProvider = ParachainsAssignmentProvider;
}

impl parachains_initializer::Config for Runtime {
	type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type WeightInfo = weights::runtime_parachains_initializer::WeightInfo<Runtime>;
}

impl parachains_disputes::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RewardValidators = ();
	type SlashingHandler = parachains_slashing::SlashValidatorsForDisputes<ParasSlashing>;
	type WeightInfo = weights::runtime_parachains_disputes::WeightInfo<Runtime>;
}

impl parachains_slashing::Config for Runtime {
	type KeyOwnerProofSystem = Historical;
	type KeyOwnerProof =
		<Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, ValidatorId)>>::Proof;
	type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		ValidatorId,
	)>>::IdentificationTuple;
	type HandleReports = parachains_slashing::SlashingReportHandler<
		Self::KeyOwnerIdentification,
		Offences,
		ReportLongevity,
	>;
	type WeightInfo = parachains_slashing::TestWeightInfo;
	type BenchmarkingConfig = parachains_slashing::BenchConfig<200>;
}

parameter_types! {
	pub const ParaDeposit: Balance = 40 * UNITS;
}

impl paras_registrar::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type OnSwap = (Crowdloan, Slots);
	type ParaDeposit = ParaDeposit;
	type DataDepositPerByte = DataDepositPerByte;
	type WeightInfo = weights::runtime_common_paras_registrar::WeightInfo<Runtime>;
}

parameter_types! {
	pub LeasePeriod: BlockNumber = prod_or_fast!(1 * DAYS, 1 * DAYS, "ROC_LEASE_PERIOD");
}

impl slots::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type Registrar = Registrar;
	type LeasePeriod = LeasePeriod;
	type LeaseOffset = ();
	type ForceOrigin = EitherOf<EnsureRoot<Self::AccountId>, LeaseAdmin>;
	type WeightInfo = weights::runtime_common_slots::WeightInfo<Runtime>;
}

parameter_types! {
	pub const CrowdloanId: PalletId = PalletId(*b"py/cfund");
	pub const SubmissionDeposit: Balance = 3 * GRAND;
	pub const MinContribution: Balance = 3_000 * CENTS;
	pub const RemoveKeysLimit: u32 = 1000;
	// Allow 32 bytes for an additional memo to a crowdloan.
	pub const MaxMemoLength: u8 = 32;
}

impl crowdloan::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type PalletId = CrowdloanId;
	type SubmissionDeposit = SubmissionDeposit;
	type MinContribution = MinContribution;
	type RemoveKeysLimit = RemoveKeysLimit;
	type Registrar = Registrar;
	type Auctioneer = Auctions;
	type MaxMemoLength = MaxMemoLength;
	type WeightInfo = weights::runtime_common_crowdloan::WeightInfo<Runtime>;
}

parameter_types! {
	// The average auction is 7 days long, so this will be 70% for ending period.
	// 5 Days = 72000 Blocks @ 6 sec per block
	pub const EndingPeriod: BlockNumber = 5 * DAYS;
	// ~ 1000 samples per day -> ~ 20 blocks per sample -> 2 minute samples
	pub const SampleLength: BlockNumber = 2 * MINUTES;
}

impl auctions::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Leaser = Slots;
	type Registrar = Registrar;
	type EndingPeriod = EndingPeriod;
	type SampleLength = SampleLength;
	type Randomness = pallet_babe::RandomnessFromOneEpochAgo<Runtime>;
	type InitiateOrigin = EitherOf<EnsureRoot<Self::AccountId>, AuctionAdmin>;
	type WeightInfo = weights::runtime_common_auctions::WeightInfo<Runtime>;
}

impl identity_migrator::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// To be changed to `EnsureSigned` once there is a People Chain to migrate to.
	type Reaper = EnsureRoot<AccountId>;
	type ReapIdentityHandler = ToParachainIdentityReaper<Runtime, Self::AccountId>;
	type WeightInfo = weights::runtime_common_identity_migrator::WeightInfo<Runtime>;
}

type NisCounterpartInstance = pallet_balances::Instance2;
impl pallet_balances::Config<NisCounterpartInstance> for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<10_000_000_000>; // One RTC cent
	type AccountStore = StorageMapShim<
		pallet_balances::Account<Runtime, NisCounterpartInstance>,
		AccountId,
		pallet_balances::AccountData<u128>,
	>;
	type MaxLocks = ConstU32<4>;
	type MaxReserves = ConstU32<4>;
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = weights::pallet_balances_nis_counterpart_balances::WeightInfo<Runtime>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = ();
	type MaxHolds = ConstU32<2>;
	type MaxFreezes = ConstU32<1>;
}

parameter_types! {
	pub const NisBasePeriod: BlockNumber = 30 * DAYS;
	pub const MinBid: Balance = 100 * UNITS;
	pub MinReceipt: Perquintill = Perquintill::from_rational(1u64, 10_000_000u64);
	pub const IntakePeriod: BlockNumber = 5 * MINUTES;
	pub MaxIntakeWeight: Weight = MAXIMUM_BLOCK_WEIGHT / 10;
	pub const ThawThrottle: (Perquintill, BlockNumber) = (Perquintill::from_percent(25), 5);
	pub storage NisTarget: Perquintill = Perquintill::zero();
	pub const NisPalletId: PalletId = PalletId(*b"py/nis  ");
}

impl pallet_nis::Config for Runtime {
	type WeightInfo = weights::pallet_nis::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type CurrencyBalance = Balance;
	type FundOrigin = frame_system::EnsureSigned<AccountId>;
	type Counterpart = NisCounterpartBalances;
	type CounterpartAmount = WithMaximumOf<ConstU128<21_000_000_000_000_000_000u128>>;
	type Deficit = (); // Mint
	type IgnoredIssuance = ();
	type Target = NisTarget;
	type PalletId = NisPalletId;
	type QueueCount = ConstU32<300>;
	type MaxQueueLen = ConstU32<1000>;
	type FifoQueueLen = ConstU32<250>;
	type BasePeriod = NisBasePeriod;
	type MinBid = MinBid;
	type MinReceipt = MinReceipt;
	type IntakePeriod = IntakePeriod;
	type MaxIntakeWeight = MaxIntakeWeight;
	type ThawThrottle = ThawThrottle;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	pub const BeefySetIdSessionEntries: u32 = BondingDuration::get() * SessionsPerEra::get();
}

impl pallet_beefy::Config for Runtime {
	type BeefyId = BeefyId;
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = BeefySetIdSessionEntries;
	type OnNewValidatorSet = MmrLeaf;
	type WeightInfo = ();
	type KeyOwnerProof = <Historical as KeyOwnerProofSystem<(KeyTypeId, BeefyId)>>::Proof;
	type EquivocationReportSystem =
		pallet_beefy::EquivocationReportSystem<Self, Offences, Historical, ReportLongevity>;
}

/// MMR helper types.
mod mmr {
	use super::Runtime;
	pub use pallet_mmr::primitives::*;

	pub type Leaf = <<Runtime as pallet_mmr::Config>::LeafData as LeafDataProvider>::LeafData;
	pub type Hashing = <Runtime as pallet_mmr::Config>::Hashing;
	pub type Hash = <Hashing as sp_runtime::traits::Hash>::Output;
}

impl pallet_mmr::Config for Runtime {
	const INDEXING_PREFIX: &'static [u8] = mmr::INDEXING_PREFIX;
	type Hashing = Keccak256;
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

pub struct ParaHeadsRootProvider;
impl BeefyDataProvider<H256> for ParaHeadsRootProvider {
	fn extra_data() -> H256 {
		let mut para_heads: Vec<(u32, Vec<u8>)> = Paras::parachains()
			.into_iter()
			.filter_map(|id| Paras::para_head(&id).map(|head| (id.into(), head.0)))
			.collect();
		para_heads.sort();
		binary_merkle_tree::merkle_root::<mmr::Hashing, _>(
			para_heads.into_iter().map(|pair| pair.encode()),
		)
		.into()
	}
}

impl pallet_beefy_mmr::Config for Runtime {
	type LeafVersion = LeafVersion;
	type BeefyAuthorityToMerkleLeaf = pallet_beefy_mmr::BeefyEcdsaToEthereum;
	type LeafExtra = H256;
	type BeefyDataProvider = ParaHeadsRootProvider;
}

impl paras_sudo_wrapper::Config for Runtime {}

parameter_types! {
	pub const PermanentSlotLeasePeriodLength: u32 = 365;
	pub const TemporarySlotLeasePeriodLength: u32 = 5;
	pub const MaxTemporarySlotPerLeasePeriod: u32 = 5;
}

impl assigned_slots::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AssignSlotOrigin = EnsureRoot<AccountId>;
	type Leaser = Slots;
	type PermanentSlotLeasePeriodLength = PermanentSlotLeasePeriodLength;
	type TemporarySlotLeasePeriodLength = TemporarySlotLeasePeriodLength;
	type MaxTemporarySlotPerLeasePeriod = MaxTemporarySlotPerLeasePeriod;
	type WeightInfo = weights::runtime_common_assigned_slots::WeightInfo<Runtime>;
}

impl validator_manager::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type PrivilegedOrigin = EnsureRoot<AccountId>;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = weights::pallet_sudo::WeightInfo<Runtime>;
}

impl pallet_root_testing::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_asset_rate::Config for Runtime {
	type WeightInfo = weights::pallet_asset_rate::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type CreateOrigin = EnsureRoot<AccountId>;
	type RemoveOrigin = EnsureRoot<AccountId>;
	type UpdateOrigin = EnsureRoot<AccountId>;
	type Currency = Balances;
	type AssetKind = <Runtime as pallet_treasury::Config>::AssetKind;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = runtime_common::impls::benchmarks::AssetRateArguments;
}

construct_runtime! {
	pub enum Runtime
	{
		// Basic stuff; balances is uncallable initially.
		System: frame_system::{Pallet, Call, Storage, Config<T>, Event<T>} = 0,

		// Babe must be before session.
		Babe: pallet_babe::{Pallet, Call, Storage, Config<T>, ValidateUnsigned} = 1,

		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 2,
		Indices: pallet_indices::{Pallet, Call, Storage, Config<T>, Event<T>} = 3,
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 4,
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 33,

		// Consensus support.
		// Authorship must be before session in order to note author in the correct session and era.
		Authorship: pallet_authorship::{Pallet, Storage} = 5,
		Offences: pallet_offences::{Pallet, Storage, Event} = 7,
		Historical: session_historical::{Pallet} = 34,

		// BEEFY Bridges support.
		Beefy: pallet_beefy::{Pallet, Call, Storage, Config<T>, ValidateUnsigned} = 240,
		// MMR leaf construction must be before session in order to have leaf contents
		// refer to block<N-1> consistently. see substrate issue #11797 for details.
		Mmr: pallet_mmr::{Pallet, Storage} = 241,
		MmrLeaf: pallet_beefy_mmr::{Pallet, Storage} = 242,

		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>} = 8,
		Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config<T>, Event, ValidateUnsigned} = 10,
		AuthorityDiscovery: pallet_authority_discovery::{Pallet, Config<T>} = 12,

		// Governance stuff; uncallable initially.
		Treasury: pallet_treasury::{Pallet, Call, Storage, Config<T>, Event<T>} = 18,
		ConvictionVoting: pallet_conviction_voting::{Pallet, Call, Storage, Event<T>} = 20,
		Referenda: pallet_referenda::{Pallet, Call, Storage, Event<T>} = 21,
		//	pub type FellowshipCollectiveInstance = pallet_ranked_collective::Instance1;
		FellowshipCollective: pallet_ranked_collective::<Instance1>::{
			Pallet, Call, Storage, Event<T>
		} = 22,
		// pub type FellowshipReferendaInstance = pallet_referenda::Instance2;
		FellowshipReferenda: pallet_referenda::<Instance2>::{
			Pallet, Call, Storage, Event<T>
		} = 23,
		Origins: pallet_custom_origins::{Origin} = 43,
		Whitelist: pallet_whitelist::{Pallet, Call, Storage, Event<T>} = 44,
		// Claims. Usable initially.
		Claims: claims::{Pallet, Call, Storage, Event<T>, Config<T>, ValidateUnsigned} = 19,

		// Utility module.
		Utility: pallet_utility::{Pallet, Call, Event} = 24,

		// Less simple identity module.
		Identity: pallet_identity::{Pallet, Call, Storage, Event<T>} = 25,

		// Society module.
		Society: pallet_society::{Pallet, Call, Storage, Event<T>} = 26,

		// Social recovery module.
		Recovery: pallet_recovery::{Pallet, Call, Storage, Event<T>} = 27,

		// Vesting. Usable initially, but removed once all vesting is finished.
		Vesting: pallet_vesting::{Pallet, Call, Storage, Event<T>, Config<T>} = 28,

		// System scheduler.
		Scheduler: pallet_scheduler::{Pallet, Call, Storage, Event<T>} = 29,

		// Proxy module. Late addition.
		Proxy: pallet_proxy::{Pallet, Call, Storage, Event<T>} = 30,

		// Multisig module. Late addition.
		Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 31,

		// Preimage registrar.
		Preimage: pallet_preimage::{Pallet, Call, Storage, Event<T>, HoldReason} = 32,

		// Asset rate.
		AssetRate: pallet_asset_rate::{Pallet, Call, Storage, Event<T>} = 39,

		// Bounties modules.
		Bounties: pallet_bounties::{Pallet, Call, Storage, Event<T>} = 35,
		ChildBounties: pallet_child_bounties = 40,

		// NIS pallet.
		Nis: pallet_nis::{Pallet, Call, Storage, Event<T>, HoldReason} = 38,
		// pub type NisCounterpartInstance = pallet_balances::Instance2;
		NisCounterpartBalances: pallet_balances::<Instance2> = 45,

		// Parachains pallets. Start indices at 50 to leave room.
		ParachainsOrigin: parachains_origin::{Pallet, Origin} = 50,
		Configuration: parachains_configuration::{Pallet, Call, Storage, Config<T>} = 51,
		ParasShared: parachains_shared::{Pallet, Call, Storage} = 52,
		ParaInclusion: parachains_inclusion::{Pallet, Call, Storage, Event<T>} = 53,
		ParaInherent: parachains_paras_inherent::{Pallet, Call, Storage, Inherent} = 54,
		ParaScheduler: parachains_scheduler::{Pallet, Storage} = 55,
		Paras: parachains_paras::{Pallet, Call, Storage, Event, Config<T>, ValidateUnsigned} = 56,
		Initializer: parachains_initializer::{Pallet, Call, Storage} = 57,
		Dmp: parachains_dmp::{Pallet, Storage} = 58,
		Hrmp: parachains_hrmp::{Pallet, Call, Storage, Event<T>, Config<T>} = 60,
		ParaSessionInfo: parachains_session_info::{Pallet, Storage} = 61,
		ParasDisputes: parachains_disputes::{Pallet, Call, Storage, Event<T>} = 62,
		ParasSlashing: parachains_slashing::{Pallet, Call, Storage, ValidateUnsigned} = 63,
		MessageQueue: pallet_message_queue::{Pallet, Call, Storage, Event<T>} = 64,
		ParaAssignmentProvider: parachains_assigner::{Pallet, Storage} = 65,
		OnDemandAssignmentProvider: parachains_assigner_on_demand::{Pallet, Call, Storage, Event<T>} = 66,
		ParachainsAssignmentProvider: parachains_assigner_parachains::{Pallet} = 67,

		// Parachain Onboarding Pallets. Start indices at 70 to leave room.
		Registrar: paras_registrar::{Pallet, Call, Storage, Event<T>, Config<T>} = 70,
		Slots: slots::{Pallet, Call, Storage, Event<T>} = 71,
		Auctions: auctions::{Pallet, Call, Storage, Event<T>} = 72,
		Crowdloan: crowdloan::{Pallet, Call, Storage, Event<T>} = 73,

		// Pallet for sending XCM.
		XcmPallet: pallet_xcm::{Pallet, Call, Storage, Event<T>, Origin, Config<T>} = 99,

		// Pallet for migrating Identity to a parachain. To be removed post-migration.
		IdentityMigrator: identity_migrator::{Pallet, Call, Event<T>} = 248,

		ParasSudoWrapper: paras_sudo_wrapper::{Pallet, Call} = 250,
		AssignedSlots: assigned_slots::{Pallet, Call, Storage, Event<T>, Config<T>} = 251,

		// Validator Manager pallet.
		ValidatorManager: validator_manager::{Pallet, Call, Storage, Event<T>} = 252,

		// State trie migration pallet, only temporary.
		StateTrieMigration: pallet_state_trie_migration = 254,

		// Root testing pallet.
		RootTesting: pallet_root_testing::{Pallet, Call, Storage, Event<T>} = 249,

		// Sudo.
		Sudo: pallet_sudo::{Pallet, Call, Storage, Event<T>, Config<T>} = 255,
	}
}

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// `BlockId` type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The `SignedExtension` to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckMortality<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// All migrations that will run on the next runtime upgrade.
///
/// This contains the combined migrations of the last 10 releases. It allows to skip runtime
/// upgrades in case governance decides to do so. THE ORDER IS IMPORTANT.
pub type Migrations = migrations::Unreleased;

/// The runtime migrations per release.
#[allow(deprecated, missing_docs)]
pub mod migrations {
	use super::*;

	use frame_support::traits::LockIdentifier;
	use frame_system::pallet_prelude::BlockNumberFor;
	#[cfg(feature = "try-runtime")]
	use sp_core::crypto::ByteArray;

	parameter_types! {
		pub const DemocracyPalletName: &'static str = "Democracy";
		pub const CouncilPalletName: &'static str = "Council";
		pub const TechnicalCommitteePalletName: &'static str = "TechnicalCommittee";
		pub const PhragmenElectionPalletName: &'static str = "PhragmenElection";
		pub const TechnicalMembershipPalletName: &'static str = "TechnicalMembership";
		pub const TipsPalletName: &'static str = "Tips";
		pub const ImOnlinePalletName: &'static str = "ImOnline";
		pub const PhragmenElectionPalletId: LockIdentifier = *b"phrelect";
	}

	// Special Config for Gov V1 pallets, allowing us to run migrations for them without
	// implementing their configs on [`Runtime`].
	pub struct UnlockConfig;
	impl pallet_democracy::migrations::unlock_and_unreserve_all_funds::UnlockConfig for UnlockConfig {
		type Currency = Balances;
		type MaxVotes = ConstU32<100>;
		type MaxDeposits = ConstU32<100>;
		type AccountId = AccountId;
		type BlockNumber = BlockNumberFor<Runtime>;
		type DbWeight = <Runtime as frame_system::Config>::DbWeight;
		type PalletName = DemocracyPalletName;
	}
	impl pallet_elections_phragmen::migrations::unlock_and_unreserve_all_funds::UnlockConfig
		for UnlockConfig
	{
		type Currency = Balances;
		type MaxVotesPerVoter = ConstU32<16>;
		type PalletId = PhragmenElectionPalletId;
		type AccountId = AccountId;
		type DbWeight = <Runtime as frame_system::Config>::DbWeight;
		type PalletName = PhragmenElectionPalletName;
	}
	impl pallet_tips::migrations::unreserve_deposits::UnlockConfig<()> for UnlockConfig {
		type Currency = Balances;
		type Hash = Hash;
		type DataDepositPerByte = DataDepositPerByte;
		type TipReportDepositBase = TipReportDepositBase;
		type AccountId = AccountId;
		type BlockNumber = BlockNumberFor<Runtime>;
		type DbWeight = <Runtime as frame_system::Config>::DbWeight;
		type PalletName = TipsPalletName;
	}

	/// Upgrade Session keys to exclude `ImOnline` key.
	/// When this is removed, should also remove `OldSessionKeys`.
	pub struct UpgradeSessionKeys;
	const UPGRADE_SESSION_KEYS_FROM_SPEC: u32 = 104000;

	impl frame_support::traits::OnRuntimeUpgrade for UpgradeSessionKeys {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
			if System::last_runtime_upgrade_spec_version() > UPGRADE_SESSION_KEYS_FROM_SPEC {
				log::warn!(target: "runtime::session_keys", "Skipping session keys migration pre-upgrade check due to spec version (already applied?)");
				return Ok(Vec::new())
			}

			log::info!(target: "runtime::session_keys", "Collecting pre-upgrade session keys state");
			let key_ids = SessionKeys::key_ids();
			frame_support::ensure!(
				key_ids.into_iter().find(|&k| *k == sp_core::crypto::key_types::IM_ONLINE) == None,
				"New session keys contain the ImOnline key that should have been removed",
			);
			let storage_key = pallet_session::QueuedKeys::<Runtime>::hashed_key();
			let mut state: Vec<u8> = Vec::new();
			frame_support::storage::unhashed::get::<Vec<(ValidatorId, OldSessionKeys)>>(
				&storage_key,
			)
			.ok_or::<sp_runtime::TryRuntimeError>("Queued keys are not available".into())?
			.into_iter()
			.for_each(|(id, keys)| {
				state.extend_from_slice(id.as_slice());
				for key_id in key_ids {
					state.extend_from_slice(keys.get_raw(*key_id));
				}
			});
			frame_support::ensure!(state.len() > 0, "Queued keys are not empty before upgrade");
			Ok(state)
		}

		fn on_runtime_upgrade() -> Weight {
			if System::last_runtime_upgrade_spec_version() > UPGRADE_SESSION_KEYS_FROM_SPEC {
				log::info!("Skipping session keys upgrade: already applied");
				return <Runtime as frame_system::Config>::DbWeight::get().reads(1)
			}
			log::trace!("Upgrading session keys");
			Session::upgrade_keys::<OldSessionKeys, _>(transform_session_keys);
			Perbill::from_percent(50) * BlockWeights::get().max_block
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(
			old_state: sp_std::vec::Vec<u8>,
		) -> Result<(), sp_runtime::TryRuntimeError> {
			if System::last_runtime_upgrade_spec_version() > UPGRADE_SESSION_KEYS_FROM_SPEC {
				log::warn!(target: "runtime::session_keys", "Skipping session keys migration post-upgrade check due to spec version (already applied?)");
				return Ok(())
			}

			let key_ids = SessionKeys::key_ids();
			let mut new_state: Vec<u8> = Vec::new();
			pallet_session::QueuedKeys::<Runtime>::get().into_iter().for_each(|(id, keys)| {
				new_state.extend_from_slice(id.as_slice());
				for key_id in key_ids {
					new_state.extend_from_slice(keys.get_raw(*key_id));
				}
			});
			frame_support::ensure!(new_state.len() > 0, "Queued keys are not empty after upgrade");
			frame_support::ensure!(
				old_state == new_state,
				"Pre-upgrade and post-upgrade keys do not match!"
			);
			log::info!(target: "runtime::session_keys", "Session keys migrated successfully");
			Ok(())
		}
	}

	/// Unreleased migrations. Add new ones here:
	pub type Unreleased = (
		pallet_society::migrations::MigrateToV2<Runtime, (), ()>,
		parachains_configuration::migration::v7::MigrateToV7<Runtime>,
		assigned_slots::migration::v1::MigrateToV1<Runtime>,
		parachains_scheduler::migration::v1::MigrateToV1<Runtime>,
		parachains_configuration::migration::v8::MigrateToV8<Runtime>,
		parachains_configuration::migration::v9::MigrateToV9<Runtime>,
		paras_registrar::migration::MigrateToV1<Runtime, ()>,
		pallet_referenda::migration::v1::MigrateV0ToV1<Runtime, ()>,
		pallet_referenda::migration::v1::MigrateV0ToV1<Runtime, pallet_referenda::Instance2>,

		// Unlock & unreserve Gov1 funds

		pallet_elections_phragmen::migrations::unlock_and_unreserve_all_funds::UnlockAndUnreserveAllFunds<UnlockConfig>,
		pallet_democracy::migrations::unlock_and_unreserve_all_funds::UnlockAndUnreserveAllFunds<UnlockConfig>,
		pallet_tips::migrations::unreserve_deposits::UnreserveDeposits<UnlockConfig, ()>,

		// Delete all Gov v1 pallet storage key/values.

		frame_support::migrations::RemovePallet<DemocracyPalletName, <Runtime as frame_system::Config>::DbWeight>,
		frame_support::migrations::RemovePallet<CouncilPalletName, <Runtime as frame_system::Config>::DbWeight>,
		frame_support::migrations::RemovePallet<TechnicalCommitteePalletName, <Runtime as frame_system::Config>::DbWeight>,
		frame_support::migrations::RemovePallet<PhragmenElectionPalletName, <Runtime as frame_system::Config>::DbWeight>,
		frame_support::migrations::RemovePallet<TechnicalMembershipPalletName, <Runtime as frame_system::Config>::DbWeight>,
		frame_support::migrations::RemovePallet<TipsPalletName, <Runtime as frame_system::Config>::DbWeight>,

		pallet_grandpa::migrations::MigrateV4ToV5<Runtime>,
		parachains_configuration::migration::v10::MigrateToV10<Runtime>,

		// Upgrade `SessionKeys` to exclude `ImOnline`
		UpgradeSessionKeys,

		// Remove `im-online` pallet on-chain storage
		frame_support::migrations::RemovePallet<ImOnlinePalletName, <Runtime as frame_system::Config>::DbWeight>,
		parachains_configuration::migration::v11::MigrateToV11<Runtime>,
	);
}

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	Migrations,
>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;

parameter_types! {
	// The deposit configuration for the singed migration. Specially if you want to allow any signed account to do the migration (see `SignedFilter`, these deposits should be high)
	pub const MigrationSignedDepositPerItem: Balance = 1 * CENTS;
	pub const MigrationSignedDepositBase: Balance = 20 * CENTS * 100;
	pub const MigrationMaxKeyLen: u32 = 512;
}

impl pallet_state_trie_migration::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type SignedDepositPerItem = MigrationSignedDepositPerItem;
	type SignedDepositBase = MigrationSignedDepositBase;
	type ControlOrigin = EnsureRoot<AccountId>;
	// specific account for the migration, can trigger the signed migrations.
	type SignedFilter = frame_system::EnsureSignedBy<MigController, AccountId>;

	// Use same weights as substrate ones.
	type WeightInfo = pallet_state_trie_migration::weights::SubstrateWeight<Runtime>;
	type MaxKeyLen = MigrationMaxKeyLen;
}

frame_support::ord_parameter_types! {
	pub const MigController: AccountId = AccountId::from(hex_literal::hex!("52bc71c1eca5353749542dfdf0af97bf764f9c2f44e860cd485f1cd86400f649"));
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame_benchmarking::define_benchmarks!(
		// Polkadot
		// NOTE: Make sure to prefix these with `runtime_common::` so
		// the that path resolves correctly in the generated file.
		[runtime_common::assigned_slots, AssignedSlots]
		[runtime_common::auctions, Auctions]
		[runtime_common::crowdloan, Crowdloan]
		[runtime_common::claims, Claims]
		[runtime_common::identity_migrator, IdentityMigrator]
		[runtime_common::slots, Slots]
		[runtime_common::paras_registrar, Registrar]
		[runtime_parachains::configuration, Configuration]
		[runtime_parachains::hrmp, Hrmp]
		[runtime_parachains::disputes, ParasDisputes]
		[runtime_parachains::inclusion, ParaInclusion]
		[runtime_parachains::initializer, Initializer]
		[runtime_parachains::paras_inherent, ParaInherent]
		[runtime_parachains::paras, Paras]
		[runtime_parachains::assigner_on_demand, OnDemandAssignmentProvider]
		// Substrate
		[pallet_balances, Balances]
		[pallet_balances, NisCounterpartBalances]
		[frame_benchmarking::baseline, Baseline::<Runtime>]
		[pallet_bounties, Bounties]
		[pallet_child_bounties, ChildBounties]
		[pallet_conviction_voting, ConvictionVoting]
		[pallet_nis, Nis]
		[pallet_identity, Identity]
		[pallet_indices, Indices]
		[pallet_message_queue, MessageQueue]
		[pallet_multisig, Multisig]
		[pallet_preimage, Preimage]
		[pallet_proxy, Proxy]
		[pallet_ranked_collective, FellowshipCollective]
		[pallet_recovery, Recovery]
		[pallet_referenda, Referenda]
		[pallet_referenda, FellowshipReferenda]
		[pallet_scheduler, Scheduler]
		[pallet_sudo, Sudo]
		[frame_system, SystemBench::<Runtime>]
		[pallet_timestamp, Timestamp]
		[pallet_treasury, Treasury]
		[pallet_utility, Utility]
		[pallet_vesting, Vesting]
		[pallet_asset_rate, AssetRate]
		[pallet_whitelist, Whitelist]
		// XCM
		[pallet_xcm, PalletXcmExtrinsicsBenchmark::<Runtime>]
		[pallet_xcm_benchmarks::fungible, pallet_xcm_benchmarks::fungible::Pallet::<Runtime>]
		[pallet_xcm_benchmarks::generic, pallet_xcm_benchmarks::generic::Pallet::<Runtime>]
	);
}
fn default_parachains_host_configuration(
) -> runtime_parachains::configuration::HostConfiguration<primitives::BlockNumber> {
	use primitives::{MAX_CODE_SIZE, MAX_POV_SIZE};

	runtime_parachains::configuration::HostConfiguration {
		validation_upgrade_cooldown: 2u32,
		validation_upgrade_delay: 2,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
		group_rotation_frequency: 20,
		paras_availability_period: 4,
		max_upward_queue_count: 8,
		max_upward_queue_size: 1024 * 1024,
		max_downward_message_size: 1024 * 1024,
		max_upward_message_size: 50 * 1024,
		max_upward_message_num_per_candidate: 5,
		hrmp_sender_deposit: 0,
		hrmp_recipient_deposit: 0,
		hrmp_channel_max_capacity: 8,
		hrmp_channel_max_total_size: 8 * 1024,
		hrmp_max_parachain_inbound_channels: 4,
		hrmp_channel_max_message_size: 1024 * 1024,
		hrmp_max_parachain_outbound_channels: 4,
		hrmp_max_message_num_per_candidate: 5,
		dispute_period: 6,
		no_show_slots: 2,
		n_delay_tranches: 25,
		needed_approvals: 2,
		relay_vrf_modulo_samples: 2,
		zeroth_delay_tranche_width: 0,
		minimum_validation_upgrade_delay: 5,
		..Default::default()
	}
}

use babe_primitives::AuthorityId as BabeId;
use primitives::AssignmentId;
use sp_core::crypto::Public;
// use sp_core::{sr25519, Pair, Public};
/// Helper function to generate an account ID from seed
// pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
// where
// 	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
// {
// 	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
// }
//
// fn testnet_accounts() -> Vec<AccountId> {
// 	vec![
// 		get_account_id_from_seed::<sr25519::Public>("Alice"),
// 		get_account_id_from_seed::<sr25519::Public>("Bob"),
// 		get_account_id_from_seed::<sr25519::Public>("Charlie"),
// 		get_account_id_from_seed::<sr25519::Public>("Dave"),
// 		get_account_id_from_seed::<sr25519::Public>("Eve"),
// 		get_account_id_from_seed::<sr25519::Public>("Ferdie"),
// 		get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
// 		get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
// 		get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
// 		get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
// 		get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
// 		get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
// 	]
// }
//
fn rococo_session_keys(
	babe: BabeId,
	grandpa: GrandpaId,
	im_online: ImOnlineId,
	para_validator: ValidatorId,
	para_assignment: AssignmentId,
	authority_discovery: AuthorityDiscoveryId,
	beefy: BeefyId,
) -> SessionKeys {
	SessionKeys {
		babe,
		grandpa,
		im_online,
		para_validator,
		para_assignment,
		authority_discovery,
		beefy,
	}
}
pub fn rococo_testnet_genesis(
	initial_authorities: Vec<(
		AccountId,
		AccountId,
		BabeId,
		GrandpaId,
		ImOnlineId,
		ValidatorId,
		AssignmentId,
		AuthorityDiscoveryId,
		BeefyId,
	)>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
) -> serde_json::Value {
	let endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap();

	const ENDOWMENT: u128 = 1_000_000 * UNITS;

	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().map(|k| (k.clone(), ENDOWMENT)).collect::<Vec<_>>(),
		},
		"session": {
			"keys": initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						rococo_session_keys(
							x.2.clone(),
							x.3.clone(),
							x.4.clone(),
							x.5.clone(),
							x.6.clone(),
							x.7.clone(),
							x.8.clone(),
						),
					)
				})
				.collect::<Vec<_>>(),
		},
		"babe": {
			"epochConfig": Some(BABE_GENESIS_EPOCH_CONFIG),
		},
		"sudo": { "key": Some(root_key.clone()) },
		"configuration": {
			"config": runtime_parachains::configuration::HostConfiguration {
				max_validators_per_core: Some(1),
				..default_parachains_host_configuration()
			},
		},
		"registrar": {
			"nextFreeParaId": primitives::LOWEST_PUBLIC_ID,
		}
	})
}

fn rococo_testnet_genesis_staging() -> serde_json::Value {
	let endowed_accounts: sp_std::vec::Vec<AccountId> = vec![
		// 5DwBmEFPXRESyEam5SsQF1zbWSCn2kCjyLW51hJHXe9vW4xs
		hex!["52bc71c1eca5353749542dfdf0af97bf764f9c2f44e860cd485f1cd86400f649"].into(),
	];

	// ./scripts/prepare-test-net.sh 8
	let initial_authorities: Vec<(
		AccountId,
		AccountId,
		BabeId,
		GrandpaId,
		ImOnlineId,
		ValidatorId,
		AssignmentId,
		AuthorityDiscoveryId,
		BeefyId,
	)> = vec![
		(
			//5EHZkbp22djdbuMFH9qt1DVzSCvqi3zWpj6DAYfANa828oei
			hex!["62475fe5406a7cb6a64c51d0af9d3ab5c2151bcae982fb812f7a76b706914d6a"].into(),
			//5FeSEpi9UYYaWwXXb3tV88qtZkmSdB3mvgj3pXkxKyYLGhcd
			hex!["9e6e781a76810fe93187af44c79272c290c2b9e2b8b92ee11466cd79d8023f50"].into(),
			//5Fh6rDpMDhM363o1Z3Y9twtaCPfizGQWCi55BSykTQjGbP7H
			hex!["a076ef1280d768051f21d060623da3ab5b56944d681d303ed2d4bf658c5bed35"]
				.unchecked_into(),
			//5CPd3zoV9Aaah4xWucuDivMHJ2nEEmpdi864nPTiyRZp4t87
			hex!["0e6d7d1afbcc6547b92995a394ba0daed07a2420be08220a5a1336c6731f0bfa"]
				.unchecked_into(),
			//5F7BEa1LGFksUihyatf3dCDYneB8pWzVyavnByCsm5nBgezi
			hex!["86975a37211f8704e947a365b720f7a3e2757988eaa7d0f197e83dba355ef743"]
				.unchecked_into(),
			//5CP6oGfwqbEfML8efqm1tCZsUgRsJztp9L8ZkEUxA16W8PPz
			hex!["0e07a51d3213842f8e9363ce8e444255990a225f87e80a3d651db7841e1a0205"]
				.unchecked_into(),
			//5HQdwiDh8Qtd5dSNWajNYpwDvoyNWWA16Y43aEkCNactFc2b
			hex!["ec60e71fe4a567ef9fef99d4bbf37ffae70564b41aa6f94ef0317c13e0a5477b"]
				.unchecked_into(),
			//5HbSgM72xVuscsopsdeG3sCSCYdAeM1Tay9p79N6ky6vwDGq
			hex!["f49eae66a0ac9f610316906ec8f1a0928e20d7059d76a5ca53cbcb5a9b50dd3c"]
				.unchecked_into(),
			//5DPSWdgw38Spu315r6LSvYCggeeieBAJtP5A1qzuzKhqmjVu
			hex!["034f68c5661a41930c82f26a662276bf89f33467e1c850f2fb8ef687fe43d62276"]
				.unchecked_into(),
		),
		(
			//5DvH8oEjQPYhzCoQVo7WDU91qmQfLZvxe9wJcrojmJKebCmG
			hex!["520b48452969f6ddf263b664de0adb0c729d0e0ad3b0e5f3cb636c541bc9022a"].into(),
			//5ENZvCRzyXJJYup8bM6yEzb2kQHEb1NDpY2ZEyVGBkCfRdj3
			hex!["6618289af7ae8621981ffab34591e7a6486e12745dfa3fd3b0f7e6a3994c7b5b"].into(),
			//5DLjSUfqZVNAADbwYLgRvHvdzXypiV1DAEaDMjcESKTcqMoM
			hex!["38757d0de00a0c739e7d7984ef4bc01161bd61e198b7c01b618425c16bb5bd5f"]
				.unchecked_into(),
			//5HnDVBN9mD6mXyx8oryhDbJtezwNSj1VRXgLoYCBA6uEkiao
			hex!["fcd5f87a6fd5707a25122a01b4dac0a8482259df7d42a9a096606df1320df08d"]
				.unchecked_into(),
			//5DhyXZiuB1LvqYKFgT5tRpgGsN3is2cM9QxgW7FikvakbAZP
			hex!["48a910c0af90898f11bd57d37ceaea53c78994f8e1833a7ade483c9a84bde055"]
				.unchecked_into(),
			//5EPEWRecy2ApL5n18n3aHyU1956zXTRqaJpzDa9DoqiggNwF
			hex!["669a10892119453e9feb4e3f1ee8e028916cc3240022920ad643846fbdbee816"]
				.unchecked_into(),
			//5ES3fw5X4bndSgLNmtPfSbM2J1kLqApVB2CCLS4CBpM1UxUZ
			hex!["68bf52c482630a8d1511f2edd14f34127a7d7082219cccf7fd4c6ecdb535f80d"]
				.unchecked_into(),
			//5HeXbwb5PxtcRoopPZTp5CQun38atn2UudQ8p2AxR5BzoaXw
			hex!["f6f8fe475130d21165446a02fb1dbce3a7bf36412e5d98f4f0473aed9252f349"]
				.unchecked_into(),
			//5F7nTtN8MyJV4UsXpjg7tHSnfANXZ5KRPJmkASc1ZSH2Xoa5
			hex!["03a90c2bb6d3b7000020f6152fe2e5002fa970fd1f42aafb6c8edda8dacc2ea77e"]
				.unchecked_into(),
		),
		(
			//5FPMzsezo1PRxYbVpJMWK7HNbR2kUxidsAAxH4BosHa4wd6S
			hex!["92ef83665b39d7a565e11bf8d18d41d45a8011601c339e57a8ea88c8ff7bba6f"].into(),
			//5G6NQidFG7YiXsvV7hQTLGArir9tsYqD4JDxByhgxKvSKwRx
			hex!["b235f57244230589523271c27b8a490922ffd7dccc83b044feaf22273c1dc735"].into(),
			//5GpZhzAVg7SAtzLvaAC777pjquPEcNy1FbNUAG2nZvhmd6eY
			hex!["d2644c1ab2c63a3ad8d40ad70d4b260969e3abfe6d7e6665f50dc9f6365c9d2a"]
				.unchecked_into(),
			//5HAes2RQYPbYKbLBfKb88f4zoXv6pPA6Ke8CjN7dob3GpmSP
			hex!["e1b68fbd84333e31486c08e6153d9a1415b2e7e71b413702b7d64e9b631184a1"]
				.unchecked_into(),
			//5HTXBf36LXmkFWJLokNUK6fPxVpkr2ToUnB1pvaagdGu4c1T
			hex!["ee93e26259decb89afcf17ef2aa0fa2db2e1042fb8f56ecfb24d19eae8629878"]
				.unchecked_into(),
			//5FtAGDZYJKXkhVhAxCQrXmaP7EE2mGbBMfmKDHjfYDgq2BiU
			hex!["a8e61ffacafaf546283dc92d14d7cc70ea0151a5dd81fdf73ff5a2951f2b6037"]
				.unchecked_into(),
			//5CtK7JHv3h6UQZ44y54skxdwSVBRtuxwPE1FYm7UZVhg8rJV
			hex!["244f3421b310c68646e99cdbf4963e02067601f57756b072a4b19431448c186e"]
				.unchecked_into(),
			//5D4r6YaB6F7A7nvMRHNFNF6zrR9g39bqDJFenrcaFmTCRwfa
			hex!["2c57f81fd311c1ab53813c6817fe67f8947f8d39258252663b3384ab4195494d"]
				.unchecked_into(),
			//5EPoHj8uV4fFKQHYThc6Z9fDkU7B6ih2ncVzQuDdNFb8UyhF
			hex!["039d065fe4f9234f0a4f13cc3ae585f2691e9c25afa469618abb6645111f607a53"]
				.unchecked_into(),
		),
		(
			//5DMNx7RoX6d7JQ38NEM7DWRcW2THu92LBYZEWvBRhJeqcWgR
			hex!["38f3c2f38f6d47f161e98c697bbe3ca0e47c033460afda0dda314ab4222a0404"].into(),
			//5GGdKNDr9P47dpVnmtq3m8Tvowwf1ot1abw6tPsTYYFoKm2v
			hex!["ba0898c1964196474c0be08d364cdf4e9e1d47088287f5235f70b0590dfe1704"].into(),
			//5EjkyPCzR2SjhDZq8f7ufsw6TfkvgNRepjCRQFc4TcdXdaB1
			hex!["764186bc30fd5a02477f19948dc723d6d57ab174debd4f80ed6038ec960bfe21"]
				.unchecked_into(),
			//5DJV3zCBTJBLGNDCcdWrYxWDacSz84goGTa4pFeKVvehEBte
			hex!["36be9069cdb4a8a07ecd51f257875150f0a8a1be44a10d9d98dabf10a030aef4"]
				.unchecked_into(),
			//5FHf8kpK4fPjEJeYcYon2gAPwEBubRvtwpzkUbhMWSweKPUY
			hex!["8e95b9b5b4dc69790b67b566567ca8bf8cdef3a3a8bb65393c0d1d1c87cd2d2c"]
				.unchecked_into(),
			//5F9FsRjpecP9GonktmtFL3kjqNAMKjHVFjyjRdTPa4hbQRZA
			hex!["882d72965e642677583b333b2d173ac94b5fd6c405c76184bb14293be748a13b"]
				.unchecked_into(),
			//5F1FZWZSj3JyTLs8sRBxU6QWyGLSL9BMRtmSKDmVEoiKFxSP
			hex!["821271c99c958b9220f1771d9f5e29af969edfa865631dba31e1ab7bc0582b75"]
				.unchecked_into(),
			//5CtgRR74VypK4h154s369abs78hDUxZSJqcbWsfXvsjcHJNA
			hex!["2496f28d887d84705c6dae98aee8bf90fc5ad10bb5545eca1de6b68425b70f7c"]
				.unchecked_into(),
			//5CPx6dsr11SCJHKFkcAQ9jpparS7FwXQBrrMznRo4Hqv1PXz
			hex!["0307d29bbf6a5c4061c2157b44fda33b7bb4ec52a5a0305668c74688cedf288d58"]
				.unchecked_into(),
		),
		(
			//5C8AL1Zb4bVazgT3EgDxFgcow1L4SJjVu44XcLC9CrYqFN4N
			hex!["02a2d8cfcf75dda85fafc04ace3bcb73160034ed1964c43098fb1fe831de1b16"].into(),
			//5FLYy3YKsAnooqE4hCudttAsoGKbVG3hYYBtVzwMjJQrevPa
			hex!["90cab33f0bb501727faa8319f0845faef7d31008f178b65054b6629fe531b772"].into(),
			//5Et3tfbVf1ByFThNAuUq5pBssdaPPskip5yob5GNyUFojXC7
			hex!["7c94715e5dd8ab54221b1b6b2bfa5666f593f28a92a18e28052531de1bd80813"]
				.unchecked_into(),
			//5EX1JBghGbQqWohTPU6msR9qZ2nYPhK9r3RTQ2oD1K8TCxaG
			hex!["6c878e33b83c20324238d22240f735457b6fba544b383e70bb62a27b57380c81"]
				.unchecked_into(),
			//5GqL8RbVAuNXpDhjQi1KrS1MyNuKhvus2AbmQwRGjpuGZmFu
			hex!["d2f9d537ffa59919a4028afdb627c14c14c97a1547e13e8e82203d2049b15b1a"]
				.unchecked_into(),
			//5EUNaBpX9mJgcmLQHyG5Pkms6tbDiKuLbeTEJS924Js9cA1N
			hex!["6a8570b9c6408e54bacf123cc2bb1b0f087f9c149147d0005badba63a5a4ac01"]
				.unchecked_into(),
			//5CaZuueRVpMATZG4hkcrgDoF4WGixuz7zu83jeBdY3bgWGaG
			hex!["16c69ea8d595e80b6736f44be1eaeeef2ac9c04a803cc4fd944364cb0d617a33"]
				.unchecked_into(),
			//5DABsdQCDUGuhzVGWe5xXzYQ9rtrVxRygW7RXf9Tsjsw1aGJ
			hex!["306ac5c772fe858942f92b6e28bd82fb7dd8cdd25f9a4626c1b0eee075fcb531"]
				.unchecked_into(),
			//5H91T5mHhoCw9JJG4NjghDdQyhC6L7XcSuBWKD3q3TAhEVvQ
			hex!["02fb0330356e63a35dd930bc74525edf28b3bf5eb44aab9e9e4962c8309aaba6a6"]
				.unchecked_into(),
		),
		(
			//5C8XbDXdMNKJrZSrQURwVCxdNdk8AzG6xgLggbzuA399bBBF
			hex!["02ea6bfa8b23b92fe4b5db1063a1f9475e3acd0ab61e6b4f454ed6ba00b5f864"].into(),
			//5GsyzFP8qtF8tXPSsjhjxAeU1v7D1PZofuQKN9TdCc7Dp1JM
			hex!["d4ffc4c05b47d1115ad200f7f86e307b20b46c50e1b72a912ec4f6f7db46b616"].into(),
			//5GHWB8ZDzegLcMW7Gdd1BS6WHVwDdStfkkE4G7KjPjZNJBtD
			hex!["bab3cccdcc34401e9b3971b96a662686cf755aa869a5c4b762199ce531b12c5b"]
				.unchecked_into(),
			//5GzDPGbUM9uH52ZEwydasTj8edokGUJ7vEpoFWp9FE1YNuFB
			hex!["d9c056c98ca0e6b4eb7f5c58c007c1db7be0fe1f3776108f797dd4990d1ccc33"]
				.unchecked_into(),
			//5GWZbVkJEfWZ7fRca39YAQeqri2Z7pkeHyd7rUctUHyQifLp
			hex!["c4a980da30939d5bb9e4a734d12bf81259ae286aa21fa4b65405347fa40eff35"]
				.unchecked_into(),
			//5CmLCFeSurRXXtwMmLcVo7sdJ9EqDguvJbuCYDcHkr3cpqyE
			hex!["1efc23c0b51ad609ab670ecf45807e31acbd8e7e5cb7c07cf49ee42992d2867c"]
				.unchecked_into(),
			//5DnsSy8a8pfE2aFjKBDtKw7WM1V4nfE5sLzP15MNTka53GqS
			hex!["4c64d3f06d28adeb36a892fdaccecace150bec891f04694448a60b74fa469c22"]
				.unchecked_into(),
			//5CZdFnyzZvKetZTeUwj5APAYskVJe4QFiTezo5dQNsrnehGd
			hex!["160ea09c5717270e958a3da42673fa011613a9539b2e4ebcad8626bc117ca04a"]
				.unchecked_into(),
			//5HgoR9JJkdBusxKrrs3zgd3ToppgNoGj1rDyAJp4e7eZiYyT
			hex!["020019a8bb188f8145d02fa855e9c36e9914457d37c500e03634b5223aa5702474"]
				.unchecked_into(),
		),
		(
			//5HinEonzr8MywkqedcpsmwpxKje2jqr9miEwuzyFXEBCvVXM
			hex!["fa373e25a1c4fe19c7148acde13bc3db1811cf656dc086820f3dda736b9c4a00"].into(),
			//5EHJbj6Td6ks5HDnyfN4ttTSi57osxcQsQexm7XpazdeqtV7
			hex!["62145d721967bd88622d08625f0f5681463c0f1b8bcd97eb3c2c53f7660fd513"].into(),
			//5EeCsC58XgJ1DFaoYA1WktEpP27jvwGpKdxPMFjicpLeYu96
			hex!["720537e2c1c554654d73b3889c3ef4c3c2f95a65dd3f7c185ebe4afebed78372"]
				.unchecked_into(),
			//5DnEySxbnppWEyN8cCLqvGjAorGdLRg2VmkY96dbJ1LHFK8N
			hex!["4bea0b37e0cce9bddd80835fa2bfd5606f5dcfb8388bbb10b10c483f0856cf14"]
				.unchecked_into(),
			//5E1Y1FJ7dVP7qtE3wm241pTm72rTMcDT5Jd8Czv7Pwp7N3AH
			hex!["560d90ca51e9c9481b8a9810060e04d0708d246714960439f804e5c6f40ca651"]
				.unchecked_into(),
			//5CAC278tFCHAeHYqE51FTWYxHmeLcENSS1RG77EFRTvPZMJT
			hex!["042f07fc5268f13c026bbe199d63e6ac77a0c2a780f71cda05cee5a6f1b3f11f"]
				.unchecked_into(),
			//5HjRTLWcQjZzN3JDvaj1UzjNSayg5ZD9ZGWMstaL7Ab2jjAa
			hex!["fab485e87ed1537d089df521edf983a777c57065a702d7ed2b6a2926f31da74f"]
				.unchecked_into(),
			//5ELv74v7QcsS6FdzvG4vL2NnYDGWmRnJUSMKYwdyJD7Xcdi7
			hex!["64d59feddb3d00316a55906953fb3db8985797472bd2e6c7ea1ab730cc339d7f"]
				.unchecked_into(),
			//5FaUcPt4fPz93vBhcrCJqmDkjYZ7jCbzAF56QJoCmvPaKrmx
			hex!["033f1a6d47fe86f88934e4b83b9fae903b92b5dcf4fec97d5e3e8bf4f39df03685"]
				.unchecked_into(),
		),
		(
			//5Ey3NQ3dfabaDc16NUv7wRLsFCMDFJSqZFzKVycAsWuUC6Di
			hex!["8062e9c21f1d92926103119f7e8153cebdb1e5ab3e52d6f395be80bb193eab47"].into(),
			//5HiWsuSBqt8nS9pnggexXuHageUifVPKPHDE2arTKqhTp1dV
			hex!["fa0388fa88f3f0cb43d583e2571fbc0edad57dff3a6fd89775451dd2c2b8ea00"].into(),
			//5H168nKX2Yrfo3bxj7rkcg25326Uv3CCCnKUGK6uHdKMdPt8
			hex!["da6b2df18f0f9001a6dcf1d301b92534fe9b1f3ccfa10c49449fee93adaa8349"]
				.unchecked_into(),
			//5DrA2fZdzmNqT5j6DXNwVxPBjDV9jhkAqvjt6Us3bQHKy3cF
			hex!["4ee66173993dd0db5d628c4c9cb61a27b76611ad3c3925947f0d0011ee2c5dcc"]
				.unchecked_into(),
			//5FNFDUGNLUtqg5LgrwYLNmBiGoP8KRxsvQpBkc7GQP6qaBUG
			hex!["92156f54a114ee191415898f2da013d9db6a5362d6b36330d5fc23e27360ab66"]
				.unchecked_into(),
			//5Gx6YeNhynqn8qkda9QKpc9S7oDr4sBrfAu516d3sPpEt26F
			hex!["d822d4088b20dca29a580a577a97d6f024bb24c9550bebdfd7d2d18e946a1c7d"]
				.unchecked_into(),
			//5DhDcHqwxoes5s89AyudGMjtZXx1nEgrk5P45X88oSTR3iyx
			hex!["481538f8c2c011a76d7d57db11c2789a5e83b0f9680dc6d26211d2f9c021ae4c"]
				.unchecked_into(),
			//5DqAvikdpfRdk5rR35ZobZhqaC5bJXZcEuvzGtexAZP1hU3T
			hex!["4e262811acdfe94528bfc3c65036080426a0e1301b9ada8d687a70ffcae99c26"]
				.unchecked_into(),
			//5E41Znrr2YtZu8bZp3nvRuLVHg3jFksfQ3tXuviLku4wsao7
			hex!["025e84e95ed043e387ddb8668176b42f8e2773ddd84f7f58a6d9bf436a4b527986"]
				.unchecked_into(),
		),
	];
	let root_key = endowed_accounts[0].clone();

	//patch
	rococo_testnet_genesis(initial_authorities, root_key, Some(endowed_accounts))
}

use serde_json::Value;
pub fn merge(a: &mut Value, b: Value) {
	match (a, b) {
		(Value::Object(a), Value::Object(b)) =>
			for (k, v) in b {
				if v.is_null() {
					a.remove(&k);
				} else {
					merge(a.entry(k).or_insert(Value::Null), v);
				}
			},
		(a, b) => *a = b,
	};
}
use hex_literal::hex;
use sp_core::crypto::UncheckedInto;

sp_api::impl_runtime_apis! {
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

	impl block_builder_api::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: inherents::InherentData,
			) -> inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl tx_pool_api::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
			) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl offchain_primitives::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	#[api_version(10)]
	impl primitives::runtime_api::ParachainHost<Block> for Runtime {
		fn validators() -> Vec<ValidatorId> {
			parachains_runtime_api_impl::validators::<Runtime>()
		}

		fn validator_groups() -> (Vec<Vec<ValidatorIndex>>, GroupRotationInfo<BlockNumber>) {
			parachains_runtime_api_impl::validator_groups::<Runtime>()
		}

		fn availability_cores() -> Vec<CoreState<Hash, BlockNumber>> {
			parachains_runtime_api_impl::availability_cores::<Runtime>()
		}

		fn persisted_validation_data(para_id: ParaId, assumption: OccupiedCoreAssumption)
			-> Option<PersistedValidationData<Hash, BlockNumber>> {
				parachains_runtime_api_impl::persisted_validation_data::<Runtime>(para_id, assumption)
			}

		fn assumed_validation_data(
			para_id: ParaId,
			expected_persisted_validation_data_hash: Hash,
			) -> Option<(PersistedValidationData<Hash, BlockNumber>, ValidationCodeHash)> {
			parachains_runtime_api_impl::assumed_validation_data::<Runtime>(
				para_id,
				expected_persisted_validation_data_hash,
				)
		}

		fn check_validation_outputs(
			para_id: ParaId,
			outputs: primitives::CandidateCommitments,
			) -> bool {
			parachains_runtime_api_impl::check_validation_outputs::<Runtime>(para_id, outputs)
		}

		fn session_index_for_child() -> SessionIndex {
			parachains_runtime_api_impl::session_index_for_child::<Runtime>()
		}

		fn validation_code(para_id: ParaId, assumption: OccupiedCoreAssumption)
			-> Option<ValidationCode> {
				parachains_runtime_api_impl::validation_code::<Runtime>(para_id, assumption)
			}

		fn candidate_pending_availability(para_id: ParaId) -> Option<CommittedCandidateReceipt<Hash>> {
			parachains_runtime_api_impl::candidate_pending_availability::<Runtime>(para_id)
		}

		fn candidate_events() -> Vec<CandidateEvent<Hash>> {
			parachains_runtime_api_impl::candidate_events::<Runtime, _>(|ev| {
				match ev {
					RuntimeEvent::ParaInclusion(ev) => {
						Some(ev)
					}
					_ => None,
				}
			})
		}

		fn session_info(index: SessionIndex) -> Option<SessionInfo> {
			parachains_runtime_api_impl::session_info::<Runtime>(index)
		}

		fn session_executor_params(session_index: SessionIndex) -> Option<ExecutorParams> {
			parachains_runtime_api_impl::session_executor_params::<Runtime>(session_index)
		}

		fn dmq_contents(recipient: ParaId) -> Vec<InboundDownwardMessage<BlockNumber>> {
			parachains_runtime_api_impl::dmq_contents::<Runtime>(recipient)
		}

		fn inbound_hrmp_channels_contents(
			recipient: ParaId
			) -> BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumber>>> {
			parachains_runtime_api_impl::inbound_hrmp_channels_contents::<Runtime>(recipient)
		}

		fn validation_code_by_hash(hash: ValidationCodeHash) -> Option<ValidationCode> {
			parachains_runtime_api_impl::validation_code_by_hash::<Runtime>(hash)
		}

		fn on_chain_votes() -> Option<ScrapedOnChainVotes<Hash>> {
			parachains_runtime_api_impl::on_chain_votes::<Runtime>()
		}

		fn submit_pvf_check_statement(
			stmt: primitives::PvfCheckStatement,
			signature: primitives::ValidatorSignature
			) {
			parachains_runtime_api_impl::submit_pvf_check_statement::<Runtime>(stmt, signature)
		}

		fn pvfs_require_precheck() -> Vec<ValidationCodeHash> {
			parachains_runtime_api_impl::pvfs_require_precheck::<Runtime>()
		}

		fn validation_code_hash(para_id: ParaId, assumption: OccupiedCoreAssumption)
			-> Option<ValidationCodeHash>
			{
				parachains_runtime_api_impl::validation_code_hash::<Runtime>(para_id, assumption)
			}

		fn disputes() -> Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)> {
			parachains_runtime_api_impl::get_session_disputes::<Runtime>()
		}

		fn unapplied_slashes(
							) -> Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)> {
			parachains_runtime_api_impl::unapplied_slashes::<Runtime>()
		}

		fn key_ownership_proof(
			validator_id: ValidatorId,
			) -> Option<slashing::OpaqueKeyOwnershipProof> {
			use parity_scale_codec::Encode;

			Historical::prove((PARACHAIN_KEY_TYPE_ID, validator_id))
				.map(|p| p.encode())
				.map(slashing::OpaqueKeyOwnershipProof::new)
		}

		fn submit_report_dispute_lost(
			dispute_proof: slashing::DisputeProof,
			key_ownership_proof: slashing::OpaqueKeyOwnershipProof,
			) -> Option<()> {
			parachains_runtime_api_impl::submit_unsigned_slashing_report::<Runtime>(
				dispute_proof,
				key_ownership_proof,
				)
		}

		fn minimum_backing_votes() -> u32 {
			parachains_runtime_api_impl::minimum_backing_votes::<Runtime>()
		}

		fn para_backing_state(para_id: ParaId) -> Option<primitives::async_backing::BackingState> {
			parachains_runtime_api_impl::backing_state::<Runtime>(para_id)
		}

		fn async_backing_params() -> primitives::AsyncBackingParams {
			parachains_runtime_api_impl::async_backing_params::<Runtime>()
		}

		fn approval_voting_params() -> ApprovalVotingParams {
			parachains_staging_runtime_api_impl::approval_voting_params::<Runtime>()
		}

		fn disabled_validators() -> Vec<ValidatorIndex> {
			parachains_staging_runtime_api_impl::disabled_validators::<Runtime>()
		}

		fn node_features() -> NodeFeatures {
			parachains_staging_runtime_api_impl::node_features::<Runtime>()
		}
	}

	#[api_version(3)]
	impl beefy_primitives::BeefyApi<Block, BeefyId> for Runtime {
		fn beefy_genesis() -> Option<BlockNumber> {
			Beefy::genesis_block()
		}

		fn validator_set() -> Option<beefy_primitives::ValidatorSet<BeefyId>> {
			Beefy::validator_set()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: beefy_primitives::EquivocationProof<
			BlockNumber,
			BeefyId,
			BeefySignature,
			>,
			key_owner_proof: beefy_primitives::OpaqueKeyOwnershipProof,
			) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Beefy::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
				)
		}

		fn generate_key_ownership_proof(
			_set_id: beefy_primitives::ValidatorSetId,
			authority_id: BeefyId,
			) -> Option<beefy_primitives::OpaqueKeyOwnershipProof> {
			use parity_scale_codec::Encode;

			Historical::prove((beefy_primitives::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(beefy_primitives::OpaqueKeyOwnershipProof::new)
		}
	}

	#[api_version(2)]
	impl mmr::MmrApi<Block, mmr::Hash, BlockNumber> for Runtime {
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

	impl fg_primitives::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> Vec<(GrandpaId, u64)> {
			Grandpa::grandpa_authorities()
		}

		fn current_set_id() -> fg_primitives::SetId {
			Grandpa::current_set_id()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: fg_primitives::EquivocationProof<
			<Block as BlockT>::Hash,
			sp_runtime::traits::NumberFor<Block>,
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
			authority_id: fg_primitives::AuthorityId,
			) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
			use parity_scale_codec::Encode;

			Historical::prove((fg_primitives::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(fg_primitives::OpaqueKeyOwnershipProof::new)
		}
	}

	impl babe_primitives::BabeApi<Block> for Runtime {
		fn configuration() -> babe_primitives::BabeConfiguration {
			let epoch_config = Babe::epoch_config().unwrap_or(BABE_GENESIS_EPOCH_CONFIG);
			babe_primitives::BabeConfiguration {
				slot_duration: Babe::slot_duration(),
				epoch_length: EpochDurationInBlocks::get().into(),
				c: epoch_config.c,
				authorities: Babe::authorities().to_vec(),
				randomness: Babe::randomness(),
				allowed_slots: epoch_config.allowed_slots,
			}
		}

		fn current_epoch_start() -> babe_primitives::Slot {
			Babe::current_epoch_start()
		}

		fn current_epoch() -> babe_primitives::Epoch {
			Babe::current_epoch()
		}

		fn next_epoch() -> babe_primitives::Epoch {
			Babe::next_epoch()
		}

		fn generate_key_ownership_proof(
			_slot: babe_primitives::Slot,
			authority_id: babe_primitives::AuthorityId,
			) -> Option<babe_primitives::OpaqueKeyOwnershipProof> {
			use parity_scale_codec::Encode;

			Historical::prove((babe_primitives::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(babe_primitives::OpaqueKeyOwnershipProof::new)
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: babe_primitives::EquivocationProof<<Block as BlockT>::Header>,
			key_owner_proof: babe_primitives::OpaqueKeyOwnershipProof,
			) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Babe::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
				)
		}
	}

	impl authority_discovery_primitives::AuthorityDiscoveryApi<Block> for Runtime {
		fn authorities() -> Vec<AuthorityDiscoveryId> {
			parachains_runtime_api_impl::relevant_authority_ids::<Runtime>()
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

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
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

	impl pallet_beefy_mmr::BeefyMmrApi<Block, Hash> for RuntimeApi {
		fn authority_set_proof() -> beefy_primitives::mmr::BeefyAuthoritySet<Hash> {
			MmrLeaf::authority_set_proof()
		}

		fn next_authority_set_proof() -> beefy_primitives::mmr::BeefyNextAuthoritySet<Hash> {
			MmrLeaf::next_authority_set_proof()
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			log::info!("try-runtime::on_runtime_upgrade rococo.");
			let weight = Executive::try_runtime_upgrade(checks).unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect,
			) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).unwrap()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
			) {
			use frame_benchmarking::{Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;

			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_benchmarking::baseline::Pallet as Baseline;

			use pallet_xcm::benchmarking::Pallet as PalletXcmExtrinsicsBenchmark;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();
			return (list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig,
			) -> Result<
			Vec<frame_benchmarking::BenchmarkBatch>,
			sp_runtime::RuntimeString,
		> {
			use frame_support::traits::WhitelistedStorageKeys;
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, BenchmarkError};
			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_benchmarking::baseline::Pallet as Baseline;
			use pallet_xcm::benchmarking::Pallet as PalletXcmExtrinsicsBenchmark;
			use sp_storage::TrackedStorageKey;
			use xcm::latest::prelude::*;
			use xcm_config::{
				AssetHub, LocalCheckAccount, LocationConverter, TokenLocation, XcmConfig,
			};

			parameter_types! {
				pub ExistentialDepositMultiAsset: Option<MultiAsset> = Some((
					TokenLocation::get(),
					ExistentialDeposit::get()
				).into());
				pub ToParachain: ParaId = rococo_runtime_constants::system_parachain::ASSET_HUB_ID.into();
			}

			impl frame_system_benchmarking::Config for Runtime {}
			impl frame_benchmarking::baseline::Config for Runtime {}
			impl pallet_xcm::benchmarking::Config for Runtime {
				fn reachable_dest() -> Option<MultiLocation> {
					Some(crate::xcm_config::AssetHub::get())
				}

				fn teleportable_asset_and_dest() -> Option<(MultiAsset, MultiLocation)> {
					// Relay/native token can be teleported to/from AH.
					Some((
						MultiAsset {
							fun: Fungible(EXISTENTIAL_DEPOSIT),
							id: Concrete(Here.into())
						},
						crate::xcm_config::AssetHub::get(),
					))
				}

				fn reserve_transferable_asset_and_dest() -> Option<(MultiAsset, MultiLocation)> {
					// Relay can reserve transfer native token to some random parachain.
					Some((
						MultiAsset {
							fun: Fungible(EXISTENTIAL_DEPOSIT),
							id: Concrete(Here.into())
						},
						Parachain(43211234).into(),
					))
				}

				fn set_up_complex_asset_transfer(
				) -> Option<(MultiAssets, u32, MultiLocation, Box<dyn FnOnce()>)> {
					// Relay supports only native token, either reserve transfer it to non-system parachains,
					// or teleport it to system parachain. Use the teleport case for benchmarking as it's
					// slightly heavier.
					// Relay/native token can be teleported to/from AH.
					let native_location = Here.into();
					let dest = crate::xcm_config::AssetHub::get();
					pallet_xcm::benchmarking::helpers::native_teleport_as_asset_transfer::<Runtime>(
						native_location,
						dest
					)
				}
			}
			impl pallet_xcm_benchmarks::Config for Runtime {
				type XcmConfig = XcmConfig;
				type AccountIdConverter = LocationConverter;
				type DeliveryHelper = runtime_common::xcm_sender::ToParachainDeliveryHelper<
					XcmConfig,
					ExistentialDepositMultiAsset,
					xcm_config::PriceForChildParachainDelivery,
					ToParachain,
					(),
				>;
				fn valid_destination() -> Result<MultiLocation, BenchmarkError> {
					Ok(AssetHub::get())
				}
				fn worst_case_holding(_depositable_count: u32) -> MultiAssets {
					// Rococo only knows about ROC
					vec![MultiAsset{
						id: Concrete(TokenLocation::get()),
						fun: Fungible(1_000_000 * UNITS),
					}].into()
				}
			}

			parameter_types! {
				pub const TrustedTeleporter: Option<(MultiLocation, MultiAsset)> = Some((
					AssetHub::get(),
					MultiAsset { fun: Fungible(1 * UNITS), id: Concrete(TokenLocation::get()) },
				));
				pub const TrustedReserve: Option<(MultiLocation, MultiAsset)> = None;
			}

			impl pallet_xcm_benchmarks::fungible::Config for Runtime {
				type TransactAsset = Balances;

				type CheckedAccount = LocalCheckAccount;
				type TrustedTeleporter = TrustedTeleporter;
				type TrustedReserve = TrustedReserve;

				fn get_multi_asset() -> MultiAsset {
					MultiAsset {
						id: Concrete(TokenLocation::get()),
						fun: Fungible(1 * UNITS),
					}
				}

				parameter_types! {
					pub const TrustedTeleporter: Option<(MultiLocation, MultiAsset)> = Some((
							AssetHub::get(),
							MultiAsset { fun: Fungible(1 * UNITS), id: Concrete(TokenLocation::get()) },
							));
					pub const TrustedReserve: Option<(MultiLocation, MultiAsset)> = None;
				}

				impl pallet_xcm_benchmarks::fungible::Config for Runtime {
					type TransactAsset = Balances;

					type CheckedAccount = LocalCheckAccount;
					type TrustedTeleporter = TrustedTeleporter;
					type TrustedReserve = TrustedReserve;

					fn get_multi_asset() -> MultiAsset {
						MultiAsset {
							id: Concrete(TokenLocation::get()),
							fun: Fungible(1 * UNITS),
						}
					}
				}

				impl pallet_xcm_benchmarks::generic::Config for Runtime {
					type RuntimeCall = RuntimeCall;

					fn worst_case_response() -> (u64, Response) {
						(0u64, Response::Version(Default::default()))
					}

					fn worst_case_asset_exchange() -> Result<(MultiAssets, MultiAssets), BenchmarkError> {
						// Rococo doesn't support asset exchanges
						Err(BenchmarkError::Skip)
					}

					fn universal_alias() -> Result<(MultiLocation, Junction), BenchmarkError> {
						// The XCM executor of Rococo doesn't have a configured `UniversalAliases`
						Err(BenchmarkError::Skip)
					}

					fn transact_origin_and_runtime_call() -> Result<(MultiLocation, RuntimeCall), BenchmarkError> {
						Ok((AssetHub::get(), frame_system::Call::remark_with_event { remark: vec![] }.into()))
					}

					fn subscribe_origin() -> Result<MultiLocation, BenchmarkError> {
						Ok(AssetHub::get())
					}

					fn claimable_asset() -> Result<(MultiLocation, MultiLocation, MultiAssets), BenchmarkError> {
						let origin = AssetHub::get();
						let assets: MultiAssets = (Concrete(TokenLocation::get()), 1_000 * UNITS).into();
						let ticket = MultiLocation { parents: 0, interior: Here };
						Ok((origin, ticket, assets))
					}

					fn unlockable_asset() -> Result<(MultiLocation, MultiLocation, MultiAsset), BenchmarkError> {
						// Rococo doesn't support asset locking
						Err(BenchmarkError::Skip)
					}

					fn export_message_origin_and_destination(
															) -> Result<(MultiLocation, NetworkId, InteriorMultiLocation), BenchmarkError> {
						// Rococo doesn't support exporting messages
						Err(BenchmarkError::Skip)
					}

					fn alias_origin() -> Result<(MultiLocation, MultiLocation), BenchmarkError> {
						// The XCM executor of Rococo doesn't have a configured `Aliasers`
						Err(BenchmarkError::Skip)
					}
				}

				let mut whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();
				let treasury_key = frame_system::Account::<Runtime>::hashed_key_for(Treasury::account_id());
				whitelist.push(treasury_key.to_vec().into());

				let mut batches = Vec::<BenchmarkBatch>::new();
				let params = (&config, &whitelist);

				add_benchmarks!(params, batches);

				Ok(batches)
			}

			impl pallet_xcm_benchmarks::generic::Config for Runtime {
				type TransactAsset = Balances;
				type RuntimeCall = RuntimeCall;

				fn worst_case_response() -> (u64, Response) {
					(0u64, Response::Version(Default::default()))
				}

				fn worst_case_asset_exchange() -> Result<(MultiAssets, MultiAssets), BenchmarkError> {
					// Rococo doesn't support asset exchanges
					Err(BenchmarkError::Skip)
				}

				fn universal_alias() -> Result<(MultiLocation, Junction), BenchmarkError> {
					// The XCM executor of Rococo doesn't have a configured `UniversalAliases`
					Err(BenchmarkError::Skip)
				}

				fn transact_origin_and_runtime_call() -> Result<(MultiLocation, RuntimeCall), BenchmarkError> {
					Ok((AssetHub::get(), frame_system::Call::remark_with_event { remark: vec![] }.into()))
				}

				fn subscribe_origin() -> Result<MultiLocation, BenchmarkError> {
					Ok(AssetHub::get())
				}

				fn claimable_asset() -> Result<(MultiLocation, MultiLocation, MultiAssets), BenchmarkError> {
					let origin = AssetHub::get();
					let assets: MultiAssets = (Concrete(TokenLocation::get()), 1_000 * UNITS).into();
					let ticket = MultiLocation { parents: 0, interior: Here };
					Ok((origin, ticket, assets))
				}

				fn unlockable_asset() -> Result<(MultiLocation, MultiLocation, MultiAsset), BenchmarkError> {
					// Rococo doesn't support asset locking
					Err(BenchmarkError::Skip)
				}

				fn export_message_origin_and_destination(
				) -> Result<(MultiLocation, NetworkId, InteriorMultiLocation), BenchmarkError> {
					// Rococo doesn't support exporting messages
					Err(BenchmarkError::Skip)
				}

				fn alias_origin() -> Result<(MultiLocation, MultiLocation), BenchmarkError> {
					// The XCM executor of Rococo doesn't have a configured `Aliasers`
					Err(BenchmarkError::Skip)
				}
			}

			let mut whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();
			let treasury_key = frame_system::Account::<Runtime>::hashed_key_for(Treasury::account_id());
			whitelist.push(treasury_key.to_vec().into());

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);

			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn create_default_config() -> Vec<u8> {
			create_default_config::<RuntimeGenesisConfig>()
		}

		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn create_default_config2(params: sp_std::vec::Vec<u8>) -> sp_std::vec::Vec<u8> {
			let default = serde_json::to_string(&RuntimeGenesisConfig::default())
				.expect("serialization to json is expected to work. qed.")
				.into_bytes();
			let mut default =
			serde_json::from_slice(&default[..]).expect("returned value is json. qed.");

			log::info!("xxx: create_default_config2: {:?} {:?}", params, "staging".as_bytes());
			if params == "staging".as_bytes() {
				let patch = rococo_testnet_genesis_staging();
				merge(&mut default, patch);
			}


			serde_json::to_string(&default)
				.expect("serialization to json is expected to work. qed.")
				.into_bytes()
		}
	}
}

#[cfg(all(test, feature = "try-runtime"))]
mod remote_tests {
	use super::*;
	use frame_try_runtime::{runtime_decl_for_try_runtime::TryRuntime, UpgradeCheckSelect};
	use remote_externalities::{
		Builder, Mode, OfflineConfig, OnlineConfig, SnapshotConfig, Transport,
	};
	use std::env::var;

	#[tokio::test]
	async fn run_migrations() {
		if var("RUN_MIGRATION_TESTS").is_err() {
			return
		}

		sp_tracing::try_init_simple();
		let transport: Transport =
			var("WS").unwrap_or("wss://rococo-rpc.polkadot.io:443".to_string()).into();
		let maybe_state_snapshot: Option<SnapshotConfig> = var("SNAP").map(|s| s.into()).ok();
		let mut ext = Builder::<Block>::default()
			.mode(if let Some(state_snapshot) = maybe_state_snapshot {
				Mode::OfflineOrElseOnline(
					OfflineConfig { state_snapshot: state_snapshot.clone() },
					OnlineConfig {
						transport,
						state_snapshot: Some(state_snapshot),
						..Default::default()
					},
				)
			} else {
				Mode::Online(OnlineConfig { transport, ..Default::default() })
			})
			.build()
			.await
			.unwrap();
		ext.execute_with(|| Runtime::on_runtime_upgrade(UpgradeCheckSelect::PreAndPost));
	}
}
