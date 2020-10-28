// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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
// Runtime-generated DecodeLimit::decode_all_With_depth_limit
#![allow(clippy::unnecessary_mut_passed)]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod exchange;

#[cfg(feature = "runtime-benchmarks")]
pub mod benches;
pub mod kovan;
pub mod millau;
pub mod millau_messages;
pub mod rialto_poa;

use codec::Decode;
use pallet_grandpa::{fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList};
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::traits::{Block as BlockT, IdentityLookup, NumberFor, OpaqueKeys};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, MultiSignature, MultiSigner,
};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// A few exports that help ease life for downstream crates.
pub use frame_support::{
	construct_runtime, parameter_types,
	traits::{Currency, ExistenceRequirement, Imbalance, KeyOwnerProofSystem, Randomness},
	weights::{IdentityFee, RuntimeDbWeight, Weight},
	StorageValue,
};

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;
pub use pallet_bridge_currency_exchange::Call as BridgeCurrencyExchangeCall;
pub use pallet_bridge_eth_poa::Call as BridgeEthPoACall;
pub use pallet_substrate_bridge::Call as BridgeMillauCall;
pub use pallet_timestamp::Call as TimestampCall;

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

/// Index of a transaction in the chain.
pub type Index = u32;

/// A hash of some data used by the chain.
pub type Hash = bp_rialto::Hash;

/// Hashing algorithm used by the chain.
pub type Hashing = bp_rialto::Hasher;

/// Digest item type.
pub type DigestItem = generic::DigestItem<Hash>;

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
		pub grandpa: Grandpa,
	}
}

/// This runtime version.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("rialto-runtime"),
	impl_name: create_runtime_str!("rialto-runtime"),
	authoring_version: 1,
	spec_version: 1,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
};

pub const MILLISECS_PER_BLOCK: u64 = 6000;

pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

// These time units are defined in number of blocks.
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion {
		runtime_version: VERSION,
		can_author_with: Default::default(),
	}
}

parameter_types! {
	pub const BlockHashCount: BlockNumber = 250;
	pub const MaximumBlockWeight: Weight = bp_rialto::MAXIMUM_BLOCK_WEIGHT;
	pub const ExtrinsicBaseWeight: Weight = 10_000_000;
	pub const AvailableBlockRatio: Perbill = Perbill::from_percent(bp_rialto::AVAILABLE_BLOCK_RATIO);
	pub MaximumExtrinsicWeight: Weight = bp_rialto::MAXIMUM_EXTRINSIC_WEIGHT;
	pub const MaximumBlockLength: u32 = 5 * 1024 * 1024;
	pub const Version: RuntimeVersion = VERSION;
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 60_000_000, // ~0.06 ms = ~60 µs
		write: 200_000_000, // ~0.2 ms = 200 µs
	};
}

impl frame_system::Trait for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = ();
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type Call = Call;
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
	type Event = Event;
	/// The ubiquitous origin type.
	type Origin = Origin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// Maximum weight of each block.
	type MaximumBlockWeight = MaximumBlockWeight;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = DbWeight;
	/// The weight of the overhead invoked on the block import process, independent of the
	/// extrinsics included in that block.
	type BlockExecutionWeight = ();
	/// The base weight of any extrinsic processed by the runtime, independent of the
	/// logic of that extrinsic. (Signature verification, nonce increment, fee, etc...)
	type ExtrinsicBaseWeight = ExtrinsicBaseWeight;
	/// The maximum weight that a single extrinsic of `Normal` dispatch class can have,
	/// idependent of the logic of that extrinsics. (Roughly max block weight - average on
	/// initialize cost).
	type MaximumExtrinsicWeight = MaximumExtrinsicWeight;
	/// Maximum size of all encoded transactions (in bytes) that are allowed in one block.
	type MaximumBlockLength = MaximumBlockLength;
	/// Portion of the block weight that is available to all normal transactions.
	type AvailableBlockRatio = AvailableBlockRatio;
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
}

impl pallet_aura::Trait for Runtime {
	type AuthorityId = AuraId;
}

type RialtoPoA = pallet_bridge_eth_poa::Instance1;
impl pallet_bridge_eth_poa::Trait<RialtoPoA> for Runtime {
	type AuraConfiguration = rialto_poa::BridgeAuraConfiguration;
	type FinalityVotesCachingInterval = rialto_poa::FinalityVotesCachingInterval;
	type ValidatorsConfiguration = rialto_poa::BridgeValidatorsConfiguration;
	type PruningStrategy = rialto_poa::PruningStrategy;
	type ChainTime = rialto_poa::ChainTime;
	type OnHeadersSubmitted = ();
}

