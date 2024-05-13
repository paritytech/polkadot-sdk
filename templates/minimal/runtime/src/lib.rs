// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A minimal runtime that includes the template [`pallet`](`pallet_minimal_template`).

#![cfg_attr(not(feature = "std"), no_std)]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use frame::{
	deps::{
		codec::Compact,
		frame_support::{
			genesis_builder_helper::{build_state, get_preset},
			runtime,
			weights::{constants::WEIGHT_REF_TIME_PER_SECOND, IdentityFee},
			traits::AsEnsureOriginWithArg,
		},
	},
	prelude::*,
	runtime::{
		apis::{
			self, impl_runtime_apis, ApplyExtrinsicResult, CheckInherentsResult,
			ExtrinsicInclusionMode, OpaqueMetadata,
		},
		prelude::*,
	},
	traits::{Convert, IdentifyAccount, One, Verify},
};
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{MultiSignature, Perbill};

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance of an account.
pub type Balance = u64;

/// The runtime version.
#[runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("minimal-template-runtime"),
	impl_name: create_runtime_str!("minimal-template-runtime"),
	authoring_version: 1,
	spec_version: 0,
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

/// The signed extensions that are added to the runtime.
pub type SignedExtra = (
	// Checks that the sender is not the zero address.
	frame_system::CheckNonZeroSender<Runtime>,
	// Checks that the runtime version is correct.
	frame_system::CheckSpecVersion<Runtime>,
	// Checks that the transaction version is correct.
	frame_system::CheckTxVersion<Runtime>,
	// Checks that the genesis hash is correct.
	frame_system::CheckGenesis<Runtime>,
	// Checks that the era is valid.
	frame_system::CheckEra<Runtime>,
	// Checks that the nonce is valid.
	frame_system::CheckNonce<Runtime>,
	// Checks that the weight is valid.
	frame_system::CheckWeight<Runtime>,
	// Ensures that the sender has enough funds to pay for the transaction
	// and deducts the fee from the sender's account.
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

// Composes the runtime by adding all the used pallets and deriving necessary types.
#[runtime]
mod runtime {
	/// The main runtime type.
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Runtime;

	/// Mandatory system pallet that should always be included in a FRAME runtime.
	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	/// Provides a way for consensus systems to set and check the onchain time.
	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp;

	/// Provides the ability to keep track of balances.
	#[runtime::pallet_index(2)]
	pub type Balances = pallet_balances;

	/// Provides a way to execute privileged functions.
	#[runtime::pallet_index(3)]
	pub type Sudo = pallet_sudo;

	/// Provides the ability to charge for extrinsic execution.
	#[runtime::pallet_index(4)]
	pub type TransactionPayment = pallet_transaction_payment;

	// /// A minimal pallet template.
	// #[runtime::pallet_index(5)]
	// pub type Template = pallet_minimal_template;

	#[runtime::pallet_index(6)]
	pub type Assets = pallet_assets;

	#[runtime::pallet_index(7)]
	pub type Voting = pallet_voting;

	#[runtime::pallet_index(8)]
	pub type Multisig = pallet_multisig_template;

	#[runtime::pallet_index(9)]
	pub type FreeTx = pallet_free_tx;

	#[runtime::pallet_index(10)]
	pub type Aura = pallet_aura;

	#[runtime::pallet_index(11)]
	pub type Dpos = pallet_dpos;

	#[runtime::pallet_index(12)]
	pub type Dex = pallet_dex;

	#[runtime::pallet_index(13)]
	pub type Treasury = pallet_treasury_template;
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::with_sensible_defaults(
			Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
			NORMAL_DISPATCH_RATIO,
		);
}

/// Implements the types required for the system pallet.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type Version = Version;
	// Use the account data from the balances pallet
	type AccountData = pallet_balances::AccountData<<Runtime as pallet_balances::Config>::Balance>;
}

// Implements the types required for the balances pallet.
#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
}

// Implements the types required for the sudo pallet.
#[derive_impl(pallet_sudo::config_preludes::TestDefaultConfig)]
impl pallet_sudo::Config for Runtime {}

// Implements the types required for the sudo pallet.
#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {}

parameter_types! {
	pub FeeMultiplier: Multiplier = Multiplier::one();
}

// Implements the types required for the transaction payment pallet.
#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = IdentityFee<Balance>;
	type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
}

// // Implements the types required for the template pallet.
// impl pallet_minimal_template::Config for Runtime {}

parameter_types! {
	pub const AssetDeposit: Balance = 100;
	pub const ApprovalDeposit: Balance = 1;
	pub const StringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = 10;
	pub const MetadataDepositPerByte: Balance = 1;
}

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = u128;
	type AssetId = u32;
	type AssetIdParameter = Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = ConstU64<1>;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = StringLimit;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// Configure the pallet-voting in pallets/voting.
impl pallet_voting::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type BlockNumberToBalance = frame::traits::ConvertInto;
}

/// Configure the pallet-multisig in pallets/multisig.
impl pallet_multisig_template::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type RuntimeCall = RuntimeCall;
}

/// Configure the pallet-free-tx in pallets/free-tx.
impl pallet_free_tx::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type RuntimeCall = RuntimeCall;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<32>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

pub struct AuthorityToAccount;

impl Convert<AuraId, AccountId> for AuthorityToAccount {
	fn convert(authority: AuraId) -> AccountId {
		// The proper way to handle this is with Session + Aura + Authorship pallets.
		// But this is good enough.
		Decode::decode(&mut &authority.encode()[..])
			.expect("We expect every authority id from aura to be the same an an account id.")
	}
}

