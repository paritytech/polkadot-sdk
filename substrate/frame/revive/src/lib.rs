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

#![doc = include_str!("../README.md")]
#![allow(rustdoc::private_intra_doc_links)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "runtime-benchmarks", recursion_limit = "1024")]

extern crate alloc;
mod address;
mod benchmarking;
mod benchmarking_dummy;
mod exec;
mod gas;
mod limits;
mod primitives;
mod storage;
mod transient_storage;
mod wasm;

#[cfg(test)]
mod tests;

pub mod chain_extension;
pub mod debug;
pub mod evm;
pub mod test_utils;
pub mod weights;

use crate::{
	evm::{runtime::GAS_PRICE, TransactionLegacyUnsigned},
	exec::{AccountIdOf, ExecError, Executable, Ext, Key, Origin, Stack as ExecStack},
	gas::GasMeter,
	storage::{meter::Meter as StorageMeter, ContractInfo, DeletionQueueManager},
	wasm::{CodeInfo, RuntimeCosts, WasmBlob},
};
use codec::{Codec, Decode, Encode};
use environmental::*;
use frame_support::{
	dispatch::{
		DispatchErrorWithPostInfo, DispatchResultWithPostInfo, GetDispatchInfo, Pays,
		PostDispatchInfo, RawOrigin,
	},
	ensure,
	pallet_prelude::DispatchClass,
	traits::{
		fungible::{Inspect, Mutate, MutateHold},
		ConstU32, ConstU64, Contains, EnsureOrigin, Get, IsType, OriginTrait, Time,
	},
	weights::{Weight, WeightMeter},
	BoundedVec, RuntimeDebugNoBound,
};
use frame_system::{
	ensure_signed,
	pallet_prelude::{BlockNumberFor, OriginFor},
	EventRecord, Pallet as System,
};
use pallet_transaction_payment::OnChargeTransaction;
use scale_info::TypeInfo;
use sp_core::{H160, H256, U256};
use sp_runtime::{
	traits::{BadOrigin, Convert, Dispatchable, Saturating},
	DispatchError,
};

pub use crate::{
	address::{create1, create2, AddressMapper, DefaultAddressMapper},
	debug::Tracing,
	exec::MomentOf,
	pallet::*,
};
pub use primitives::*;
pub use weights::WeightInfo;

#[cfg(doc)]
pub use crate::wasm::SyscallDoc;

type TrieId = BoundedVec<u8, ConstU32<128>>;
type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
type OnChargeTransactionBalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<T>>::Balance;
type CodeVec = BoundedVec<u8, ConstU32<{ limits::code::BLOB_BYTES }>>;
type EventRecordOf<T> =
	EventRecord<<T as frame_system::Config>::RuntimeEvent, <T as frame_system::Config>::Hash>;
type DebugBuffer = BoundedVec<u8, ConstU32<{ limits::DEBUG_BUFFER_BYTES }>>;
type ImmutableData = BoundedVec<u8, ConstU32<{ limits::IMMUTABLE_BYTES }>>;

/// Used as a sentinel value when reading and writing contract memory.
///
/// It is usually used to signal `None` to a contract when only a primitive is allowed
/// and we don't want to go through encoding a full Rust type. Using `u32::Max` is a safe
/// sentinel because contracts are never allowed to use such a large amount of resources
/// that this value makes sense for a memory location or length.
const SENTINEL: u32 = u32::MAX;

/// The target that is used for the log output emitted by this crate.
///
/// Hence you can use this target to selectively increase the log level for this crate.
///
/// Example: `RUST_LOG=runtime::revive=debug my_code --dev`
const LOG_TARGET: &str = "runtime::revive";

/// This version determines which syscalls are available to contracts.
///
/// Needs to be bumped every time a versioned syscall is added.
const API_VERSION: u16 = 0;