type Kovan = pallet_bridge_eth_poa::Instance2;
impl pallet_bridge_eth_poa::Trait<Kovan> for Runtime {
	type AuraConfiguration = kovan::BridgeAuraConfiguration;
	type FinalityVotesCachingInterval = kovan::FinalityVotesCachingInterval;
	type ValidatorsConfiguration = kovan::BridgeValidatorsConfiguration;
	type PruningStrategy = kovan::PruningStrategy;
	type ChainTime = kovan::ChainTime;
	type OnHeadersSubmitted = ();
}

type RialtoCurrencyExchange = pallet_bridge_currency_exchange::Instance1;
impl pallet_bridge_currency_exchange::Trait<RialtoCurrencyExchange> for Runtime {
	type OnTransactionSubmitted = ();
	type PeerBlockchain = rialto_poa::RialtoBlockchain;
	type PeerMaybeLockFundsTransaction = exchange::EthTransaction;
	type RecipientsMap = bp_currency_exchange::IdentityRecipients<AccountId>;
	type Amount = Balance;
	type CurrencyConverter = bp_currency_exchange::IdentityCurrencyConverter<Balance>;
	type DepositInto = DepositInto;
}

type KovanCurrencyExchange = pallet_bridge_currency_exchange::Instance2;
impl pallet_bridge_currency_exchange::Trait<KovanCurrencyExchange> for Runtime {
	type OnTransactionSubmitted = ();
	type PeerBlockchain = kovan::KovanBlockchain;
	type PeerMaybeLockFundsTransaction = exchange::EthTransaction;
	type RecipientsMap = bp_currency_exchange::IdentityRecipients<AccountId>;
	type Amount = Balance;
	type CurrencyConverter = bp_currency_exchange::IdentityCurrencyConverter<Balance>;
	type DepositInto = DepositInto;
}

impl pallet_bridge_call_dispatch::Trait for Runtime {
	type Event = Event;
	type MessageId = (bp_message_lane::LaneId, bp_message_lane::MessageNonce);
	type Call = Call;
	type SourceChainAccountPublic = MultiSigner;
	type TargetChainAccountPublic = MultiSigner;
	type TargetChainSignature = MultiSignature;
}

pub struct DepositInto;

impl bp_currency_exchange::DepositInto for DepositInto {
	type Recipient = AccountId;
	type Amount = Balance;

	fn deposit_into(recipient: Self::Recipient, amount: Self::Amount) -> bp_currency_exchange::Result<()> {
		// let balances module make all checks for us (it won't allow depositing lower than existential
		// deposit, balance overflow, ...)
		let deposited = <pallet_balances::Module<Runtime> as Currency<AccountId>>::deposit_creating(&recipient, amount);

		// I'm dropping deposited here explicitly to illustrate the fact that it'll update `TotalIssuance`
		// on drop
		let deposited_amount = deposited.peek();
		drop(deposited);

		// we have 3 cases here:
		// - deposited == amount: success
		// - deposited == 0: deposit has failed and no changes to storage were made
		// - deposited != 0: (should never happen in practice) deposit has been partially completed
		match deposited_amount {
			_ if deposited_amount == amount => {
				frame_support::debug::trace!(
					target: "runtime",
					"Deposited {} to {:?}",
					amount,
					recipient,
				);

				Ok(())
			}
			_ if deposited_amount == 0 => {
				frame_support::debug::error!(
					target: "runtime",
					"Deposit of {} to {:?} has failed",
					amount,
					recipient,
				);

				Err(bp_currency_exchange::Error::DepositFailed)
			}
			_ => {
				frame_support::debug::error!(
					target: "runtime",
					"Deposit of {} to {:?} has partially competed. {} has been deposited",
					amount,
					recipient,
					deposited_amount,
				);

				// we can't return DepositFailed error here, because storage changes were made
				Err(bp_currency_exchange::Error::DepositPartiallyFailed)
			}
		}
	}
}

impl pallet_grandpa::Trait for Runtime {
	type Event = Event;
	type Call = Call;
	type KeyOwnerProofSystem = ();
	type KeyOwnerProof = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::Proof;
	type KeyOwnerIdentification =
		<Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::IdentificationTuple;
	type HandleEquivocation = ();
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Trait for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = MinimumPeriod;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 500;
	// For weight estimation, we assume that the most locks on an individual account will be 50.
	// This number may need to be adjusted in the future if this assumption no longer holds true.
	pub const MaxLocks: u32 = 50;
}