/// Configure the pallet-dpos in pallets/dpos.
impl pallet_dpos::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AuthorityToAccount = AuthorityToAccount;
	type NativeBalance = Balances;
}

/// Configure the pallet-dex in pallets/dex.
impl pallet_dex::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type Fungibles = Assets;
}

/// Configure the pallet-treasury in pallets/treasury.
impl pallet_treasury_template::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type NativeBalance = Balances;
	type Fungibles = Assets;
}

type Block = frame::runtime::types_common::BlockOf<Runtime, SignedExtra>;
type Header = HeaderFor<Runtime>;

type RuntimeExecutive =
	Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, AllPalletsWithSystem>;

use pallet_transaction_payment::{FeeDetails, RuntimeDispatchInfo};

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	frame::deps::frame_benchmarking::define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
		[pallet_timestamp, Timestamp]
		[pallet_sudo, Sudo]
		[pallet_dex, Dex]
		[pallet_dpos, Dpos]
		[pallet_voting, Voting]
		[pallet_multisig, Multisig]
		[pallet_free_tx, FreeTx]
		[pallet_treasury, Treasury]
	);
}

impl_runtime_apis! {
	impl apis::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			RuntimeExecutive::execute_block(block)
		}

		fn initialize_block(header: &Header) -> ExtrinsicInclusionMode {
			RuntimeExecutive::initialize_block(header)
		}
	}
	impl apis::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl apis::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: ExtrinsicFor<Runtime>) -> ApplyExtrinsicResult {
			RuntimeExecutive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> HeaderFor<Runtime> {
			RuntimeExecutive::finalize_block()
		}

		fn inherent_extrinsics(data: InherentData) -> Vec<ExtrinsicFor<Runtime>> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: InherentData,
		) -> CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl apis::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: ExtrinsicFor<Runtime>,
			block_hash: <Runtime as frame_system::Config>::Hash,
		) -> TransactionValidity {
			RuntimeExecutive::validate_transaction(source, tx, block_hash)
		}
	}

	impl apis::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &HeaderFor<Runtime>) {
			RuntimeExecutive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			pallet_aura::Authorities::<Runtime>::get().into_inner()
		}
	}

	impl apis::SessionKeys<Block> for Runtime {
		fn generate_session_keys(_seed: Option<Vec<u8>>) -> Vec<u8> {
			Default::default()
		}

		fn decode_session_keys(
			_encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, apis::KeyTypeId)>> {
			Default::default()
		}
	}

	impl apis::AccountNonceApi<Block, interface::AccountId, interface::Nonce> for Runtime {
		fn account_nonce(account: interface::AccountId) -> interface::Nonce {
			System::account_nonce(account)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<
		Block,
		interface::Balance,
	> for Runtime {
		fn query_info(uxt: ExtrinsicFor<Runtime>, len: u32) -> RuntimeDispatchInfo<interface::Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(uxt: ExtrinsicFor<Runtime>, len: u32) -> FeeDetails<interface::Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
		fn query_weight_to_fee(weight: Weight) -> interface::Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> interface::Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame::deps::frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame::deps::frame_benchmarking::BenchmarkList>,
			Vec<frame::deps::frame_support::traits::StorageInfo>,
		) {
			use frame::deps::frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
			use frame::deps::frame_support::traits::StorageInfoTrait;
			use frame::deps::frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame::deps::frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame::deps::frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame::deps::frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch};
			use frame::deps::sp_storage::TrackedStorageKey;
			use frame::deps::frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;

			impl frame::deps::frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

			use frame::deps::frame_support::traits::WhitelistedStorageKeys;
			let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame::deps::frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame::deps::frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = RuntimeExecutive::try_runtime_upgrade(checks).unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame::deps::frame_try_runtime::TryStateSelect
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			RuntimeExecutive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			get_preset::<RuntimeGenesisConfig>(id, |_| None)
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			vec![]
		}
	}
}

/// Some re-exports that the node side code needs to know. Some are useful in this context as well.
///
/// Other types should preferably be private.
// TODO: this should be standardized in some way, see:
// https://github.com/paritytech/substrate/issues/10579#issuecomment-1600537558
pub mod interface {
	use super::Runtime;
	use frame::deps::frame_system;

	/// Unchecked extrinsic type as expected by this runtime.
	pub type UncheckedExtrinsic =
		sp_runtime::generic::UncheckedExtrinsic<sp_runtime::MultiAddress<super::AccountId, ()>, super::RuntimeCall, super::Signature, super::SignedExtra>;
	/// The payload being signed in transactions.
	pub type SignedPayload = sp_runtime::generic::SignedPayload<super::RuntimeCall, super::SignedExtra>;

	pub type Block = super::Block;
	pub type BlockHashCount = <Runtime as frame_system::Config>::BlockHashCount;
	pub use frame::runtime::types_common::OpaqueBlock;
	pub type AccountId = <Runtime as frame_system::Config>::AccountId;
	pub type Nonce = <Runtime as frame_system::Config>::Nonce;
	pub type Hash = <Runtime as frame_system::Config>::Hash;
	pub type Balance = <Runtime as pallet_balances::Config>::Balance;
	pub type MinimumBalance = <Runtime as pallet_balances::Config>::ExistentialDeposit;

	pub use frame_system::Call as SystemCall;
	pub use pallet_balances::Call as BalancesCall;

	pub use pallet_transaction_payment::ChargeTransactionPayment as ChargeTransactionPayment;
}