#[test]
fn api_version_up_to_date() {
	assert!(
		API_VERSION == crate::wasm::HIGHEST_API_VERSION,
		"A new versioned API has been added. The `API_VERSION` needs to be bumped."
	);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::debug::Debugger;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_core::U256;
	use sp_runtime::Perbill;

	/// The in-code storage version.
	pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// The time implementation used to supply timestamps to contracts through `seal_now`.
		type Time: Time;

		/// The fungible in which fees are paid and contract balances are held.
		#[pallet::no_default]
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// The overarching event type.
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		#[pallet::no_default_bounds]
		type RuntimeCall: Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ codec::Decode
			+ core::fmt::Debug
			+ IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// Overarching hold reason.
		#[pallet::no_default_bounds]
		type RuntimeHoldReason: From<HoldReason>;

		/// Filter that is applied to calls dispatched by contracts.
		///
		/// Use this filter to control which dispatchables are callable by contracts.
		/// This is applied in **addition** to [`frame_system::Config::BaseCallFilter`].
		/// It is recommended to treat this as a whitelist.
		///
		/// # Stability
		///
		/// The runtime **must** make sure that all dispatchables that are callable by
		/// contracts remain stable. In addition [`Self::RuntimeCall`] itself must remain stable.
		/// This means that no existing variants are allowed to switch their positions.
		///
		/// # Note
		///
		/// Note that dispatchables that are called via contracts do not spawn their
		/// own wasm instance for each call (as opposed to when called via a transaction).
		/// Therefore please make sure to be restrictive about which dispatchables are allowed
		/// in order to not introduce a new DoS vector like memory allocation patterns that can
		/// be exploited to drive the runtime into a panic.
		///
		/// This filter does not apply to XCM transact calls. To impose restrictions on XCM transact
		/// calls, you must configure them separately within the XCM pallet itself.
		#[pallet::no_default_bounds]
		type CallFilter: Contains<<Self as frame_system::Config>::RuntimeCall>;

		/// Used to answer contracts' queries regarding the current weight price. This is **not**
		/// used to calculate the actual fee and is only for informational purposes.
		#[pallet::no_default_bounds]
		type WeightPrice: Convert<Weight, BalanceOf<Self>>;

		/// Describes the weights of the dispatchables of this module and is also used to
		/// construct a default cost schedule.
		type WeightInfo: WeightInfo;

		/// Type that allows the runtime authors to add new host functions for a contract to call.
		#[pallet::no_default_bounds]
		type ChainExtension: chain_extension::ChainExtension<Self> + Default;

		/// The amount of balance a caller has to pay for each byte of storage.
		///
		/// # Note
		///
		/// It is safe to change this value on a live chain as all refunds are pro rata.
		#[pallet::constant]
		#[pallet::no_default_bounds]
		type DepositPerByte: Get<BalanceOf<Self>>;

		/// The amount of balance a caller has to pay for each storage item.
		///
		/// # Note
		///
		/// It is safe to change this value on a live chain as all refunds are pro rata.
		#[pallet::constant]
		#[pallet::no_default_bounds]
		type DepositPerItem: Get<BalanceOf<Self>>;

		/// The percentage of the storage deposit that should be held for using a code hash.
		/// Instantiating a contract, or calling [`chain_extension::Ext::lock_delegate_dependency`]
		/// protects the code from being removed. In order to prevent abuse these actions are
		/// protected with a percentage of the code deposit.
		#[pallet::constant]
		type CodeHashLockupDepositPercent: Get<Perbill>;

		/// Only valid type is [`DefaultAddressMapper`].
		#[pallet::no_default_bounds]
		type AddressMapper: AddressMapper<AccountIdOf<Self>>;

		/// Make contract callable functions marked as `#[unstable]` available.
		///
		/// Contracts that use `#[unstable]` functions won't be able to be uploaded unless
		/// this is set to `true`. This is only meant for testnets and dev nodes in order to
		/// experiment with new features.
		///
		/// # Warning
		///
		/// Do **not** set to `true` on productions chains.
		#[pallet::constant]
		type UnsafeUnstableInterface: Get<bool>;

		/// Origin allowed to upload code.
		///
		/// By default, it is safe to set this to `EnsureSigned`, allowing anyone to upload contract
		/// code.
		#[pallet::no_default_bounds]
		type UploadOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

		/// Origin allowed to instantiate code.
		///
		/// # Note
		///
		/// This is not enforced when a contract instantiates another contract. The
		/// [`Self::UploadOrigin`] should make sure that no code is deployed that does unwanted
		/// instantiations.
		///
		/// By default, it is safe to set this to `EnsureSigned`, allowing anyone to instantiate
		/// contract code.
		#[pallet::no_default_bounds]
		type InstantiateOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

		/// For most production chains, it's recommended to use the `()` implementation of this
		/// trait. This implementation offers additional logging when the log target
		/// "runtime::revive" is set to trace.
		#[pallet::no_default_bounds]
		type Debug: Debugger<Self>;

		/// A type that exposes XCM APIs, allowing contracts to interact with other parachains, and
		/// execute XCM programs.
		#[pallet::no_default_bounds]
		type Xcm: xcm_builder::Controller<
			OriginFor<Self>,
			<Self as frame_system::Config>::RuntimeCall,
			BlockNumberFor<Self>,
		>;

		/// The amount of memory in bytes that parachain nodes a lot to the runtime.
		///
		/// This is used in [`Pallet::integrity_test`] to make sure that the runtime has enough
		/// memory to support this pallet if set to the correct value.
		type RuntimeMemory: Get<u32>;

		/// The amount of memory in bytes that relay chain validators a lot to the PoV.
		///
		/// This is used in [`Pallet::integrity_test`] to make sure that the runtime has enough
		/// memory to support this pallet if set to the correct value.
		///
		/// This value is usually higher than [`Self::RuntimeMemory`] to account for the fact
		/// that validators have to hold all storage items in PvF memory.
		type PVFMemory: Get<u32>;

		/// The [EIP-155](https://eips.ethereum.org/EIPS/eip-155) chain ID.
		///
		/// This is a unique identifier assigned to each blockchain network,
		/// preventing replay attacks.
		#[pallet::constant]
		type ChainId: Get<u64>;
	}

	/// Container for different types that implement [`DefaultConfig`]` of this pallet.
	pub mod config_preludes {
		use super::*;
		use frame_support::{
			derive_impl,
			traits::{ConstBool, ConstU32},
		};
		use frame_system::EnsureSigned;
		use sp_core::parameter_types;

		type AccountId = sp_runtime::AccountId32;
		type Balance = u64;
		const UNITS: Balance = 10_000_000_000;
		const CENTS: Balance = UNITS / 100;

		pub const fn deposit(items: u32, bytes: u32) -> Balance {
			items as Balance * 1 * CENTS + (bytes as Balance) * 1 * CENTS
		}

		parameter_types! {
			pub const DepositPerItem: Balance = deposit(1, 0);
			pub const DepositPerByte: Balance = deposit(0, 1);
			pub const CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(0);
		}

		/// A type providing default configurations for this pallet in testing environment.
		pub struct TestDefaultConfig;

		impl Time for TestDefaultConfig {
			type Moment = u64;
			fn now() -> Self::Moment {
				unimplemented!("No default `now` implementation in `TestDefaultConfig` provide a custom `T::Time` type.")
			}
		}

		impl<T: From<u64>> Convert<Weight, T> for TestDefaultConfig {
			fn convert(w: Weight) -> T {
				w.ref_time().into()
			}
		}

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig, no_aggregated_types)]
		impl frame_system::DefaultConfig for TestDefaultConfig {}

		#[frame_support::register_default_impl(TestDefaultConfig)]
		impl DefaultConfig for TestDefaultConfig {
			#[inject_runtime_type]
			type RuntimeEvent = ();

			#[inject_runtime_type]
			type RuntimeHoldReason = ();

			#[inject_runtime_type]
			type RuntimeCall = ();
			type AddressMapper = DefaultAddressMapper;
			type CallFilter = ();
			type ChainExtension = ();
			type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
			type DepositPerByte = DepositPerByte;
			type DepositPerItem = DepositPerItem;
			type Time = Self;
			type UnsafeUnstableInterface = ConstBool<true>;
			type UploadOrigin = EnsureSigned<AccountId>;
			type InstantiateOrigin = EnsureSigned<AccountId>;
			type WeightInfo = ();
			type WeightPrice = Self;
			type Debug = ();
			type Xcm = ();
			type RuntimeMemory = ConstU32<{ 128 * 1024 * 1024 }>;
			type PVFMemory = ConstU32<{ 512 * 1024 * 1024 }>;
			type ChainId = ConstU64<{ 0 }>;
		}
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		/// Contract deployed by address at the specified address.
		Instantiated { deployer: H160, contract: H160 },

		/// Contract has been removed.
		///
		/// # Note
		///
		/// The only way for a contract to be removed and emitting this event is by calling
		/// `seal_terminate`.
		Terminated {
			/// The contract that was terminated.
			contract: H160,
			/// The account that received the contracts remaining balance
			beneficiary: H160,
		},

		/// Code with the specified hash has been stored.
		CodeStored { code_hash: H256, deposit_held: BalanceOf<T>, uploader: H160 },

		/// A custom event emitted by the contract.
		ContractEmitted {
			/// The contract that emitted the event.
			contract: H160,
			/// Data supplied by the contract. Metadata generated during contract compilation
			/// is needed to decode it.
			data: Vec<u8>,
			/// A list of topics used to index the event.
			/// Number of topics is capped by [`limits::NUM_EVENT_TOPICS`].
			topics: Vec<H256>,
		},

		/// A code with the specified hash was removed.
		CodeRemoved { code_hash: H256, deposit_released: BalanceOf<T>, remover: H160 },

		/// A contract's code was updated.
		ContractCodeUpdated {
			/// The contract that has been updated.
			contract: H160,
			/// New code hash that was set for the contract.
			new_code_hash: H256,
			/// Previous code hash of the contract.
			old_code_hash: H256,
		},

		/// A contract was called either by a plain account or another contract.
		///
		/// # Note
		///
		/// Please keep in mind that like all events this is only emitted for successful
		/// calls. This is because on failure all storage changes including events are
		/// rolled back.
		Called {
			/// The caller of the `contract`.
			caller: Origin<T>,
			/// The contract that was called.
			contract: H160,
		},

		/// A contract delegate called a code hash.
		///
		/// # Note
		///
		/// Please keep in mind that like all events this is only emitted for successful
		/// calls. This is because on failure all storage changes including events are
		/// rolled back.
		DelegateCalled {
			/// The contract that performed the delegate call and hence in whose context
			/// the `code_hash` is executed.
			contract: H160,
			/// The code hash that was delegate called.
			code_hash: H256,
		},

		/// Some funds have been transferred and held as storage deposit.
		StorageDepositTransferredAndHeld { from: H160, to: H160, amount: BalanceOf<T> },

		/// Some storage deposit funds have been transferred and released.
		StorageDepositTransferredAndReleased { from: H160, to: H160, amount: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid schedule supplied, e.g. with zero weight of a basic operation.
		InvalidSchedule,
		/// Invalid combination of flags supplied to `seal_call` or `seal_delegate_call`.
		InvalidCallFlags,
		/// The executed contract exhausted its gas limit.
		OutOfGas,
		/// Performing the requested transfer failed. Probably because there isn't enough
		/// free balance in the sender's account.
		TransferFailed,
		/// Performing a call was denied because the calling depth reached the limit
		/// of what is specified in the schedule.
		MaxCallDepthReached,
		/// No contract was found at the specified address.
		ContractNotFound,
		/// No code could be found at the supplied code hash.
		CodeNotFound,
		/// No code info could be found at the supplied code hash.
		CodeInfoNotFound,
		/// A buffer outside of sandbox memory was passed to a contract API function.
		OutOfBounds,
		/// Input passed to a contract API function failed to decode as expected type.
		DecodingFailed,
		/// Contract trapped during execution.
		ContractTrapped,
		/// The size defined in `T::MaxValueSize` was exceeded.
		ValueTooLarge,
		/// Termination of a contract is not allowed while the contract is already
		/// on the call stack. Can be triggered by `seal_terminate`.
		TerminatedWhileReentrant,
		/// `seal_call` forwarded this contracts input. It therefore is no longer available.
		InputForwarded,
		/// The amount of topics passed to `seal_deposit_events` exceeds the limit.
		TooManyTopics,
		/// The chain does not provide a chain extension. Calling the chain extension results
		/// in this error. Note that this usually  shouldn't happen as deploying such contracts
		/// is rejected.
		NoChainExtension,
		/// Failed to decode the XCM program.
		XCMDecodeFailed,
		/// A contract with the same AccountId already exists.
		DuplicateContract,
		/// A contract self destructed in its constructor.
		///
		/// This can be triggered by a call to `seal_terminate`.
		TerminatedInConstructor,
		/// A call tried to invoke a contract that is flagged as non-reentrant.
		ReentranceDenied,
		/// A contract called into the runtime which then called back into this pallet.
		ReenteredPallet,
		/// A contract attempted to invoke a state modifying API while being in read-only mode.
		StateChangeDenied,
		/// Origin doesn't have enough balance to pay the required storage deposits.
		StorageDepositNotEnoughFunds,
		/// More storage was created than allowed by the storage deposit limit.
		StorageDepositLimitExhausted,
		/// Code removal was denied because the code is still in use by at least one contract.
		CodeInUse,
		/// The contract ran to completion but decided to revert its storage changes.
		/// Please note that this error is only returned from extrinsics. When called directly
		/// or via RPC an `Ok` will be returned. In this case the caller needs to inspect the flags
		/// to determine whether a reversion has taken place.
		ContractReverted,
		/// The contract failed to compile or is missing the correct entry points.
		///
		/// A more detailed error can be found on the node console if debug messages are enabled
		/// by supplying `-lruntime::revive=debug`.
		CodeRejected,
		/// The code blob supplied is larger than [`limits::code::BLOB_BYTES`].
		BlobTooLarge,
		/// The static memory consumption of the blob will be larger than
		/// [`limits::code::STATIC_MEMORY_BYTES`].
		StaticMemoryTooLarge,
		/// The program contains a basic block that is larger than allowed.
		BasicBlockTooLarge,
		/// The program contains an invalid instruction.
		InvalidInstruction,
		/// The contract has reached its maximum number of delegate dependencies.
		MaxDelegateDependenciesReached,
		/// The dependency was not found in the contract's delegate dependencies.
		DelegateDependencyNotFound,
		/// The contract already depends on the given delegate dependency.
		DelegateDependencyAlreadyExists,
		/// Can not add a delegate dependency to the code hash of the contract itself.
		CannotAddSelfAsDelegateDependency,
		/// Can not add more data to transient storage.
		OutOfTransientStorage,
		/// The contract tried to call a syscall which does not exist (at its current api level).
		InvalidSyscall,
		/// Invalid storage flags were passed to one of the storage syscalls.
		InvalidStorageFlags,
		/// PolkaVM failed during code execution. Probably due to a malformed program.
		ExecutionFailed,
		/// Failed to convert a U256 to a Balance.
		BalanceConversionFailed,
		/// Immutable data can only be set during deploys and only be read during calls.
		/// Additionally, it is only valid to set the data once and it must not be empty.
		InvalidImmutableAccess,
	}

	/// A reason for the pallet contracts placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The Pallet has reserved it for storing code on-chain.
		CodeUploadDepositReserve,
		/// The Pallet has reserved it for storage deposit.
		StorageDepositReserve,
	}

	/// A mapping from a contract's code hash to its code.
	#[pallet::storage]
	pub(crate) type PristineCode<T: Config> = StorageMap<_, Identity, H256, CodeVec>;

	/// A mapping from a contract's code hash to its code info.
	#[pallet::storage]
	pub(crate) type CodeInfoOf<T: Config> = StorageMap<_, Identity, H256, CodeInfo<T>>;

	/// The code associated with a given account.
	#[pallet::storage]
	pub(crate) type ContractInfoOf<T: Config> = StorageMap<_, Identity, H160, ContractInfo<T>>;

	/// The immutable data associated with a given account.
	#[pallet::storage]
	pub(crate) type ImmutableDataOf<T: Config> = StorageMap<_, Identity, H160, ImmutableData>;

	/// Evicted contracts that await child trie deletion.
	///
	/// Child trie deletion is a heavy operation depending on the amount of storage items
	/// stored in said trie. Therefore this operation is performed lazily in `on_idle`.
	#[pallet::storage]
	pub(crate) type DeletionQueue<T: Config> = StorageMap<_, Twox64Concat, u32, TrieId>;

	/// A pair of monotonic counters used to track the latest contract marked for deletion
	/// and the latest deleted contract in queue.
	#[pallet::storage]
	pub(crate) type DeletionQueueCounter<T: Config> =
		StorageValue<_, DeletionQueueManager<T>, ValueQuery>;

	#[pallet::extra_constants]
	impl<T: Config> Pallet<T> {
		#[pallet::constant_name(ApiVersion)]
		fn api_version() -> u16 {
			API_VERSION
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_block: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(limit);
			ContractInfo::<T>::process_deletion_queue_batch(&mut meter);
			meter.consumed()
		}

		fn integrity_test() {
			use limits::code::STATIC_MEMORY_BYTES;

			// The memory available in the block building runtime
			let max_runtime_mem: u32 = T::RuntimeMemory::get();
			// The root frame is not accounted in CALL_STACK_DEPTH
			let max_call_depth =
				limits::CALL_STACK_DEPTH.checked_add(1).expect("CallStack size is too big");
			// Transient storage uses a BTreeMap, which has overhead compared to the raw size of
			// key-value data. To ensure safety, a margin of 2x the raw key-value size is used.
			let max_transient_storage_size = limits::TRANSIENT_STORAGE_BYTES
				.checked_mul(2)
				.expect("MaxTransientStorageSize is too large");

			// We only allow 50% of the runtime memory to be utilized by the contracts call
			// stack, keeping the rest for other facilities, such as PoV, etc.
			const TOTAL_MEMORY_DEVIDER: u32 = 2;

			// The inefficiencies of the freeing-bump allocator
			// being used in the client for the runtime memory allocations, could lead to possible
			// memory allocations grow up to `x4` times in some extreme cases.
			const MEMORY_ALLOCATOR_INEFFICENCY_DEVIDER: u32 = 4;

			// Check that the configured `STATIC_MEMORY_BYTES` fits into runtime memory.
			//
			// `STATIC_MEMORY_BYTES` is the amount of memory that a contract can consume
			// in memory and is enforced at upload time.
			//
			// Dynamic allocations are not available, yet. Hence are not taken into consideration
			// here.
			let static_memory_limit = max_runtime_mem
				.saturating_div(TOTAL_MEMORY_DEVIDER)
				.saturating_sub(max_transient_storage_size)
				.saturating_div(max_call_depth)
				.saturating_sub(STATIC_MEMORY_BYTES)
				.saturating_div(MEMORY_ALLOCATOR_INEFFICENCY_DEVIDER);

			assert!(
				STATIC_MEMORY_BYTES < static_memory_limit,
				"Given `CallStack` height {:?}, `STATIC_MEMORY_LIMIT` should be set less than {:?} \
				 (current value is {:?}), to avoid possible runtime oom issues.",
				max_call_depth,
				static_memory_limit,
				STATIC_MEMORY_BYTES,
			);

			// Validators are configured to be able to use more memory than block builders. This is
			// because in addition to `max_runtime_mem` they need to hold additional data in
			// memory: PoV in multiple copies (1x encoded + 2x decoded) and all storage which
			// includes emitted events. The assumption is that storage/events size
			// can be a maximum of half of the validator runtime memory - max_runtime_mem.
			let max_block_ref_time = T::BlockWeights::get()
				.get(DispatchClass::Normal)
				.max_total
				.unwrap_or_else(|| T::BlockWeights::get().max_block)
				.ref_time();
			let max_payload_size = limits::PAYLOAD_BYTES;
			let max_key_size =
				Key::try_from_var(alloc::vec![0u8; limits::STORAGE_KEY_BYTES as usize])
					.expect("Key of maximal size shall be created")
					.hash()
					.len() as u32;

			let max_immutable_key_size = T::AccountId::max_encoded_len() as u32;
			let max_immutable_size: u32 = ((max_block_ref_time /
				(<RuntimeCosts as gas::Token<T>>::weight(&RuntimeCosts::SetImmutableData(
					limits::IMMUTABLE_BYTES,
				))
				.ref_time()))
			.saturating_mul(limits::IMMUTABLE_BYTES.saturating_add(max_immutable_key_size) as u64))
			.try_into()
			.expect("Immutable data size too big");

			// We can use storage to store items using the available block ref_time with the
			// `set_storage` host function.
			let max_storage_size: u32 = ((max_block_ref_time /
				(<RuntimeCosts as gas::Token<T>>::weight(&RuntimeCosts::SetStorage {
					new_bytes: max_payload_size,
					old_bytes: 0,
				})
				.ref_time()))
			.saturating_mul(max_payload_size.saturating_add(max_key_size) as u64))
			.saturating_add(max_immutable_size.into())
			.try_into()
			.expect("Storage size too big");

			let max_pvf_mem: u32 = T::PVFMemory::get();
			let storage_size_limit = max_pvf_mem.saturating_sub(max_runtime_mem) / 2;

			assert!(
				max_storage_size < storage_size_limit,
				"Maximal storage size {} exceeds the storage limit {}",
				max_storage_size,
				storage_size_limit
			);

			// We can use storage to store events using the available block ref_time with the
			// `deposit_event` host function. The overhead of stored events, which is around 100B,
			// is not taken into account to simplify calculations, as it does not change much.
			let max_events_size: u32 = ((max_block_ref_time /
				(<RuntimeCosts as gas::Token<T>>::weight(&RuntimeCosts::DepositEvent {
					num_topic: 0,
					len: max_payload_size,
				})
				.ref_time()))
			.saturating_mul(max_payload_size as u64))
			.try_into()
			.expect("Events size too big");

			assert!(
				max_events_size < storage_size_limit,
				"Maximal events size {} exceeds the events limit {}",
				max_events_size,
				storage_size_limit
			);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		BalanceOf<T>: Into<U256> + TryFrom<U256>,
		MomentOf<T>: Into<U256>,
	{
		/// A raw EVM transaction, typically dispatched by an Ethereum JSON-RPC server.
		///
		/// # Parameters
		///
		/// * `payload`: The RLP-encoded [`crate::evm::TransactionLegacySigned`].
		/// * `gas_limit`: The gas limit enforced during contract execution.
		/// * `storage_deposit_limit`: The maximum balance that can be charged to the caller for
		///   storage usage.
		///
		/// # Note
		///
		/// This call cannot be dispatched directly; attempting to do so will result in a failed
		/// transaction. It serves as a wrapper for an Ethereum transaction. When submitted, the
		/// runtime converts it into a [`sp_runtime::generic::CheckedExtrinsic`] by recovering the
		/// signer and validating the transaction.
		#[allow(unused_variables)]
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::MAX)]
		pub fn eth_transact(
			origin: OriginFor<T>,
			payload: Vec<u8>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			Err(frame_system::Error::CallFiltered::<T>.into())
		}

		/// Makes a call to an account, optionally transferring some balance.
		///
		/// # Parameters
		///
		/// * `dest`: Address of the contract to call.
		/// * `value`: The balance to transfer from the `origin` to `dest`.
		/// * `gas_limit`: The gas limit enforced when executing the constructor.
		/// * `storage_deposit_limit`: The maximum amount of balance that can be charged from the
		///   caller to pay for the storage consumed.
		/// * `data`: The input data to pass to the contract.
		///
		/// * If the account is a smart-contract account, the associated code will be
		/// executed and any value will be transferred.
		/// * If the account is a regular account, any value will be transferred.
		/// * If no account exists and the call value is not less than `existential_deposit`,
		/// a regular account will be created and any value will be transferred.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::call().saturating_add(*gas_limit))]
		pub fn call(
			origin: OriginFor<T>,
			dest: H160,
			#[pallet::compact] value: BalanceOf<T>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			data: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			log::info!(target: LOG_TARGET, "Call: {:?} {:?} {:?}", dest, value, data);
			let mut output = Self::bare_call(
				origin,
				dest,
				value,
				gas_limit,
				storage_deposit_limit,
				data,
				DebugInfo::Skip,
				CollectEvents::Skip,
			);
			if let Ok(return_value) = &output.result {
				if return_value.did_revert() {
					output.result = Err(<Error<T>>::ContractReverted.into());
				}
			}
			dispatch_result(output.result, output.gas_consumed, T::WeightInfo::call())
		}

		/// Instantiates a contract from a previously deployed wasm binary.
		///
		/// This function is identical to [`Self::instantiate_with_code`] but without the
		/// code deployment step. Instead, the `code_hash` of an on-chain deployed wasm binary
		/// must be supplied.
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::WeightInfo::instantiate(data.len() as u32).saturating_add(*gas_limit)
		)]
		pub fn instantiate(
			origin: OriginFor<T>,
			#[pallet::compact] value: BalanceOf<T>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			code_hash: sp_core::H256,
			data: Vec<u8>,
			salt: Option<[u8; 32]>,
		) -> DispatchResultWithPostInfo {
			let data_len = data.len() as u32;
			let mut output = Self::bare_instantiate(
				origin,
				value,
				gas_limit,
				storage_deposit_limit,
				Code::Existing(code_hash),
				data,
				salt,
				DebugInfo::Skip,
				CollectEvents::Skip,
			);
			if let Ok(retval) = &output.result {
				if retval.result.did_revert() {
					output.result = Err(<Error<T>>::ContractReverted.into());
				}
			}
			dispatch_result(
				output.result.map(|result| result.result),
				output.gas_consumed,
				T::WeightInfo::instantiate(data_len),
			)
		}

		/// Instantiates a new contract from the supplied `code` optionally transferring
		/// some balance.
		///
		/// This dispatchable has the same effect as calling [`Self::upload_code`] +
		/// [`Self::instantiate`]. Bundling them together provides efficiency gains. Please
		/// also check the documentation of [`Self::upload_code`].
		///
		/// # Parameters
		///
		/// * `value`: The balance to transfer from the `origin` to the newly created contract.
		/// * `gas_limit`: The gas limit enforced when executing the constructor.
		/// * `storage_deposit_limit`: The maximum amount of balance that can be charged/reserved
		///   from the caller to pay for the storage consumed.
		/// * `code`: The contract code to deploy in raw bytes.
		/// * `data`: The input data to pass to the contract constructor.
		/// * `salt`: Used for the address derivation. If `Some` is supplied then `CREATE2`
		/// 	semantics are used. If `None` then `CRATE1` is used.
		///
		///
		/// Instantiation is executed as follows:
		///
		/// - The supplied `code` is deployed, and a `code_hash` is created for that code.
		/// - If the `code_hash` already exists on the chain the underlying `code` will be shared.
		/// - The destination address is computed based on the sender, code_hash and the salt.
		/// - The smart-contract account is created at the computed address.
		/// - The `value` is transferred to the new account.
		/// - The `deploy` function is executed in the context of the newly-created account.
		#[pallet::call_index(3)]
		#[pallet::weight(
			T::WeightInfo::instantiate_with_code(code.len() as u32, data.len() as u32)
			.saturating_add(*gas_limit)
		)]
		pub fn instantiate_with_code(
			origin: OriginFor<T>,
			#[pallet::compact] value: BalanceOf<T>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			code: Vec<u8>,
			data: Vec<u8>,
			salt: Option<[u8; 32]>,
		) -> DispatchResultWithPostInfo {
			let code_len = code.len() as u32;
			let data_len = data.len() as u32;
			let mut output = Self::bare_instantiate(
				origin,
				value,
				gas_limit,
				storage_deposit_limit,
				Code::Upload(code),
				data,
				salt,
				DebugInfo::Skip,
				CollectEvents::Skip,
			);
			if let Ok(retval) = &output.result {
				if retval.result.did_revert() {
					output.result = Err(<Error<T>>::ContractReverted.into());
				}
			}
			dispatch_result(
				output.result.map(|result| result.result),
				output.gas_consumed,
				T::WeightInfo::instantiate_with_code(code_len, data_len),
			)
		}

		/// Upload new `code` without instantiating a contract from it.
		///
		/// If the code does not already exist a deposit is reserved from the caller
		/// and unreserved only when [`Self::remove_code`] is called. The size of the reserve
		/// depends on the size of the supplied `code`.
		///
		/// # Note
		///
		/// Anyone can instantiate a contract from any uploaded code and thus prevent its removal.
		/// To avoid this situation a constructor could employ access control so that it can
		/// only be instantiated by permissioned entities. The same is true when uploading
		/// through [`Self::instantiate_with_code`].
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::upload_code(code.len() as u32))]
		pub fn upload_code(
			origin: OriginFor<T>,
			code: Vec<u8>,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
		) -> DispatchResult {
			Self::bare_upload_code(origin, code, storage_deposit_limit).map(|_| ())
		}

		/// Remove the code stored under `code_hash` and refund the deposit to its owner.
		///
		/// A code can only be removed by its original uploader (its owner) and only if it is
		/// not used by any contract.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::remove_code())]
		pub fn remove_code(
			origin: OriginFor<T>,
			code_hash: sp_core::H256,
		) -> DispatchResultWithPostInfo {
			let origin = ensure_signed(origin)?;
			<WasmBlob<T>>::remove(&origin, code_hash)?;
			// we waive the fee because removing unused code is beneficial
			Ok(Pays::No.into())
		}

		/// Privileged function that changes the code of an existing contract.
		///
		/// This takes care of updating refcounts and all other necessary operations. Returns
		/// an error if either the `code_hash` or `dest` do not exist.
		///
		/// # Note
		///
		/// This does **not** change the address of the contract in question. This means
		/// that the contract address is no longer derived from its code hash after calling
		/// this dispatchable.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_code())]
		pub fn set_code(
			origin: OriginFor<T>,
			dest: H160,
			code_hash: sp_core::H256,
		) -> DispatchResult {
			ensure_root(origin)?;
			<ContractInfoOf<T>>::try_mutate(&dest, |contract| {
				let contract = if let Some(contract) = contract {
					contract
				} else {
					return Err(<Error<T>>::ContractNotFound.into());
				};
				<ExecStack<T, WasmBlob<T>>>::increment_refcount(code_hash)?;
				<ExecStack<T, WasmBlob<T>>>::decrement_refcount(contract.code_hash);
				Self::deposit_event(Event::ContractCodeUpdated {
					contract: dest,
					new_code_hash: code_hash,
					old_code_hash: contract.code_hash,
				});
				contract.code_hash = code_hash;
				Ok(())
			})
		}
	}
}