impl pallet_balances::Trait for Runtime {
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type Event = Event;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
	type MaxLocks = MaxLocks;
}

parameter_types! {
	pub const TransactionBaseFee: Balance = 0;
	pub const TransactionByteFee: Balance = 1;
}

impl pallet_transaction_payment::Trait for Runtime {
	type Currency = pallet_balances::Module<Runtime>;
	type OnTransactionPayment = ();
	type TransactionByteFee = TransactionByteFee;
	type WeightToFee = IdentityFee<Balance>;
	type FeeMultiplierUpdate = ();
}

impl pallet_sudo::Trait for Runtime {
	type Event = Event;
	type Call = Call;
}

parameter_types! {
	pub const Period: BlockNumber = 4;
	pub const Offset: BlockNumber = 0;
}

impl pallet_session::Trait for Runtime {
	type Event = Event;
	type ValidatorId = <Self as frame_system::Trait>::AccountId;
	type ValidatorIdOf = ();
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = pallet_shift_session_manager::Module<Runtime>;
	type SessionHandler = <SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type DisabledValidatorsThreshold = ();
	// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
	type WeightInfo = ();
}

impl pallet_substrate_bridge::Trait for Runtime {
	type BridgedChain = bp_millau::Millau;
}

impl pallet_shift_session_manager::Trait for Runtime {}

parameter_types! {
	pub const MaxMessagesToPruneAtOnce: bp_message_lane::MessageNonce = 8;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_message_lane::MessageNonce = 128;
}

impl pallet_message_lane::Trait for Runtime {
	type Event = Event;
	type MaxMessagesToPruneAtOnce = MaxMessagesToPruneAtOnce;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type OutboundPayload = crate::millau_messages::ToMillauMessagePayload;
	type OutboundMessageFee = Balance;

	type InboundPayload = crate::millau_messages::FromMillauMessagePayload;
	type InboundMessageFee = bp_millau::Balance;
	type InboundRelayer = bp_millau::AccountId;

	type TargetHeaderChain = crate::millau_messages::Millau;
	type LaneMessageVerifier = crate::millau_messages::ToMillauMessageVerifier;
	type MessageDeliveryAndDispatchPayment =
		pallet_message_lane::instant_payments::InstantCurrencyPayments<AccountId, pallet_balances::Module<Runtime>>;

	type SourceHeaderChain = crate::millau_messages::Millau;
	type MessageDispatch = crate::millau_messages::FromMillauMessageDispatch;
}

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		BridgeRialtoPoA: pallet_bridge_eth_poa::<Instance1>::{Module, Call, Config, Storage, ValidateUnsigned},
		BridgeKovan: pallet_bridge_eth_poa::<Instance2>::{Module, Call, Config, Storage, ValidateUnsigned},
		BridgeRialtoCurrencyExchange: pallet_bridge_currency_exchange::<Instance1>::{Module, Call},
		BridgeKovanCurrencyExchange: pallet_bridge_currency_exchange::<Instance2>::{Module, Call},
		BridgeMillau: pallet_substrate_bridge::{Module, Call, Storage, Config<T>},
		BridgeCallDispatch: pallet_bridge_call_dispatch::{Module, Event<T>},
		BridgeMillauMessageLane: pallet_message_lane::{Module, Call, Event<T>},
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		RandomnessCollectiveFlip: pallet_randomness_collective_flip::{Module, Call, Storage},
		Timestamp: pallet_timestamp::{Module, Call, Storage, Inherent},
		Aura: pallet_aura::{Module, Config<T>, Inherent},
		Grandpa: pallet_grandpa::{Module, Call, Storage, Config, Event},
		Balances: pallet_balances::{Module, Call, Storage, Config<T>, Event<T>},
		TransactionPayment: pallet_transaction_payment::{Module, Storage},
		Sudo: pallet_sudo::{Module, Call, Config<T>, Storage, Event<T>},
		Session: pallet_session::{Module, Call, Storage, Event, Config<T>},
		ShiftSessionManager: pallet_shift_session_manager::{Module},
	}
);

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
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Call, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive =
	frame_executive::Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, AllModules>;

impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block)
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			Runtime::metadata().into()
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

		fn random_seed() -> <Block as BlockT>::Hash {
			RandomnessCollectiveFlip::random_seed()
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
		fn account_nonce(account: AccountId) -> Index {
			System::account_nonce(account)
		}
	}

	impl bp_eth_poa::RialtoPoAHeaderApi<Block> for Runtime {
		fn best_block() -> (u64, bp_eth_poa::H256) {
			let best_block = BridgeRialtoPoA::best_block();
			(best_block.number, best_block.hash)
		}

		fn finalized_block() -> (u64, bp_eth_poa::H256) {
			let finalized_block = BridgeRialtoPoA::finalized_block();
			(finalized_block.number, finalized_block.hash)
		}

		fn is_import_requires_receipts(header: bp_eth_poa::AuraHeader) -> bool {
			BridgeRialtoPoA::is_import_requires_receipts(header)
		}

		fn is_known_block(hash: bp_eth_poa::H256) -> bool {
			BridgeRialtoPoA::is_known_block(hash)
		}
	}

	impl bp_eth_poa::KovanHeaderApi<Block> for Runtime {
		fn best_block() -> (u64, bp_eth_poa::H256) {
			let best_block = BridgeKovan::best_block();
			(best_block.number, best_block.hash)
		}

		fn finalized_block() -> (u64, bp_eth_poa::H256) {
			let finalized_block = BridgeKovan::finalized_block();
			(finalized_block.number, finalized_block.hash)
		}

		fn is_import_requires_receipts(header: bp_eth_poa::AuraHeader) -> bool {
			BridgeKovan::is_import_requires_receipts(header)
		}

		fn is_known_block(hash: bp_eth_poa::H256) -> bool {
			BridgeKovan::is_known_block(hash)
		}
	}

	impl bp_millau::MillauHeaderApi<Block> for Runtime {
		fn best_blocks() -> Vec<(bp_millau::BlockNumber, bp_millau::Hash)> {
			BridgeMillau::best_headers()
		}

		fn finalized_block() -> (bp_millau::BlockNumber, bp_millau::Hash) {
			let header = BridgeMillau::best_finalized();
			(header.number, header.hash())
		}

		fn incomplete_headers() -> Vec<(bp_millau::BlockNumber, bp_millau::Hash)> {
			BridgeMillau::require_justifications()
		}

		fn is_known_block(hash: bp_millau::Hash) -> bool {
			BridgeMillau::is_known_header(hash)
		}

		fn is_finalized_block(hash: bp_millau::Hash) -> bool {
			BridgeMillau::is_finalized_header(hash)
		}
	}

	impl bp_currency_exchange::RialtoCurrencyExchangeApi<Block, exchange::EthereumTransactionInclusionProof> for Runtime {
		fn filter_transaction_proof(proof: exchange::EthereumTransactionInclusionProof) -> bool {
			BridgeRialtoCurrencyExchange::filter_transaction_proof(&proof)
		}
	}

	impl bp_currency_exchange::KovanCurrencyExchangeApi<Block, exchange::EthereumTransactionInclusionProof> for Runtime {
		fn filter_transaction_proof(proof: exchange::EthereumTransactionInclusionProof) -> bool {
			BridgeKovanCurrencyExchange::filter_transaction_proof(&proof)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> u64 {
			Aura::slot_duration()
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities()
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

	// TODO: runtime should support several chains (https://github.com/paritytech/parity-bridges-common/issues/457)
	impl bp_message_lane::OutboundLaneApi<Block> for Runtime {
		fn messages_dispatch_weight(lane: bp_message_lane::LaneId, begin: bp_message_lane::MessageNonce, end: bp_message_lane::MessageNonce) -> Weight {
			(begin..=end)
				.filter_map(|nonce| BridgeMillauMessageLane::outbound_message_payload(lane, nonce))
				.filter_map(|encoded_payload| millau_messages::ToMillauMessagePayload::decode(&mut &encoded_payload[..]).ok())
				.map(|decoded_payload| decoded_payload.weight)
				.fold(0, |sum, weight| sum.saturating_add(weight))
		}

		fn latest_received_nonce(lane: bp_message_lane::LaneId) -> bp_message_lane::MessageNonce {
			BridgeMillauMessageLane::outbound_latest_received_nonce(lane)
		}

		fn latest_generated_nonce(lane: bp_message_lane::LaneId) -> bp_message_lane::MessageNonce {
			BridgeMillauMessageLane::outbound_latest_generated_nonce(lane)
		}
	}

	// TODO: runtime should support several chains (https://github.com/paritytech/parity-bridges-common/issues/457)
	impl bp_message_lane::InboundLaneApi<Block> for Runtime {
		fn latest_received_nonce(lane: bp_message_lane::LaneId) -> bp_message_lane::MessageNonce {
			BridgeMillauMessageLane::inbound_latest_received_nonce(lane)
		}

		fn latest_confirmed_nonce(lane: bp_message_lane::LaneId) -> bp_message_lane::MessageNonce {
			BridgeMillauMessageLane::inbound_latest_confirmed_nonce(lane)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
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

			use pallet_bridge_currency_exchange::benchmarking::{
				Module as BridgeCurrencyExchangeBench,
				Trait as BridgeCurrencyExchangeTrait,
				ProofParams as BridgeCurrencyExchangeProofParams,
			};

			impl BridgeCurrencyExchangeTrait<KovanCurrencyExchange> for Runtime {
				fn make_proof(
					proof_params: BridgeCurrencyExchangeProofParams<AccountId>,
				) -> crate::exchange::EthereumTransactionInclusionProof {
					use bp_currency_exchange::DepositInto;

					if proof_params.recipient_exists {
						<Runtime as pallet_bridge_currency_exchange::Trait<KovanCurrencyExchange>>::DepositInto::deposit_into(
							proof_params.recipient.clone(),
							ExistentialDeposit::get(),
						).unwrap();
					}

					let (transaction, receipt) = crate::exchange::prepare_ethereum_transaction(
						&proof_params.recipient,
						|tx| {
							// our runtime only supports transactions where data is exactly 32 bytes long
							// (receiver key)
							// => we are ignoring `transaction_size_factor` here
							tx.value = (ExistentialDeposit::get() * 10).into();
						},
					);
					let transactions = sp_std::iter::repeat((transaction, receipt))
						.take(1 + proof_params.proof_size_factor as usize)
						.collect::<Vec<_>>();
					let block_hash = crate::exchange::prepare_environment_for_claim::<Runtime, Kovan>(&transactions);
					crate::exchange::EthereumTransactionInclusionProof {
						block: block_hash,
						index: 0,
						proof: transactions,
					}
				}
			}

			add_benchmark!(params, batches, pallet_bridge_eth_poa, BridgeKovan);
			add_benchmark!(
				params,
				batches,
				pallet_bridge_currency_exchange,
				BridgeCurrencyExchangeBench::<Runtime, KovanCurrencyExchange>
			);

			if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
			Ok(batches)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_currency_exchange::DepositInto;

	fn run_deposit_into_test(test: impl Fn(AccountId) -> Balance) {
		let mut ext: sp_io::TestExternalities = SystemConfig::default().build_storage::<Runtime>().unwrap().into();
		ext.execute_with(|| {
			// initially issuance is zero
			assert_eq!(
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::total_issuance(),
				0,
			);

			// create account
			let account: AccountId = [1u8; 32].into();
			let initial_amount = ExistentialDeposit::get();
			let deposited =
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::deposit_creating(&account, initial_amount);
			drop(deposited);
			assert_eq!(
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::total_issuance(),
				initial_amount,
			);
			assert_eq!(
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::free_balance(&account),
				initial_amount,
			);

			// run test
			let total_issuance_change = test(account);

			// check that total issuance has changed by `run_deposit_into_test`
			assert_eq!(
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::total_issuance(),
				initial_amount + total_issuance_change,
			);
		});
	}

	#[test]
	fn deposit_into_existing_account_works() {
		run_deposit_into_test(|existing_account| {
			let initial_amount =
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::free_balance(&existing_account);
			let additional_amount = 10_000;
			<Runtime as pallet_bridge_currency_exchange::Trait<KovanCurrencyExchange>>::DepositInto::deposit_into(
				existing_account.clone(),
				additional_amount,
			)
			.unwrap();
			assert_eq!(
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::free_balance(&existing_account),
				initial_amount + additional_amount,
			);
			additional_amount
		});
	}

	#[test]
	fn deposit_into_new_account_works() {
		run_deposit_into_test(|_| {
			let initial_amount = 0;
			let additional_amount = ExistentialDeposit::get() + 10_000;
			let new_account: AccountId = [42u8; 32].into();
			<Runtime as pallet_bridge_currency_exchange::Trait<KovanCurrencyExchange>>::DepositInto::deposit_into(
				new_account.clone(),
				additional_amount,
			)
			.unwrap();
			assert_eq!(
				<pallet_balances::Module<Runtime> as Currency<AccountId>>::free_balance(&new_account),
				initial_amount + additional_amount,
			);
			additional_amount
		});
	}
}
