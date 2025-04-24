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
mod exec;
mod gas;
mod limits;
mod primitives;
mod pure_precompiles;
mod storage;
mod transient_storage;
mod wasm;

#[cfg(test)]
mod tests;

pub mod chain_extension;
pub mod evm;
pub mod test_utils;
pub mod tracing;
pub mod weights;

use crate::{
	evm::{
		runtime::GAS_PRICE, CallTrace, GasEncoder, GenericTransaction, TracerConfig, TYPE_EIP1559,
	},
	exec::{AccountIdOf, ExecError, Executable, Key, Stack as ExecStack},
	gas::GasMeter,
	storage::{meter::Meter as StorageMeter, ContractInfo, DeletionQueueManager},
	wasm::{CodeInfo, RuntimeCosts, WasmBlob},
};
use alloc::{boxed::Box, format, vec};
use codec::{Codec, Decode, Encode};
use environmental::*;
use frame_support::{
	dispatch::{
		DispatchErrorWithPostInfo, DispatchInfo, DispatchResultWithPostInfo, GetDispatchInfo, Pays,
		PostDispatchInfo, RawOrigin,
	},
	ensure,
	pallet_prelude::DispatchClass,
	traits::{
		fungible::{Inspect, Mutate, MutateHold},
		tokens::{Fortitude::Polite, Preservation::Preserve},
		ConstU32, ConstU64, Contains, EnsureOrigin, Get, IsType, OriginTrait, Time,
	},
	weights::{Weight, WeightMeter},
	BoundedVec, RuntimeDebugNoBound,
};
use frame_system::{
	ensure_signed,
	pallet_prelude::{BlockNumberFor, OriginFor},
	Pallet as System,
};
use scale_info::TypeInfo;
use sp_core::{H160, H256, U256};
use sp_runtime::{
	traits::{BadOrigin, Bounded, Convert, Dispatchable, Saturating, Zero},
	AccountId32, DispatchError,
};

pub use crate::{
	address::{create1, create2, is_eth_derived, AccountId32Mapper, AddressMapper},
	exec::{MomentOf, Origin},
	pallet::*,
};
pub use primitives::*;
pub use weights::WeightInfo;

#[cfg(doc)]
pub use crate::wasm::SyscallDoc;

