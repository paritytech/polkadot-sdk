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
mod primitives;
pub use primitives::*;

mod limits;
mod storage;
mod transient_storage;
mod wasm;

pub mod chain_extension;
pub mod debug;
pub mod migration;
pub mod test_utils;
pub mod weights;

#[cfg(test)]
mod tests;
use crate::{
	exec::{AccountIdOf, ExecError, Executable, Ext, Key, Origin, Stack as ExecStack},
	gas::GasMeter,
	storage::{meter::Meter as StorageMeter, ContractInfo, DeletionQueueManager},
	wasm::{CodeInfo, RuntimeCosts, WasmBlob},
};
use codec::{Codec, Decode, Encode, HasCompact};
use core::fmt::Debug;
use environmental::*;
use frame_support::{
	dispatch::{
		DispatchErrorWithPostInfo, DispatchResultWithPostInfo, GetDispatchInfo, Pays,
		PostDispatchInfo, RawOrigin, WithPostDispatchInfo,
	},
	ensure,
	traits::{
		fungible::{Inspect, Mutate, MutateHold},
		ConstU32, Contains, EnsureOrigin, Get, Time,
	},
	weights::{Weight, WeightMeter},
	BoundedVec, RuntimeDebugNoBound,
};
use frame_system::{
	ensure_signed,
	pallet_prelude::{BlockNumberFor, OriginFor},
	EventRecord, Pallet as System,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{BadOrigin, Convert, Dispatchable, Saturating, StaticLookup},
	DispatchError,
};

pub use crate::{
	address::{AddressGenerator, DefaultAddressGenerator},
	debug::Tracing,
	migration::{MigrateSequence, Migration, NoopMigration},
	pallet::*,
};
pub use weights::WeightInfo;

#[cfg(doc)]
pub use crate::wasm::SyscallDoc;

type CodeHash<T> = <T as frame_system::Config>::Hash;
type TrieId = BoundedVec<u8, ConstU32<128>>;
type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
type CodeVec<T> = BoundedVec<u8, <T as Config>::MaxCodeLen>;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type EventRecordOf<T> =
	EventRecord<<T as frame_system::Config>::RuntimeEvent, <T as frame_system::Config>::Hash>;