/// Create a dispatch result reflecting the amount of consumed gas.
fn dispatch_result<R>(
	result: Result<R, DispatchError>,
	gas_consumed: Weight,
	base_weight: Weight,
) -> DispatchResultWithPostInfo {
	let post_info = PostDispatchInfo {
		actual_weight: Some(gas_consumed.saturating_add(base_weight)),
		pays_fee: Default::default(),
	};

	result
		.map(|_| post_info)
		.map_err(|e| DispatchErrorWithPostInfo { post_info, error: e })
}

impl<T: Config> Pallet<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
{
	/// A generalized version of [`Self::call`].
	///
	/// Identical to [`Self::call`] but tailored towards being called by other code within the
	/// runtime as opposed to from an extrinsic. It returns more information and allows the
	/// enablement of features that are not suitable for an extrinsic (debugging, event
	/// collection).
	pub fn bare_call(
		origin: OriginFor<T>,
		dest: H160,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		data: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractResult<ExecReturnValue, BalanceOf<T>, EventRecordOf<T>> {
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();
		let mut debug_message = if matches!(debug, DebugInfo::UnsafeDebug) {
			Some(DebugBuffer::default())
		} else {
			None
		};
		let try_call = || {
			let origin = Origin::from_runtime_origin(origin)?;
			let mut storage_meter = StorageMeter::new(&origin, storage_deposit_limit, value)?;
			let result = ExecStack::<T, WasmBlob<T>>::run_call(
				origin.clone(),
				dest,
				&mut gas_meter,
				&mut storage_meter,
				value,
				data,
				debug_message.as_mut(),
			)?;
			storage_deposit = storage_meter.try_into_deposit(&origin)?;
			Ok(result)
		};
		let result = Self::run_guarded(try_call);
		let events = if matches!(collect_events, CollectEvents::UnsafeCollect) {
			Some(System::<T>::read_events_no_consensus().map(|e| *e).collect())
		} else {
			None
		};
		ContractResult {
			result: result.map_err(|r| r.error),
			gas_consumed: gas_meter.gas_consumed(),
			gas_required: gas_meter.gas_required(),
			storage_deposit,
			debug_message: debug_message.unwrap_or_default().to_vec(),
			events,
		}
	}

	/// A generalized version of [`Self::instantiate`] or [`Self::instantiate_with_code`].
	///
	/// Identical to [`Self::instantiate`] or [`Self::instantiate_with_code`] but tailored towards
	/// being called by other code within the runtime as opposed to from an extrinsic. It returns
	/// more information and allows the enablement of features that are not suitable for an
	/// extrinsic (debugging, event collection).
	pub fn bare_instantiate(
		origin: OriginFor<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		mut storage_deposit_limit: BalanceOf<T>,
		code: Code,
		data: Vec<u8>,
		salt: Option<[u8; 32]>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractResult<InstantiateReturnValue, BalanceOf<T>, EventRecordOf<T>> {
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();
		let mut debug_message =
			if debug == DebugInfo::UnsafeDebug { Some(DebugBuffer::default()) } else { None };
		let try_instantiate = || {
			let instantiate_account = T::InstantiateOrigin::ensure_origin(origin.clone())?;
			let (executable, upload_deposit) = match code {
				Code::Upload(code) => {
					let upload_account = T::UploadOrigin::ensure_origin(origin)?;
					let (executable, upload_deposit) =
						Self::try_upload_code(upload_account, code, storage_deposit_limit)?;
					storage_deposit_limit.saturating_reduce(upload_deposit);
					(executable, upload_deposit)
				},
				Code::Existing(code_hash) =>
					(WasmBlob::from_storage(code_hash, &mut gas_meter)?, Default::default()),
			};
			let instantiate_origin = Origin::from_account_id(instantiate_account.clone());
			let mut storage_meter =
				StorageMeter::new(&instantiate_origin, storage_deposit_limit, value)?;
			let result = ExecStack::<T, WasmBlob<T>>::run_instantiate(
				instantiate_account,
				executable,
				&mut gas_meter,
				&mut storage_meter,
				value,
				data,
				salt.as_ref(),
				debug_message.as_mut(),
			);
			storage_deposit = storage_meter
				.try_into_deposit(&instantiate_origin)?
				.saturating_add(&StorageDeposit::Charge(upload_deposit));
			result
		};
		let output = Self::run_guarded(try_instantiate);
		let events = if matches!(collect_events, CollectEvents::UnsafeCollect) {
			Some(System::<T>::read_events_no_consensus().map(|e| *e).collect())
		} else {
			None
		};
		ContractResult {
			result: output
				.map(|(addr, result)| InstantiateReturnValue { result, addr })
				.map_err(|e| e.error),
			gas_consumed: gas_meter.gas_consumed(),
			gas_required: gas_meter.gas_required(),
			storage_deposit,
			debug_message: debug_message.unwrap_or_default().to_vec(),
			events,
		}
	}

	/// A version of [`Self::eth_transact`] used to dry-run Ethereum calls.
	///
	/// # Parameters
	///
	/// - `origin`: The origin of the call.
	/// - `dest`: The destination address of the call.
	/// - `value`: The value to transfer.
	/// - `input`: The input data.
	/// - `gas_limit`: The gas limit enforced during contract execution.
	/// - `storage_deposit_limit`: The maximum balance that can be charged to the caller for storage
	///   usage.
	/// - `utx_encoded_size`: A function that takes a call and returns the encoded size of the
	///   unchecked extrinsic.
	/// - `debug`: Debugging configuration.
	/// - `collect_events`: Event collection configuration.
	pub fn bare_eth_transact(
		origin: T::AccountId,
		dest: Option<H160>,
		value: BalanceOf<T>,
		input: Vec<u8>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		utx_encoded_size: impl Fn(Call<T>) -> u32,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> EthContractResult<BalanceOf<T>>
	where
		T: pallet_transaction_payment::Config,
		<T as frame_system::Config>::RuntimeCall:
			Dispatchable<Info = frame_support::dispatch::DispatchInfo>,
		<T as Config>::RuntimeCall: From<crate::Call<T>>,
		<T as Config>::RuntimeCall: Encode,
		OnChargeTransactionBalanceOf<T>: Into<BalanceOf<T>>,
		T::Nonce: Into<U256>,
	{
		// Get the nonce to encode in the tx.
		let nonce: T::Nonce = <System<T>>::account_nonce(&origin);

		// Use a big enough gas price to ensure that the encoded size is large enough.
		let max_gas_fee: BalanceOf<T> =
			(pallet_transaction_payment::Pallet::<T>::weight_to_fee(Weight::MAX) /
				GAS_PRICE.into())
			.into();

		// A contract call.
		if let Some(dest) = dest {
			// Dry run the call.
			let result = crate::Pallet::<T>::bare_call(
				T::RuntimeOrigin::signed(origin),
				dest,
				value,
				gas_limit,
				storage_deposit_limit,
				input.clone(),
				debug,
				collect_events,
			);

			// Get the encoded size of the transaction.
			let tx = TransactionLegacyUnsigned {
				value: value.into(),
				input: input.into(),
				nonce: nonce.into(),
				chain_id: Some(T::ChainId::get().into()),
				gas_price: GAS_PRICE.into(),
				gas: max_gas_fee.into(),
				to: Some(dest),
				..Default::default()
			};
			let eth_dispatch_call = crate::Call::<T>::eth_transact {
				payload: tx.dummy_signed_payload(),
				gas_limit: result.gas_required,
				storage_deposit_limit: result.storage_deposit.charge_or_zero(),
			};
			let encoded_len = utx_encoded_size(eth_dispatch_call);

			// Get the dispatch info of the call.
			let dispatch_call: <T as Config>::RuntimeCall = crate::Call::<T>::call {
				dest,
				value,
				gas_limit: result.gas_required,
				storage_deposit_limit: result.storage_deposit.charge_or_zero(),
				data: tx.input.0,
			}
			.into();
			let dispatch_info = dispatch_call.get_dispatch_info();

			// Compute the fee.
			let fee = pallet_transaction_payment::Pallet::<T>::compute_fee(
				encoded_len,
				&dispatch_info,
				0u32.into(),
			)
			.into();

			log::debug!(target: LOG_TARGET, "Call dry run Result: dispatch_info: {dispatch_info:?} len: {encoded_len:?} fee: {fee:?}");
			EthContractResult {
				gas_required: result.gas_required,
				storage_deposit: result.storage_deposit.charge_or_zero(),
				result: result.result.map(|v| v.data),
				fee,
			}
			// A contract deployment
		} else {
			// Extract code and data from the input.
			let (code, data) = match polkavm::ProgramBlob::blob_length(&input) {
				Some(blob_len) => blob_len
					.try_into()
					.ok()
					.and_then(|blob_len| (input.split_at_checked(blob_len)))
					.unwrap_or_else(|| (&input[..], &[][..])),
				_ => {
					log::debug!(target: LOG_TARGET, "Failed to extract polkavm blob length");
					(&input[..], &[][..])
				},
			};

			// Dry run the call.
			let result = crate::Pallet::<T>::bare_instantiate(
				T::RuntimeOrigin::signed(origin),
				value,
				gas_limit,
				storage_deposit_limit,
				Code::Upload(code.to_vec()),
				data.to_vec(),
				None,
				debug,
				collect_events,
			);

			// Get the encoded size of the transaction.
			let tx = TransactionLegacyUnsigned {
				gas: max_gas_fee.into(),
				nonce: nonce.into(),
				value: value.into(),
				input: input.clone().into(),
				gas_price: GAS_PRICE.into(),
				chain_id: Some(T::ChainId::get().into()),
				..Default::default()
			};
			let eth_dispatch_call = crate::Call::<T>::eth_transact {
				payload: tx.dummy_signed_payload(),
				gas_limit: result.gas_required,
				storage_deposit_limit: result.storage_deposit.charge_or_zero(),
			};
			let encoded_len = utx_encoded_size(eth_dispatch_call);

			// Get the dispatch info of the call.
			let dispatch_call: <T as Config>::RuntimeCall =
				crate::Call::<T>::instantiate_with_code {
					value,
					gas_limit: result.gas_required,
					storage_deposit_limit: result.storage_deposit.charge_or_zero(),
					code: code.to_vec(),
					data: data.to_vec(),
					salt: None,
				}
				.into();
			let dispatch_info = dispatch_call.get_dispatch_info();

			// Compute the fee.
			let fee = pallet_transaction_payment::Pallet::<T>::compute_fee(
				encoded_len,
				&dispatch_info,
				0u32.into(),
			)
			.into();

			log::debug!(target: LOG_TARGET, "Call dry run Result: dispatch_info: {dispatch_info:?} len: {encoded_len:?} fee: {fee:?}");
			EthContractResult {
				gas_required: result.gas_required,
				storage_deposit: result.storage_deposit.charge_or_zero(),
				result: result.result.map(|v| v.result.data),
				fee,
			}
		}
	}

	/// A generalized version of [`Self::upload_code`].
	///
	/// It is identical to [`Self::upload_code`] and only differs in the information it returns.
	pub fn bare_upload_code(
		origin: OriginFor<T>,
		code: Vec<u8>,
		storage_deposit_limit: BalanceOf<T>,
	) -> CodeUploadResult<BalanceOf<T>> {
		let origin = T::UploadOrigin::ensure_origin(origin)?;
		let (module, deposit) = Self::try_upload_code(origin, code, storage_deposit_limit)?;
		Ok(CodeUploadReturnValue { code_hash: *module.code_hash(), deposit })
	}

	/// Query storage of a specified contract under a specified key.
	pub fn get_storage(address: H160, key: [u8; 32]) -> GetStorageResult {
		let contract_info =
			ContractInfoOf::<T>::get(&address).ok_or(ContractAccessError::DoesntExist)?;

		let maybe_value = contract_info.read(&Key::from_fixed(key));
		Ok(maybe_value)
	}

	/// Uploads new code and returns the Wasm blob and deposit amount collected.
	fn try_upload_code(
		origin: T::AccountId,
		code: Vec<u8>,
		storage_deposit_limit: BalanceOf<T>,
	) -> Result<(WasmBlob<T>, BalanceOf<T>), DispatchError> {
		let mut module = WasmBlob::from_code(code, origin)?;
		let deposit = module.store_code()?;
		ensure!(storage_deposit_limit >= deposit, <Error<T>>::StorageDepositLimitExhausted);
		Ok((module, deposit))
	}

	/// Run the supplied function `f` if no other instance of this pallet is on the stack.
	fn run_guarded<R, F: FnOnce() -> Result<R, ExecError>>(f: F) -> Result<R, ExecError> {
		executing_contract::using_once(&mut false, || {
			executing_contract::with(|f| {
				// Fail if already entered contract execution
				if *f {
					return Err(())
				}
				// We are entering contract execution
				*f = true;
				Ok(())
			})
				.expect("Returns `Ok` if called within `using_once`. It is syntactically obvious that this is the case; qed")
				.map_err(|_| <Error<T>>::ReenteredPallet.into())
				.map(|_| f())
				.and_then(|r| r)
		})
	}
}

impl<T: Config> Pallet<T> {
	/// Return the existential deposit of [`Config::Currency`].
	fn min_balance() -> BalanceOf<T> {
		<T::Currency as Inspect<AccountIdOf<T>>>::minimum_balance()
	}

	/// Deposit a pallet contracts event.
	fn deposit_event(event: Event<T>) {
		<frame_system::Pallet<T>>::deposit_event(<T as Config>::RuntimeEvent::from(event))
	}
}

// Set up a global reference to the boolean flag used for the re-entrancy guard.
environmental!(executing_contract: bool);

sp_api::decl_runtime_apis! {
	/// The API used to dry-run contract interactions.
	#[api_version(1)]
	pub trait ReviveApi<AccountId, Balance, BlockNumber, EventRecord> where
		AccountId: Codec,
		Balance: Codec,
		BlockNumber: Codec,
		EventRecord: Codec,
	{
		/// Perform a call from a specified account to a given contract.
		///
		/// See [`crate::Pallet::bare_call`].
		fn call(
			origin: AccountId,
			dest: H160,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			input_data: Vec<u8>,
		) -> ContractResult<ExecReturnValue, Balance, EventRecord>;

		/// Instantiate a new contract.
		///
		/// See `[crate::Pallet::bare_instantiate]`.
		fn instantiate(
			origin: AccountId,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			code: Code,
			data: Vec<u8>,
			salt: Option<[u8; 32]>,
		) -> ContractResult<InstantiateReturnValue, Balance, EventRecord>;


		/// Perform an Ethereum call.
		///
		/// See [`crate::Pallet::bare_eth_transact`]
		fn eth_transact(
			origin: H160,
			dest: Option<H160>,
			value: Balance,
			input: Vec<u8>,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
		) -> EthContractResult<Balance>;

		/// Upload new code without instantiating a contract from it.
		///
		/// See [`crate::Pallet::bare_upload_code`].
		fn upload_code(
			origin: AccountId,
			code: Vec<u8>,
			storage_deposit_limit: Option<Balance>,
		) -> CodeUploadResult<Balance>;

		/// Query a given storage key in a given contract.
		///
		/// Returns `Ok(Some(Vec<u8>))` if the storage value exists under the given key in the
		/// specified account and `Ok(None)` if it doesn't. If the account specified by the address
		/// doesn't exist, or doesn't have a contract then `Err` is returned.
		fn get_storage(
			address: H160,
			key: [u8; 32],
		) -> GetStorageResult;
	}
}