type TrieId = BoundedVec<u8, ConstU32<128>>;
type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
type CodeVec = BoundedVec<u8, ConstU32<{ limits::code::BLOB_BYTES }>>;
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

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, traits::FindAuthor};
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
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		#[pallet::no_default_bounds]
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo;

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

		/// Find the author of the current block.
		type FindAuthor: FindAuthor<Self::AccountId>;

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
		/// Instantiating a contract, protects the code from being removed. In order to prevent
		/// abuse these actions are protected with a percentage of the code deposit.
		#[pallet::constant]
		type CodeHashLockupDepositPercent: Get<Perbill>;

		/// Use either valid type is [`address::AccountId32Mapper`] or [`address::H160Mapper`].
		#[pallet::no_default]
		type AddressMapper: AddressMapper<Self>;

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

		/// The ratio between the decimal representation of the native token and the ETH token.
		#[pallet::constant]
		type NativeToEthRatio: Get<u32>;

		/// Encode and decode Ethereum gas values.
		/// Only valid value is `()`. See [`GasEncoder`].
		#[pallet::no_default_bounds]
		type EthGasEncoder: GasEncoder<BalanceOf<Self>>;
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
			type Xcm = ();
			type RuntimeMemory = ConstU32<{ 128 * 1024 * 1024 }>;
			type PVFMemory = ConstU32<{ 512 * 1024 * 1024 }>;
			type ChainId = ConstU64<0>;
			type NativeToEthRatio = ConstU32<1>;
			type EthGasEncoder = ();
			type FindAuthor = ();
		}
	}

	#[pallet::event]
	pub enum Event<T: Config> {
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
	}

	#[pallet::error]
	#[repr(u8)]
	pub enum Error<T> {
		/// Invalid schedule supplied, e.g. with zero weight of a basic operation.
		InvalidSchedule = 0x01,
		/// Invalid combination of flags supplied to `seal_call` or `seal_delegate_call`.
		InvalidCallFlags = 0x02,
		/// The executed contract exhausted its gas limit.
		OutOfGas = 0x03,
		/// Performing the requested transfer failed. Probably because there isn't enough
		/// free balance in the sender's account.
		TransferFailed = 0x04,
		/// Performing a call was denied because the calling depth reached the limit
		/// of what is specified in the schedule.
		MaxCallDepthReached = 0x05,
		/// No contract was found at the specified address.
		ContractNotFound = 0x06,
		/// No code could be found at the supplied code hash.
		CodeNotFound = 0x07,
		/// No code info could be found at the supplied code hash.
		CodeInfoNotFound = 0x08,
		/// A buffer outside of sandbox memory was passed to a contract API function.
		OutOfBounds = 0x09,
		/// Input passed to a contract API function failed to decode as expected type.
		DecodingFailed = 0x0A,
		/// Contract trapped during execution.
		ContractTrapped = 0x0B,
		/// The size defined in `T::MaxValueSize` was exceeded.
		ValueTooLarge = 0x0C,
		/// Termination of a contract is not allowed while the contract is already
		/// on the call stack. Can be triggered by `seal_terminate`.
		TerminatedWhileReentrant = 0x0D,
		/// `seal_call` forwarded this contracts input. It therefore is no longer available.
		InputForwarded = 0x0E,
		/// The amount of topics passed to `seal_deposit_events` exceeds the limit.
		TooManyTopics = 0x0F,
		/// The chain does not provide a chain extension. Calling the chain extension results
		/// in this error. Note that this usually  shouldn't happen as deploying such contracts
		/// is rejected.
		NoChainExtension = 0x10,
		/// Failed to decode the XCM program.
		XCMDecodeFailed = 0x11,
		/// A contract with the same AccountId already exists.
		DuplicateContract = 0x12,
		/// A contract self destructed in its constructor.
		///
		/// This can be triggered by a call to `seal_terminate`.
		TerminatedInConstructor = 0x13,
		/// A call tried to invoke a contract that is flagged as non-reentrant.
		ReentranceDenied = 0x14,
		/// A contract called into the runtime which then called back into this pallet.
		ReenteredPallet = 0x15,
		/// A contract attempted to invoke a state modifying API while being in read-only mode.
		StateChangeDenied = 0x16,
		/// Origin doesn't have enough balance to pay the required storage deposits.
		StorageDepositNotEnoughFunds = 0x17,
		/// More storage was created than allowed by the storage deposit limit.
		StorageDepositLimitExhausted = 0x18,
		/// Code removal was denied because the code is still in use by at least one contract.
		CodeInUse = 0x19,
		/// The contract ran to completion but decided to revert its storage changes.
		/// Please note that this error is only returned from extrinsics. When called directly
		/// or via RPC an `Ok` will be returned. In this case the caller needs to inspect the flags
		/// to determine whether a reversion has taken place.
		ContractReverted = 0x1A,
		/// The contract failed to compile or is missing the correct entry points.
		///
		/// A more detailed error can be found on the node console if debug messages are enabled
		/// by supplying `-lruntime::revive=debug`.
		CodeRejected = 0x1B,
		/// The code blob supplied is larger than [`limits::code::BLOB_BYTES`].
		BlobTooLarge = 0x1C,
		/// The static memory consumption of the blob will be larger than
		/// [`limits::code::STATIC_MEMORY_BYTES`].
		StaticMemoryTooLarge = 0x1D,
		/// The program contains a basic block that is larger than allowed.
		BasicBlockTooLarge = 0x1E,
		/// The program contains an invalid instruction.
		InvalidInstruction = 0x1F,
		/// The contract has reached its maximum number of delegate dependencies.
		MaxDelegateDependenciesReached = 0x20,
		/// The dependency was not found in the contract's delegate dependencies.
		DelegateDependencyNotFound = 0x21,
		/// The contract already depends on the given delegate dependency.
		DelegateDependencyAlreadyExists = 0x22,
		/// Can not add a delegate dependency to the code hash of the contract itself.
		CannotAddSelfAsDelegateDependency = 0x23,
		/// Can not add more data to transient storage.
		OutOfTransientStorage = 0x24,
		/// The contract tried to call a syscall which does not exist (at its current api level).
		InvalidSyscall = 0x25,
		/// Invalid storage flags were passed to one of the storage syscalls.
		InvalidStorageFlags = 0x26,
		/// PolkaVM failed during code execution. Probably due to a malformed program.
		ExecutionFailed = 0x27,
		/// Failed to convert a U256 to a Balance.
		BalanceConversionFailed = 0x28,
		/// Failed to convert an EVM balance to a native balance.
		DecimalPrecisionLoss = 0x29,
		/// Immutable data can only be set during deploys and only be read during calls.
		/// Additionally, it is only valid to set the data once and it must not be empty.
		InvalidImmutableAccess = 0x2A,
		/// An `AccountID32` account tried to interact with the pallet without having a mapping.
		///
		/// Call [`Pallet::map_account`] in order to create a mapping for the account.
		AccountUnmapped = 0x2B,
		/// Tried to map an account that is already mapped.
		AccountAlreadyMapped = 0x2C,
		/// The transaction used to dry-run a contract is invalid.
		InvalidGenericTransaction = 0x2D,
		/// The refcount of a code either over or underflowed.
		RefcountOverOrUnderflow = 0x2E,
		/// Unsupported precompile address
		UnsupportedPrecompileAddress = 0x2F,
		/// Precompile Error
		PrecompileFailure = 0x30,
	}

	/// A reason for the pallet contracts placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The Pallet has reserved it for storing code on-chain.
		CodeUploadDepositReserve,
		/// The Pallet has reserved it for storage deposit.
		StorageDepositReserve,
		/// Deposit for creating an address mapping in [`OriginalAccount`].
		AddressMapping,
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

	/// Map a Ethereum address to its original `AccountId32`.
	///
	/// When deriving a `H160` from an `AccountId32` we use a hash function. In order to
	/// reconstruct the original account we need to store the reverse mapping here.
	/// Register your `AccountId32` using [`Pallet::map_account`] in order to
	/// use it with this pallet.
	#[pallet::storage]
	pub(crate) type OriginalAccount<T: Config> = StorageMap<_, Identity, H160, AccountId32>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Genesis mapped accounts
		pub mapped_accounts: Vec<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for id in &self.mapped_accounts {
				if let Err(err) = T::AddressMapper::map(id) {
					log::error!(target: LOG_TARGET, "Failed to map account {id:?}: {err:?}");
				}
			}
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
				.saturating_add(<RuntimeCosts as gas::Token<T>>::weight(&RuntimeCosts::HostFn))
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
		T::Hash: frame_support::traits::IsType<H256>,
	{
		/// A raw EVM transaction, typically dispatched by an Ethereum JSON-RPC server.
		///
		/// # Parameters
		///
		/// * `payload`: The encoded [`crate::evm::TransactionSigned`].
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
		pub fn eth_transact(origin: OriginFor<T>, payload: Vec<u8>) -> DispatchResultWithPostInfo {
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
			let mut output = Self::bare_call(
				origin,
				dest,
				value,
				gas_limit,
				DepositLimit::Balance(storage_deposit_limit),
				data,
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
				DepositLimit::Balance(storage_deposit_limit),
				Code::Existing(code_hash),
				data,
				salt,
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
				DepositLimit::Balance(storage_deposit_limit),
				Code::Upload(code),
				data,
				salt,
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
				<CodeInfo<T>>::increment_refcount(code_hash)?;
				<CodeInfo<T>>::decrement_refcount(contract.code_hash)?;
				contract.code_hash = code_hash;
				Ok(())
			})
		}

		/// Register the callers account id so that it can be used in contract interactions.
		///
		/// This will error if the origin is already mapped or is a eth native `Address20`. It will
		/// take a deposit that can be released by calling [`Self::unmap_account`].
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::map_account())]
		pub fn map_account(origin: OriginFor<T>) -> DispatchResult {
			let origin = ensure_signed(origin)?;
			T::AddressMapper::map(&origin)
		}

		/// Unregister the callers account id in order to free the deposit.
		///
		/// There is no reason to ever call this function other than freeing up the deposit.
		/// This is only useful when the account should no longer be used.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::unmap_account())]
		pub fn unmap_account(origin: OriginFor<T>) -> DispatchResult {
			let origin = ensure_signed(origin)?;
			T::AddressMapper::unmap(&origin)
		}

		/// Dispatch an `call` with the origin set to the callers fallback address.
		///
		/// Every `AccountId32` can control its corresponding fallback account. The fallback account
		/// is the `AccountId20` with the last 12 bytes set to `0xEE`. This is essentially a
		/// recovery function in case an `AccountId20` was used without creating a mapping first.
		#[pallet::call_index(9)]
		#[pallet::weight({
			let dispatch_info = call.get_dispatch_info();
			(
				T::WeightInfo::dispatch_as_fallback_account().saturating_add(dispatch_info.call_weight),
				dispatch_info.class
			)
		})]
		pub fn dispatch_as_fallback_account(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			let origin = ensure_signed(origin)?;
			let unmapped_account =
				T::AddressMapper::to_fallback_account_id(&T::AddressMapper::to_address(&origin));
			call.dispatch(RawOrigin::Signed(unmapped_account).into())
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
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
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
		storage_deposit_limit: DepositLimit<BalanceOf<T>>,
		data: Vec<u8>,
	) -> ContractResult<ExecReturnValue, BalanceOf<T>> {
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();

		let try_call = || {
			let origin = Origin::from_runtime_origin(origin)?;
			let mut storage_meter = match storage_deposit_limit {
				DepositLimit::Balance(limit) => StorageMeter::new(&origin, limit, value)?,
				DepositLimit::Unchecked => StorageMeter::new_unchecked(BalanceOf::<T>::max_value()),
			};
			let result = ExecStack::<T, WasmBlob<T>>::run_call(
				origin.clone(),
				dest,
				&mut gas_meter,
				&mut storage_meter,
				Self::convert_native_to_evm(value),
				data,
				storage_deposit_limit.is_unchecked(),
			)?;
			storage_deposit = storage_meter
				.try_into_deposit(&origin, storage_deposit_limit.is_unchecked())
				.inspect_err(|err| {
					log::debug!(target: LOG_TARGET, "Failed to transfer deposit: {err:?}");
				})?;
			Ok(result)
		};
		let result = Self::run_guarded(try_call);
		ContractResult {
			result: result.map_err(|r| r.error),
			gas_consumed: gas_meter.gas_consumed(),
			gas_required: gas_meter.gas_required(),
			storage_deposit,
		}
	}

	/// A generalized version of [`Self::instantiate`] or [`Self::instantiate_with_code`].
	///
	/// Identical to [`Self::instantiate`] or [`Self::instantiate_with_code`] but tailored towards
	/// being called by other code within the runtime as opposed to from an extrinsic. It returns
	/// more information to the caller useful to estimate the cost of the operation.
	pub fn bare_instantiate(
		origin: OriginFor<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: DepositLimit<BalanceOf<T>>,
		code: Code,
		data: Vec<u8>,
		salt: Option<[u8; 32]>,
	) -> ContractResult<InstantiateReturnValue, BalanceOf<T>> {
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();
		let unchecked_deposit_limit = storage_deposit_limit.is_unchecked();
		let mut storage_deposit_limit = match storage_deposit_limit {
			DepositLimit::Balance(limit) => limit,
			DepositLimit::Unchecked => BalanceOf::<T>::max_value(),
		};

		let try_instantiate = || {
			let instantiate_account = T::InstantiateOrigin::ensure_origin(origin.clone())?;
			let (executable, upload_deposit) = match code {
				Code::Upload(code) => {
					let upload_account = T::UploadOrigin::ensure_origin(origin)?;
					let (executable, upload_deposit) = Self::try_upload_code(
						upload_account,
						code,
						storage_deposit_limit,
						unchecked_deposit_limit,
					)?;
					storage_deposit_limit.saturating_reduce(upload_deposit);
					(executable, upload_deposit)
				},
				Code::Existing(code_hash) =>
					(WasmBlob::from_storage(code_hash, &mut gas_meter)?, Default::default()),
			};
			let instantiate_origin = Origin::from_account_id(instantiate_account.clone());
			let mut storage_meter = if unchecked_deposit_limit {
				StorageMeter::new_unchecked(storage_deposit_limit)
			} else {
				StorageMeter::new(&instantiate_origin, storage_deposit_limit, value)?
			};

			let result = ExecStack::<T, WasmBlob<T>>::run_instantiate(
				instantiate_account,
				executable,
				&mut gas_meter,
				&mut storage_meter,
				Self::convert_native_to_evm(value),
				data,
				salt.as_ref(),
				unchecked_deposit_limit,
			);
			storage_deposit = storage_meter
				.try_into_deposit(&instantiate_origin, unchecked_deposit_limit)?
				.saturating_add(&StorageDeposit::Charge(upload_deposit));
			result
		};
		let output = Self::run_guarded(try_instantiate);
		ContractResult {
			result: output
				.map(|(addr, result)| InstantiateReturnValue { result, addr })
				.map_err(|e| e.error),
			gas_consumed: gas_meter.gas_consumed(),
			gas_required: gas_meter.gas_required(),
			storage_deposit,
		}
	}

	/// A version of [`Self::eth_transact`] used to dry-run Ethereum calls.
	///
	/// # Parameters
	///
	/// - `tx`: The Ethereum transaction to simulate.
	/// - `gas_limit`: The gas limit enforced during contract execution.
	/// - `tx_fee`: A function that returns the fee for the given call and dispatch info.
	pub fn bare_eth_transact(
		mut tx: GenericTransaction,
		gas_limit: Weight,
		tx_fee: impl Fn(Call<T>, DispatchInfo) -> BalanceOf<T>,
	) -> Result<EthTransactInfo<BalanceOf<T>>, EthTransactError>
	where
		<T as frame_system::Config>::RuntimeCall:
			Dispatchable<Info = frame_support::dispatch::DispatchInfo>,
		<T as Config>::RuntimeCall: From<crate::Call<T>>,
		<T as Config>::RuntimeCall: Encode,
		T::Nonce: Into<U256>,
		T::Hash: frame_support::traits::IsType<H256>,
	{
		log::trace!(target: LOG_TARGET, "bare_eth_transact: tx: {tx:?} gas_limit: {gas_limit:?}");

		let from = tx.from.unwrap_or_default();
		let origin = T::AddressMapper::to_account_id(&from);

		let storage_deposit_limit = if tx.gas.is_some() {
			DepositLimit::Balance(BalanceOf::<T>::max_value())
		} else {
			DepositLimit::Unchecked
		};

		if tx.nonce.is_none() {
			tx.nonce = Some(<System<T>>::account_nonce(&origin).into());
		}
		if tx.chain_id.is_none() {
			tx.chain_id = Some(T::ChainId::get().into());
		}
		if tx.gas_price.is_none() {
			tx.gas_price = Some(GAS_PRICE.into());
		}
		if tx.max_priority_fee_per_gas.is_none() {
			tx.max_priority_fee_per_gas = Some(GAS_PRICE.into());
		}
		if tx.max_fee_per_gas.is_none() {
			tx.max_fee_per_gas = Some(GAS_PRICE.into());
		}
		if tx.gas.is_none() {
			tx.gas = Some(Self::evm_block_gas_limit());
		}
		if tx.r#type.is_none() {
			tx.r#type = Some(TYPE_EIP1559.into());
		}

		// Convert the value to the native balance type.
		let evm_value = tx.value.unwrap_or_default();
		let native_value = match Self::convert_evm_to_native(evm_value, ConversionPrecision::Exact)
		{
			Ok(v) => v,
			Err(_) => return Err(EthTransactError::Message("Failed to convert value".into())),
		};

		let input = tx.input.clone().to_vec();

		let extract_error = |err| {
			if err == Error::<T>::TransferFailed.into() ||
				err == Error::<T>::StorageDepositNotEnoughFunds.into() ||
				err == Error::<T>::StorageDepositLimitExhausted.into()
			{
				let balance = Self::evm_balance(&from);
				return Err(EthTransactError::Message(
						format!("insufficient funds for gas * price + value: address {from:?} have {balance} (supplied gas {})",
							tx.gas.unwrap_or_default()))
					);
			}

			return Err(EthTransactError::Message(format!(
				"Failed to instantiate contract: {err:?}"
			)));
		};

		// Dry run the call
		let (mut result, dispatch_info) = match tx.to {
			// A contract call.
			Some(dest) => {
				// Dry run the call.
				let result = crate::Pallet::<T>::bare_call(
					T::RuntimeOrigin::signed(origin),
					dest,
					native_value,
					gas_limit,
					storage_deposit_limit,
					input.clone(),
				);

				let data = match result.result {
					Ok(return_value) => {
						if return_value.did_revert() {
							return Err(EthTransactError::Data(return_value.data));
						}
						return_value.data
					},
					Err(err) => {
						log::debug!(target: LOG_TARGET, "Failed to execute call: {err:?}");
						return extract_error(err)
					},
				};

				let result = EthTransactInfo {
					gas_required: result.gas_required,
					storage_deposit: result.storage_deposit.charge_or_zero(),
					data,
					eth_gas: Default::default(),
				};

				let (gas_limit, storage_deposit_limit) = T::EthGasEncoder::as_encoded_values(
					result.gas_required,
					result.storage_deposit,
				);
				let dispatch_call: <T as Config>::RuntimeCall = crate::Call::<T>::call {
					dest,
					value: native_value,
					gas_limit,
					storage_deposit_limit,
					data: input.clone(),
				}
				.into();
				(result, dispatch_call.get_dispatch_info())
			},
			// A contract deployment
			None => {
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
					native_value,
					gas_limit,
					storage_deposit_limit,
					Code::Upload(code.to_vec()),
					data.to_vec(),
					None,
				);

				let returned_data = match result.result {
					Ok(return_value) => {
						if return_value.result.did_revert() {
							return Err(EthTransactError::Data(return_value.result.data));
						}
						return_value.result.data
					},
					Err(err) => {
						log::debug!(target: LOG_TARGET, "Failed to instantiate: {err:?}");
						return extract_error(err)
					},
				};

				let result = EthTransactInfo {
					gas_required: result.gas_required,
					storage_deposit: result.storage_deposit.charge_or_zero(),
					data: returned_data,
					eth_gas: Default::default(),
				};

				// Get the dispatch info of the call.
				let (gas_limit, storage_deposit_limit) = T::EthGasEncoder::as_encoded_values(
					result.gas_required,
					result.storage_deposit,
				);
				let dispatch_call: <T as Config>::RuntimeCall =
					crate::Call::<T>::instantiate_with_code {
						value: native_value,
						gas_limit,
						storage_deposit_limit,
						code: code.to_vec(),
						data: data.to_vec(),
						salt: None,
					}
					.into();
				(result, dispatch_call.get_dispatch_info())
			},
		};

		let Ok(unsigned_tx) = tx.clone().try_into_unsigned() else {
			return Err(EthTransactError::Message("Invalid transaction".into()));
		};

		let eth_dispatch_call =
			crate::Call::<T>::eth_transact { payload: unsigned_tx.dummy_signed_payload() };
		let fee = tx_fee(eth_dispatch_call, dispatch_info);
		let raw_gas = Self::evm_fee_to_gas(fee);
		let eth_gas =
			T::EthGasEncoder::encode(raw_gas, result.gas_required, result.storage_deposit);

		log::trace!(target: LOG_TARGET, "bare_eth_call: raw_gas: {raw_gas:?} eth_gas: {eth_gas:?}");
		result.eth_gas = eth_gas;
		Ok(result)
	}

	/// Get the balance with EVM decimals of the given `address`.
	pub fn evm_balance(address: &H160) -> U256 {
		let account = T::AddressMapper::to_account_id(&address);
		Self::convert_native_to_evm(T::Currency::reducible_balance(&account, Preserve, Polite))
	}

	/// Convert a substrate fee into a gas value, using the fixed `GAS_PRICE`.
	/// The gas is calculated as `fee / GAS_PRICE`, rounded up to the nearest integer.
	pub fn evm_fee_to_gas(fee: BalanceOf<T>) -> U256 {
		let fee = Self::convert_native_to_evm(fee);
		let gas_price = GAS_PRICE.into();
		let (quotient, remainder) = fee.div_mod(gas_price);
		if remainder.is_zero() {
			quotient
		} else {
			quotient + U256::one()
		}
	}

	/// Convert a gas value into a substrate fee
	fn evm_gas_to_fee(gas: U256, gas_price: U256) -> Result<BalanceOf<T>, Error<T>> {
		let fee = gas.saturating_mul(gas_price);
		Self::convert_evm_to_native(fee, ConversionPrecision::RoundUp)
	}

	/// Convert a weight to a gas value.
	pub fn evm_gas_from_weight(weight: Weight) -> U256 {
		let fee = T::WeightPrice::convert(weight);
		Self::evm_fee_to_gas(fee)
	}

	/// Get the block gas limit.
	pub fn evm_block_gas_limit() -> U256 {
		let max_block_weight = T::BlockWeights::get()
			.get(DispatchClass::Normal)
			.max_total
			.unwrap_or_else(|| T::BlockWeights::get().max_block);

		Self::evm_gas_from_weight(max_block_weight)
	}

	/// Get the gas price.
	pub fn evm_gas_price() -> U256 {
		GAS_PRICE.into()
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
		let (module, deposit) = Self::try_upload_code(origin, code, storage_deposit_limit, false)?;
		Ok(CodeUploadReturnValue { code_hash: *module.code_hash(), deposit })
	}

	/// Query storage of a specified contract under a specified key.
	pub fn get_storage(address: H160, key: [u8; 32]) -> GetStorageResult {
		let contract_info =
			ContractInfoOf::<T>::get(&address).ok_or(ContractAccessError::DoesntExist)?;

		let maybe_value = contract_info.read(&Key::from_fixed(key));
		Ok(maybe_value)
	}

	/// Query storage of a specified contract under a specified variable-sized key.
	pub fn get_storage_var_key(address: H160, key: Vec<u8>) -> GetStorageResult {
		let contract_info =
			ContractInfoOf::<T>::get(&address).ok_or(ContractAccessError::DoesntExist)?;

		let maybe_value = contract_info.read(
			&Key::try_from_var(key)
				.map_err(|_| ContractAccessError::KeyDecodingFailed)?
				.into(),
		);
		Ok(maybe_value)
	}

	/// Uploads new code and returns the Wasm blob and deposit amount collected.
	fn try_upload_code(
		origin: T::AccountId,
		code: Vec<u8>,
		storage_deposit_limit: BalanceOf<T>,
		skip_transfer: bool,
	) -> Result<(WasmBlob<T>, BalanceOf<T>), DispatchError> {
		let mut module = WasmBlob::from_code(code, origin)?;
		let deposit = module.store_code(skip_transfer)?;
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

	/// Convert a native balance to EVM balance.
	fn convert_native_to_evm(value: BalanceOf<T>) -> U256 {
		value.into().saturating_mul(T::NativeToEthRatio::get().into())
	}

	/// Convert an EVM balance to a native balance.
	fn convert_evm_to_native(
		value: U256,
		precision: ConversionPrecision,
	) -> Result<BalanceOf<T>, Error<T>> {
		if value.is_zero() {
			return Ok(Zero::zero())
		}

		let (quotient, remainder) = value.div_mod(T::NativeToEthRatio::get().into());
		match (precision, remainder.is_zero()) {
			(ConversionPrecision::Exact, false) => Err(Error::<T>::DecimalPrecisionLoss),
			(_, true) => quotient.try_into().map_err(|_| Error::<T>::BalanceConversionFailed),
			(_, false) => quotient
				.saturating_add(U256::one())
				.try_into()
				.map_err(|_| Error::<T>::BalanceConversionFailed),
		}
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
	pub trait ReviveApi<AccountId, Balance, Nonce, BlockNumber> where
		AccountId: Codec,
		Balance: Codec,
		Nonce: Codec,
		BlockNumber: Codec,
	{
		/// Returns the block gas limit.
		fn block_gas_limit() -> U256;

		/// Returns the free balance of the given `[H160]` address, using EVM decimals.
		fn balance(address: H160) -> U256;

		/// Returns the gas price.
		fn gas_price() -> U256;

		/// Returns the nonce of the given `[H160]` address.
		fn nonce(address: H160) -> Nonce;

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
		) -> ContractResult<ExecReturnValue, Balance>;

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
		) -> ContractResult<InstantiateReturnValue, Balance>;


		/// Perform an Ethereum call.
		///
		/// See [`crate::Pallet::bare_eth_transact`]
		fn eth_transact(tx: GenericTransaction) -> Result<EthTransactInfo<Balance>, EthTransactError>;

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

		/// Query a given variable-sized storage key in a given contract.
		///
		/// Returns `Ok(Some(Vec<u8>))` if the storage value exists under the given key in the
		/// specified account and `Ok(None)` if it doesn't. If the account specified by the address
		/// doesn't exist, or doesn't have a contract then `Err` is returned.
		fn get_storage_var_key(
			address: H160,
			key: Vec<u8>,
		) -> GetStorageResult;

		/// Traces the execution of an entire block and returns call traces.
		///
		/// This is intended to be called through `state_call` to replay the block from the
		/// parent block.
		///
		/// See eth-rpc `debug_traceBlockByNumber` for usage.
		fn trace_block(
			block: Block,
			config: TracerConfig
		) -> Vec<(u32, CallTrace)>;

		/// Traces the execution of a specific transaction within a block.
		///
		/// This is intended to be called through `state_call` to replay the block from the
		/// parent hash up to the transaction.
		///
		/// See eth-rpc `debug_traceTransaction` for usage.
		fn trace_tx(
			block: Block,
			tx_index: u32,
			config: TracerConfig
		) -> Option<CallTrace>;

		/// Dry run and return the trace of the given call.
		///
		/// See eth-rpc `debug_traceCall` for usage.
		fn trace_call(tx: GenericTransaction, config: TracerConfig) -> Result<CallTrace, EthTransactError>;

	}
}