type DebugBuffer = BoundedVec<u8, ConstU32<{ limits::DEBUG_BUFFER_BYTES }>>;

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
	use sp_runtime::Perbill;

	/// The in-code storage version.
	pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

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
		/// It is safe to chage this value on a live chain as all refunds are pro rata.
		#[pallet::constant]
		#[pallet::no_default_bounds]
		type DepositPerByte: Get<BalanceOf<Self>>;

		/// The amount of balance a caller has to pay for each storage item.
		///
		/// # Note
		///
		/// It is safe to chage this value on a live chain as all refunds are pro rata.
		#[pallet::constant]
		#[pallet::no_default_bounds]
		type DepositPerItem: Get<BalanceOf<Self>>;

		/// The percentage of the storage deposit that should be held for using a code hash.
		/// Instantiating a contract, or calling [`chain_extension::Ext::lock_delegate_dependency`]
		/// protects the code from being removed. In order to prevent abuse these actions are
		/// protected with a percentage of the code deposit.
		#[pallet::constant]
		type CodeHashLockupDepositPercent: Get<Perbill>;

		/// The address generator used to generate the addresses of contracts.
		#[pallet::no_default_bounds]
		type AddressGenerator: AddressGenerator<Self>;

		/// The maximum length of a contract code in bytes.
		///
		/// This value hugely affects the memory requirements of this pallet since all the code of
		/// all contracts on the call stack will need to be held in memory. Setting of a correct
		/// value will be enforced in [`Pallet::integrity_test`].
		#[pallet::constant]
		type MaxCodeLen: Get<u32>;

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

		/// The sequence of migration steps that will be applied during a migration.
		///
		/// # Examples
		/// ```ignore
		/// use pallet_revive::migration::{v10, v11};
		/// # struct Runtime {};
		/// # struct Currency {};
		/// type Migrations = (v10::Migration<Runtime, Currency>, v11::Migration<Runtime>);
		/// ```
		///
		/// If you have a single migration step, you can use a tuple with a single element:
		/// ```ignore
		/// use pallet_revive::migration::v10;
		/// # struct Runtime {};
		/// # struct Currency {};
		/// type Migrations = (v10::Migration<Runtime, Currency>,);
		/// ```
		type Migrations: MigrateSequence;

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

		/// The amount of memory in bytes that parachain nodes alot to the runtime.
		///
		/// This is used in [`Pallet::integrity_test`] to make sure that the runtime has enough
		/// memory to support this pallet if set to the correct value.
		type RuntimeMemory: Get<u32>;

		/// The amount of memory in bytes that relay chain validators alot to the PoV.
		///
		/// This is used in [`Pallet::integrity_test`] to make sure that the runtime has enough
		/// memory to support this pallet if set to the correct value.
		///
		/// This value is usually higher than [`Self::RuntimeMemory`] to account for the fact
		/// that validators have to hold all storage items in PvF memory.
		type PVFMemory: Get<u32>;
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

			type AddressGenerator = DefaultAddressGenerator;
			type CallFilter = ();
			type ChainExtension = ();
			type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
			type DepositPerByte = DepositPerByte;
			type DepositPerItem = DepositPerItem;
			type MaxCodeLen = ConstU32<{ 123 * 1024 }>;
			type Migrations = ();
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
		}
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		/// Contract deployed by address at the specified address.
		Instantiated { deployer: T::AccountId, contract: T::AccountId },

		/// Contract has been removed.
		///
		/// # Note
		///
		/// The only way for a contract to be removed and emitting this event is by calling
		/// `seal_terminate`.
		Terminated {
			/// The contract that was terminated.
			contract: T::AccountId,
			/// The account that received the contracts remaining balance
			beneficiary: T::AccountId,
		},

		/// Code with the specified hash has been stored.
		CodeStored { code_hash: T::Hash, deposit_held: BalanceOf<T>, uploader: T::AccountId },

		/// A custom event emitted by the contract.
		ContractEmitted {
			/// The contract that emitted the event.
			contract: T::AccountId,
			/// Data supplied by the contract. Metadata generated during contract compilation
			/// is needed to decode it.
			data: Vec<u8>,
		},

		/// A code with the specified hash was removed.
		CodeRemoved { code_hash: T::Hash, deposit_released: BalanceOf<T>, remover: T::AccountId },

		/// A contract's code was updated.
		ContractCodeUpdated {
			/// The contract that has been updated.
			contract: T::AccountId,
			/// New code hash that was set for the contract.
			new_code_hash: T::Hash,
			/// Previous code hash of the contract.
			old_code_hash: T::Hash,
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
			contract: T::AccountId,
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
			contract: T::AccountId,
			/// The code hash that was delegate called.
			code_hash: CodeHash<T>,
		},

		/// Some funds have been transferred and held as storage deposit.
		StorageDepositTransferredAndHeld {
			from: T::AccountId,
			to: T::AccountId,
			amount: BalanceOf<T>,
		},

		/// Some storage deposit funds have been transferred and released.
		StorageDepositTransferredAndReleased {
			from: T::AccountId,
			to: T::AccountId,
			amount: BalanceOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid schedule supplied, e.g. with zero weight of a basic operation.
		InvalidSchedule,
		/// Invalid combination of flags supplied to `seal_call` or `seal_delegate_call`.
		InvalidCallFlags,
		/// The executed contract exhausted its gas limit.
		OutOfGas,
		/// The output buffer supplied to a contract API call was too small.
		OutputBufferTooSmall,
		/// Performing the requested transfer failed. Probably because there isn't enough
		/// free balance in the sender's account.
		TransferFailed,
		/// Performing a call was denied because the calling depth reached the limit
		/// of what is specified in the schedule.
		MaxCallDepthReached,
		/// No contract was found at the specified address.
		ContractNotFound,
		/// The code supplied to `instantiate_with_code` exceeds the limit specified in the
		/// current schedule.
		CodeTooLarge,
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
		/// A pending migration needs to complete before the extrinsic can be called.
		MigrationInProgress,
		/// Migrate dispatch call was attempted but no migration was performed.
		NoMigrationPerformed,
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
	pub(crate) type PristineCode<T: Config> = StorageMap<_, Identity, CodeHash<T>, CodeVec<T>>;

	/// A mapping from a contract's code hash to its code info.
	#[pallet::storage]
	pub(crate) type CodeInfoOf<T: Config> = StorageMap<_, Identity, CodeHash<T>, CodeInfo<T>>;

	/// The code associated with a given account.
	#[pallet::storage]
	pub(crate) type ContractInfoOf<T: Config> =
		StorageMap<_, Identity, T::AccountId, ContractInfo<T>>;

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

	/// A migration can span across multiple blocks. This storage defines a cursor to track the
	/// progress of the migration, enabling us to resume from the last completed position.
	#[pallet::storage]
	pub(crate) type MigrationInProgress<T: Config> =
		StorageValue<_, migration::Cursor, OptionQuery>;

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
			use migration::MigrateResult::*;
			let mut meter = WeightMeter::with_limit(limit);

			loop {
				match Migration::<T>::migrate(&mut meter) {
					// There is not enough weight to perform a migration.
					// We can't do anything more, so we return the used weight.
					NoMigrationPerformed | InProgress { steps_done: 0 } => return meter.consumed(),
					// Migration is still in progress, we can start the next step.
					InProgress { .. } => continue,
					// Either no migration is in progress, or we are done with all migrations, we
					// can do some more other work with the remaining weight.
					Completed | NoMigrationInProgress => break,
				}
			}

			ContractInfo::<T>::process_deletion_queue_batch(&mut meter);
			meter.consumed()
		}

		fn integrity_test() {
			Migration::<T>::integrity_test();

			// Total runtime memory limit
			let max_runtime_mem: u32 = T::RuntimeMemory::get();
			// Memory limits for a single contract:
			// Value stack size: 1Mb per contract, default defined in wasmi
			const MAX_STACK_SIZE: u32 = 1024 * 1024;
			// Heap limit is normally 16 mempages of 64kb each = 1Mb per contract
			let max_heap_size = limits::MEMORY_BYTES;
			// The root frame is not accounted in CALL_STACK_DEPTH
			let max_call_depth =
				limits::CALL_STACK_DEPTH.checked_add(1).expect("CallStack size is too big");
			// Transient storage uses a BTreeMap, which has overhead compared to the raw size of
			// key-value data. To ensure safety, a margin of 2x the raw key-value size is used.
			let max_transient_storage_size = limits::TRANSIENT_STORAGE_BYTES
				.checked_mul(2)
				.expect("MaxTransientStorageSize is too large");

			// Check that given configured `MaxCodeLen`, runtime heap memory limit can't be broken.
			//
			// In worst case, the decoded Wasm contract code would be `x16` times larger than the
			// encoded one. This is because even a single-byte wasm instruction has 16-byte size in
			// wasmi. This gives us `MaxCodeLen*16` safety margin.
			//
			// Next, the pallet keeps the Wasm blob for each
			// contract, hence we add up `MaxCodeLen` to the safety margin.
			//
			// The inefficiencies of the freeing-bump allocator
			// being used in the client for the runtime memory allocations, could lead to possible
			// memory allocations for contract code grow up to `x4` times in some extreme cases,
			// which gives us total multiplier of `17*4` for `MaxCodeLen`.
			//
			// That being said, for every contract executed in runtime, at least `MaxCodeLen*17*4`
			// memory should be available. Note that maximum allowed heap memory and stack size per
			// each contract (stack frame) should also be counted.
			//
			// The pallet holds transient storage with a size up to `max_transient_storage_size`.
			//
			// Finally, we allow 50% of the runtime memory to be utilized by the contracts call
			// stack, keeping the rest for other facilities, such as PoV, etc.
			//
			// This gives us the following formula:
			//
			// `(MaxCodeLen * 17 * 4 + MAX_STACK_SIZE + max_heap_size) * max_call_depth +
			// max_transient_storage_size < max_runtime_mem/2`
			//
			// Hence the upper limit for the `MaxCodeLen` can be defined as follows:
			let code_len_limit = max_runtime_mem
				.saturating_div(2)
				.saturating_sub(max_transient_storage_size)
				.saturating_div(max_call_depth)
				.saturating_sub(max_heap_size)
				.saturating_sub(MAX_STACK_SIZE)
				.saturating_div(17 * 4);

			assert!(
				T::MaxCodeLen::get() < code_len_limit,
				"Given `CallStack` height {:?}, `MaxCodeLen` should be set less than {:?} \
				 (current value is {:?}), to avoid possible runtime oom issues.",
				max_call_depth,
				code_len_limit,
				T::MaxCodeLen::get(),
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

			// We can use storage to store items using the available block ref_time with the
			// `set_storage` host function.
			let max_storage_size: u32 = ((max_block_ref_time /
				(<RuntimeCosts as gas::Token<T>>::weight(&RuntimeCosts::SetStorage {
					new_bytes: max_payload_size,
					old_bytes: 0,
				})
				.ref_time()))
			.saturating_mul(max_payload_size.saturating_add(max_key_size) as u64))
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
		<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
	{
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
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::call().saturating_add(*gas_limit))]
		pub fn call(
			origin: OriginFor<T>,
			dest: AccountIdLookupOf<T>,
			#[pallet::compact] value: BalanceOf<T>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			data: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let dest = T::Lookup::lookup(dest)?;
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
		#[pallet::call_index(1)]
		#[pallet::weight(
			T::WeightInfo::instantiate(data.len() as u32, salt.len() as u32).saturating_add(*gas_limit)
		)]
		pub fn instantiate(
			origin: OriginFor<T>,
			#[pallet::compact] value: BalanceOf<T>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			code_hash: CodeHash<T>,
			data: Vec<u8>,
			salt: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let data_len = data.len() as u32;
			let salt_len = salt.len() as u32;
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
				T::WeightInfo::instantiate(data_len, salt_len),
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
		/// * `salt`: Used for the address derivation. See [`Pallet::contract_address`].
		///
		/// Instantiation is executed as follows:
		///
		/// - The supplied `code` is deployed, and a `code_hash` is created for that code.
		/// - If the `code_hash` already exists on the chain the underlying `code` will be shared.
		/// - The destination address is computed based on the sender, code_hash and the salt.
		/// - The smart-contract account is created at the computed address.
		/// - The `value` is transferred to the new account.
		/// - The `deploy` function is executed in the context of the newly-created account.
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::WeightInfo::instantiate_with_code(code.len() as u32, data.len() as u32, salt.len() as u32)
			.saturating_add(*gas_limit)
		)]
		pub fn instantiate_with_code(
			origin: OriginFor<T>,
			#[pallet::compact] value: BalanceOf<T>,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			code: Vec<u8>,
			data: Vec<u8>,
			salt: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let code_len = code.len() as u32;
			let data_len = data.len() as u32;
			let salt_len = salt.len() as u32;
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
				T::WeightInfo::instantiate_with_code(code_len, data_len, salt_len),
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
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::upload_code_determinism_enforced(code.len() as u32))]
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
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::remove_code())]
		pub fn remove_code(
			origin: OriginFor<T>,
			code_hash: CodeHash<T>,
		) -> DispatchResultWithPostInfo {
			Migration::<T>::ensure_migrated()?;
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
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::set_code())]
		pub fn set_code(
			origin: OriginFor<T>,
			dest: AccountIdLookupOf<T>,
			code_hash: CodeHash<T>,
		) -> DispatchResult {
			Migration::<T>::ensure_migrated()?;
			ensure_root(origin)?;
			let dest = T::Lookup::lookup(dest)?;
			<ContractInfoOf<T>>::try_mutate(&dest, |contract| {
				let contract = if let Some(contract) = contract {
					contract
				} else {
					return Err(<Error<T>>::ContractNotFound.into())
				};
				<ExecStack<T, WasmBlob<T>>>::increment_refcount(code_hash)?;
				<ExecStack<T, WasmBlob<T>>>::decrement_refcount(contract.code_hash);
				Self::deposit_event(Event::ContractCodeUpdated {
					contract: dest.clone(),
					new_code_hash: code_hash,
					old_code_hash: contract.code_hash,
				});
				contract.code_hash = code_hash;
				Ok(())
			})
		}

		/// When a migration is in progress, this dispatchable can be used to run migration steps.
		/// Calls that contribute to advancing the migration have their fees waived, as it's helpful
		/// for the chain. Note that while the migration is in progress, the pallet will also
		/// leverage the `on_idle` hooks to run migration steps.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::migrate().saturating_add(*weight_limit))]
		pub fn migrate(origin: OriginFor<T>, weight_limit: Weight) -> DispatchResultWithPostInfo {
			use migration::MigrateResult::*;
			ensure_signed(origin)?;

			let weight_limit = weight_limit.saturating_add(T::WeightInfo::migrate());
			let mut meter = WeightMeter::with_limit(weight_limit);
			let result = Migration::<T>::migrate(&mut meter);

			match result {
				Completed => Ok(PostDispatchInfo {
					actual_weight: Some(meter.consumed()),
					pays_fee: Pays::No,
				}),
				InProgress { steps_done, .. } if steps_done > 0 => Ok(PostDispatchInfo {
					actual_weight: Some(meter.consumed()),
					pays_fee: Pays::No,
				}),
				InProgress { .. } => Ok(PostDispatchInfo {
					actual_weight: Some(meter.consumed()),
					pays_fee: Pays::Yes,
				}),
				NoMigrationInProgress | NoMigrationPerformed => {
					let err: DispatchError = <Error<T>>::NoMigrationPerformed.into();
					Err(err.with_weight(meter.consumed()))
				},
			}
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

impl<T: Config> Pallet<T> {
	/// A generalized version of [`Self::call`].
	///
	/// Identical to [`Self::call`] but tailored towards being called by other code within the
	/// runtime as opposed to from an extrinsic. It returns more information and allows the
	/// enablement of features that are not suitable for an extrinsic (debugging, event
	/// collection).
	pub fn bare_call(
		origin: OriginFor<T>,
		dest: T::AccountId,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		data: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractExecResult<BalanceOf<T>, EventRecordOf<T>> {
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();
		let mut debug_message = if matches!(debug, DebugInfo::UnsafeDebug) {
			Some(DebugBuffer::default())
		} else {
			None
		};
		let try_call = || {
			Migration::<T>::ensure_migrated()?;
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
		ContractExecResult {
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
		code: Code<CodeHash<T>>,
		data: Vec<u8>,
		salt: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractInstantiateResult<T::AccountId, BalanceOf<T>, EventRecordOf<T>> {
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();
		let mut debug_message =
			if debug == DebugInfo::UnsafeDebug { Some(DebugBuffer::default()) } else { None };
		let try_instantiate = || {
			Migration::<T>::ensure_migrated()?;
			let instantiate_account = T::InstantiateOrigin::ensure_origin(origin.clone())?;
			let (executable, upload_deposit) = match code {
				Code::Upload(code) => {
					let upload_account = T::UploadOrigin::ensure_origin(origin)?;
					let (executable, upload_deposit) = Self::try_upload_code(
						upload_account,
						code,
						storage_deposit_limit,
						debug_message.as_mut(),
					)?;
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
				&salt,
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
		ContractInstantiateResult {
			result: output
				.map(|(account_id, result)| InstantiateReturnValue { result, account_id })
				.map_err(|e| e.error),
			gas_consumed: gas_meter.gas_consumed(),
			gas_required: gas_meter.gas_required(),
			storage_deposit,
			debug_message: debug_message.unwrap_or_default().to_vec(),
			events,
		}
	}

	/// A generalized version of [`Self::upload_code`].
	///
	/// It is identical to [`Self::upload_code`] and only differs in the information it returns.
	pub fn bare_upload_code(
		origin: OriginFor<T>,
		code: Vec<u8>,
		storage_deposit_limit: BalanceOf<T>,
	) -> CodeUploadResult<CodeHash<T>, BalanceOf<T>> {
		Migration::<T>::ensure_migrated()?;
		let origin = T::UploadOrigin::ensure_origin(origin)?;
		let (module, deposit) = Self::try_upload_code(origin, code, storage_deposit_limit, None)?;
		Ok(CodeUploadReturnValue { code_hash: *module.code_hash(), deposit })
	}

	/// Query storage of a specified contract under a specified key.
	pub fn get_storage(address: T::AccountId, key: Vec<u8>) -> GetStorageResult {
		if Migration::<T>::in_progress() {
			return Err(ContractAccessError::MigrationInProgress)
		}
		let contract_info =
			ContractInfoOf::<T>::get(&address).ok_or(ContractAccessError::DoesntExist)?;

		let maybe_value = contract_info.read(
			&Key::try_from_var(key)
				.map_err(|_| ContractAccessError::KeyDecodingFailed)?
				.into(),
		);
		Ok(maybe_value)
	}

	/// Determine the address of a contract.
	///
	/// This is the address generation function used by contract instantiation. See
	/// [`DefaultAddressGenerator`] for the default implementation.
	pub fn contract_address(
		deploying_address: &T::AccountId,
		code_hash: &CodeHash<T>,
		input_data: &[u8],
		salt: &[u8],
	) -> T::AccountId {
		T::AddressGenerator::contract_address(deploying_address, code_hash, input_data, salt)
	}

	/// Uploads new code and returns the Wasm blob and deposit amount collected.
	fn try_upload_code(
		origin: T::AccountId,
		code: Vec<u8>,
		storage_deposit_limit: BalanceOf<T>,
		mut debug_message: Option<&mut DebugBuffer>,
	) -> Result<(WasmBlob<T>, BalanceOf<T>), DispatchError> {
		let mut module = WasmBlob::from_code(code, origin).map_err(|(err, msg)| {
			debug_message.as_mut().map(|d| d.try_extend(msg.bytes()));
			err
		})?;
		let deposit = module.store_code()?;
		ensure!(storage_deposit_limit >= deposit, <Error<T>>::StorageDepositLimitExhausted);
		Ok((module, deposit))
	}

	/// Deposit a pallet contracts event.
	fn deposit_event(event: Event<T>) {
		<frame_system::Pallet<T>>::deposit_event(<T as Config>::RuntimeEvent::from(event))
	}

	/// Deposit a pallet contracts indexed event.
	fn deposit_indexed_event(topics: Vec<T::Hash>, event: Event<T>) {
		<frame_system::Pallet<T>>::deposit_event_indexed(
			&topics,
			<T as Config>::RuntimeEvent::from(event).into(),
		)
	}

	/// Return the existential deposit of [`Config::Currency`].
	fn min_balance() -> BalanceOf<T> {
		<T::Currency as Inspect<AccountIdOf<T>>>::minimum_balance()
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

// Set up a global reference to the boolean flag used for the re-entrancy guard.
environmental!(executing_contract: bool);

sp_api::decl_runtime_apis! {
	/// The API used to dry-run contract interactions.
	#[api_version(1)]
	pub trait ReviveApi<AccountId, Balance, BlockNumber, Hash, EventRecord> where
		AccountId: Codec,
		Balance: Codec,
		BlockNumber: Codec,
		Hash: Codec,
		EventRecord: Codec,
	{
		/// Perform a call from a specified account to a given contract.
		///
		/// See [`crate::Pallet::bare_call`].
		fn call(
			origin: AccountId,
			dest: AccountId,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			input_data: Vec<u8>,
		) -> ContractExecResult<Balance, EventRecord>;

		/// Instantiate a new contract.
		///
		/// See `[crate::Pallet::bare_instantiate]`.
		fn instantiate(
			origin: AccountId,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			code: Code<Hash>,
			data: Vec<u8>,
			salt: Vec<u8>,
		) -> ContractInstantiateResult<AccountId, Balance, EventRecord>;

		/// Upload new code without instantiating a contract from it.
		///
		/// See [`crate::Pallet::bare_upload_code`].
		fn upload_code(
			origin: AccountId,
			code: Vec<u8>,
			storage_deposit_limit: Option<Balance>,
		) -> CodeUploadResult<Hash, Balance>;

		/// Query a given storage key in a given contract.
		///
		/// Returns `Ok(Some(Vec<u8>))` if the storage value exists under the given key in the
		/// specified account and `Ok(None)` if it doesn't. If the account specified by the address
		/// doesn't exist, or doesn't have a contract then `Err` is returned.
		fn get_storage(
			address: AccountId,
			key: Vec<u8>,
		) -> GetStorageResult;
	}
}
