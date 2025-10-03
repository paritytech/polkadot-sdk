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
mod call_builder;
mod exec;
mod gas;
mod impl_fungibles;
mod limits;
mod primitives;
mod storage;
#[cfg(test)]
mod tests;
mod transient_storage;
mod vm;

pub mod evm;
pub mod migrations;
pub mod precompiles;
pub mod test_utils;
pub mod tracing;
pub mod weights;

use crate::{
	evm::{
		runtime::GAS_PRICE, CallTracer, GasEncoder, GenericTransaction, PrestateTracer, Trace,
		Tracer, TracerType, TYPE_EIP1559,
	},
	exec::{AccountIdOf, ExecError, Executable, Stack as ExecStack},
	gas::GasMeter,
	storage::{meter::Meter as StorageMeter, AccountType, DeletionQueueManager},
	tracing::if_tracing,
	vm::{pvm::extract_code_and_data, CodeInfo, ContractBlob, RuntimeCosts},
};
use alloc::{boxed::Box, format, vec};
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
		ConstU32, ConstU64, EnsureOrigin, Get, IsType, OriginTrait, Time,
	},
	weights::WeightMeter,
	BoundedVec, RuntimeDebugNoBound,
};
use frame_system::{
	ensure_signed,
	pallet_prelude::{BlockNumberFor, OriginFor},
	Pallet as System,
};
use pallet_transaction_payment::OnChargeTransaction;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Bounded, Convert, Dispatchable, Saturating},
	AccountId32, DispatchError,
};

pub use crate::{
	address::{
		create1, create2, is_eth_derived, AccountId32Mapper, AddressMapper, TestAccountMapper,
	},
	exec::{Key, MomentOf, Origin},
	pallet::{genesis, *},
	storage::{AccountInfo, ContractInfo},
};
pub use codec;
pub use frame_support::{self, dispatch::DispatchInfo, weights::Weight};
pub use frame_system::{self, limits::BlockWeights};
pub use pallet_transaction_payment;
pub use primitives::*;
pub use sp_core::{H160, H256, U256};
pub use sp_runtime;
pub use weights::WeightInfo;

#[cfg(doc)]
pub use crate::vm::pvm::SyscallDoc;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
type TrieId = BoundedVec<u8, ConstU32<128>>;
type ImmutableData = BoundedVec<u8, ConstU32<{ limits::IMMUTABLE_BYTES }>>;
pub(crate) type OnChargeTransactionBalanceOf<T> = <<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<T>>::Balance;

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

		/// Used to answer contracts' queries regarding the current weight price. This is **not**
		/// used to calculate the actual fee and is only for informational purposes.
		#[pallet::no_default_bounds]
		type WeightPrice: Convert<Weight, BalanceOf<Self>>;

		/// Describes the weights of the dispatchables of this module and is also used to
		/// construct a default cost schedule.
		type WeightInfo: WeightInfo;

		/// Type that allows the runtime authors to add new host functions for a contract to call.
		///
		/// Pass in a tuple of types that implement [`precompiles::Precompile`].
		#[pallet::no_default_bounds]
		#[allow(private_bounds)]
		type Precompiles: precompiles::Precompiles<Self>;

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

		/// Allow EVM bytecode to be uploaded and instantiated.
		#[pallet::constant]
		type AllowEVMBytecode: Get<bool>;

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
				0u64
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
			type Precompiles = ();
			type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
			type DepositPerByte = DepositPerByte;
			type DepositPerItem = DepositPerItem;
			type Time = Self;
			type UnsafeUnstableInterface = ConstBool<true>;
			type AllowEVMBytecode = ConstBool<true>;
			type UploadOrigin = EnsureSigned<Self::AccountId>;
			type InstantiateOrigin = EnsureSigned<Self::AccountId>;
			type WeightInfo = ();
			type WeightPrice = Self;
			type RuntimeMemory = ConstU32<{ 128 * 1024 * 1024 }>;
			type PVFMemory = ConstU32<{ 512 * 1024 * 1024 }>;
			type ChainId = ConstU64<42>;
			type NativeToEthRatio = ConstU32<1_000_000>;
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

		/// Contract deployed by deployer at the specified address.
		Instantiated { deployer: H160, contract: H160 },
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
		/// Event body or storage item exceeds [`limits::PAYLOAD_BYTES`].
		ValueTooLarge = 0x0C,
		/// Termination of a contract is not allowed while the contract is already
		/// on the call stack. Can be triggered by `seal_terminate`.
		TerminatedWhileReentrant = 0x0D,
		/// `seal_call` forwarded this contracts input. It therefore is no longer available.
		InputForwarded = 0x0E,
		/// The amount of topics passed to `seal_deposit_events` exceeds the limit.
		TooManyTopics = 0x0F,
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
		/// The contract declares too much memory (ro + rw + stack).
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
		/// Unsupported precompile address.
		UnsupportedPrecompileAddress = 0x2F,
		/// The calldata exceeds [`limits::CALLDATA_BYTES`].
		CallDataTooLarge = 0x30,
		/// The return data exceeds [`limits::CALLDATA_BYTES`].
		ReturnDataTooLarge = 0x31,
		/// Invalid jump destination. Dynamic jumps points to invalid not jumpdest opcode.
		InvalidJump = 0x32,
		/// Attempting to pop a value from an empty stack.
		StackUnderflow = 0x33,
		/// Attempting to push a value onto a full stack.
		StackOverflow = 0x34,
	}

	/// A reason for the pallet revive placing a hold on funds.
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
	/// The code's size is bounded by [`crate::limits::BLOB_BYTES`] for PVM and
	/// [`revm::primitives::eip170::MAX_CODE_SIZE`] for EVM bytecode.
	#[pallet::storage]
	#[pallet::unbounded]
	pub(crate) type PristineCode<T: Config> = StorageMap<_, Identity, H256, Vec<u8>>;

	/// A mapping from a contract's code hash to its code info.
	#[pallet::storage]
	pub(crate) type CodeInfoOf<T: Config> = StorageMap<_, Identity, H256, CodeInfo<T>>;

	/// The data associated to a contract or externally owned account.
	#[pallet::storage]
	pub(crate) type AccountInfoOf<T: Config> = StorageMap<_, Identity, H160, AccountInfo<T>>;

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

	pub mod genesis {
		use super::*;
		use crate::evm::Bytes32;

		/// Genesis configuration for contract-specific data.
		#[derive(Clone, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
		pub struct ContractData {
			/// Contract code.
			pub code: Vec<u8>,
			/// Initial storage entries as 32-byte key/value pairs.
			pub storage: alloc::collections::BTreeMap<Bytes32, Bytes32>,
		}

		/// Genesis configuration for a contract account.
		#[derive(PartialEq, Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
		pub struct Account<T: Config> {
			/// Contract address.
			pub address: H160,
			/// Contract balance.
			#[serde(default)]
			pub balance: U256,
			/// Account nonce
			#[serde(default)]
			pub nonce: T::Nonce,
			/// Contract-specific data (code and storage). None for EOAs.
			#[serde(flatten, skip_serializing_if = "Option::is_none")]
			pub contract_data: Option<ContractData>,
		}
	}

	#[pallet::genesis_config]
	#[derive(Debug, PartialEq, frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// List of native Substrate accounts (typically `AccountId32`) to be mapped at genesis
		/// block, enabling them to interact with smart contracts.
		#[serde(default, skip_serializing_if = "Vec::is_empty")]
		pub mapped_accounts: Vec<T::AccountId>,

		/// Account entries (both EOAs and contracts)
		#[serde(default, skip_serializing_if = "Vec::is_empty")]
		pub accounts: Vec<genesis::Account<T>>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T>
	where
		BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
		MomentOf<T>: Into<U256>,
		T::Hash: frame_support::traits::IsType<H256>,
	{
		fn build(&self) {
			use crate::{exec::Key, vm::ContractBlob};
			use frame_support::traits::fungible::Mutate;

			if !System::<T>::account_exists(&Pallet::<T>::account_id()) {
				let _ = T::Currency::mint_into(
					&Pallet::<T>::account_id(),
					T::Currency::minimum_balance(),
				);
			}

			for id in &self.mapped_accounts {
				if let Err(err) = T::AddressMapper::map_no_deposit(id) {
					log::error!(target: LOG_TARGET, "Failed to map account {id:?}: {err:?}");
				}
			}

			let owner = Pallet::<T>::account_id();

			for genesis::Account { address, balance, nonce, contract_data } in &self.accounts {
				let account_id = T::AddressMapper::to_account_id(address);

				frame_system::Account::<T>::mutate(&account_id, |info| {
					info.nonce = (*nonce).into();
				});

				match contract_data {
					None => {
						AccountInfoOf::<T>::insert(
							address,
							AccountInfo { account_type: AccountType::EOA, dust: 0 },
						);
					},
					Some(genesis::ContractData { code, storage }) => {
						let blob = if code.starts_with(&polkavm_common::program::BLOB_MAGIC) {
							ContractBlob::<T>::from_pvm_code(   code.clone(), owner.clone()).inspect_err(|err| {
								log::error!(target: LOG_TARGET, "Failed to create PVM ContractBlob for {address:?}: {err:?}");
							})
						} else {
							ContractBlob::<T>::from_evm_runtime_code(code.clone(), account_id).inspect_err(|err| {
								log::error!(target: LOG_TARGET, "Failed to create EVM ContractBlob for {address:?}: {err:?}");
							})
						};

						let Ok(blob) = blob else {
							continue;
						};

						let code_hash = *blob.code_hash();
						let Ok(info) = <ContractInfo<T>>::new(&address, 0u32.into(), code_hash)
							.inspect_err(|err| {
								log::error!(target: LOG_TARGET, "Failed to create ContractInfo for {address:?}: {err:?}");
							})
						else {
							continue;
						};

						AccountInfoOf::<T>::insert(
							address,
							AccountInfo { account_type: info.clone().into(), dust: 0 },
						);

						<PristineCode<T>>::insert(blob.code_hash(), code);
						<CodeInfoOf<T>>::insert(blob.code_hash(), blob.code_info().clone());
						for (k, v) in storage {
							let _ = info.write(&Key::from_fixed(k.0), Some(v.0.to_vec()), None, false).inspect_err(|err| {
								log::error!(target: LOG_TARGET, "Failed to write genesis storage for {address:?} at key {k:?}: {err:?}");
							});
						}
					},
				}

				let _ = Pallet::<T>::set_evm_balance(address, *balance).inspect_err(|err| {
					log::error!(target: LOG_TARGET, "Failed to set EVM balance for {address:?}: {err:?}");
				});
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_block: BlockNumberFor<T>) -> Weight {
			// Warm up the pallet account.
			System::<T>::account_exists(&Pallet::<T>::account_id());
			return T::DbWeight::get().reads(1)
		}
		fn on_idle(_block: BlockNumberFor<T>, limit: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(limit);
			ContractInfo::<T>::process_deletion_queue_batch(&mut meter);
			meter.consumed()
		}

		fn integrity_test() {
			assert!(T::ChainId::get() > 0, "ChainId must be greater than 0");

			// The memory available in the block building runtime
			let max_runtime_mem: u32 = T::RuntimeMemory::get();

			// We only allow 50% of the runtime memory to be utilized by the contracts call
			// stack, keeping the rest for other facilities, such as PoV, etc.
			const TOTAL_MEMORY_DEVIDER: u32 = 2;

			// Check that the configured memory limits fit into runtime memory.
			//
			// Dynamic allocations are not available, yet. Hence they are not taken into
			// consideration here.
			let memory_left = i64::from(max_runtime_mem)
				.saturating_div(TOTAL_MEMORY_DEVIDER.into())
				.saturating_sub(limits::MEMORY_REQUIRED.into());

			log::debug!(target: LOG_TARGET, "Integrity check: memory_left={} KB", memory_left / 1024);

			assert!(
				memory_left >= 0,
				"Runtime does not have enough memory for current limits. Additional runtime memory required: {} KB",
				memory_left.saturating_mul(TOTAL_MEMORY_DEVIDER.into()).abs() / 1024
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
				Pallet::<T>::convert_native_to_evm(value),
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

		/// Instantiates a contract from a previously deployed vm binary.
		///
		/// This function is identical to [`Self::instantiate_with_code`] but without the
		/// code deployment step. Instead, the `code_hash` of an on-chain deployed vm binary
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
				Pallet::<T>::convert_native_to_evm(value),
				gas_limit,
				DepositLimit::Balance(storage_deposit_limit),
				Code::Existing(code_hash),
				data,
				salt,
				BumpNonce::Yes,
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
				Pallet::<T>::convert_native_to_evm(value),
				gas_limit,
				DepositLimit::Balance(storage_deposit_limit),
				Code::Upload(code),
				data,
				salt,
				BumpNonce::Yes,
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

		/// Same as [`Self::instantiate_with_code`], but intended to be dispatched **only**
		/// by an EVM transaction through the EVM compatibility layer.
		///
		/// Calling this dispatchable ensures that the origin's nonce is bumped only once,
		/// via the `CheckNonce` transaction extension. In contrast, [`Self::instantiate_with_code`]
		/// also bumps the nonce after contract instantiation, since it may be invoked multiple
		/// times within a batch call transaction.
		#[pallet::call_index(10)]
		#[pallet::weight(
			T::WeightInfo::eth_instantiate_with_code(code.len() as u32, data.len() as u32, Pallet::<T>::has_dust(*value).into())
			.saturating_add(*gas_limit)
		)]
		pub fn eth_instantiate_with_code(
			origin: OriginFor<T>,
			value: U256,
			gas_limit: Weight,
			#[pallet::compact] storage_deposit_limit: BalanceOf<T>,
			code: Vec<u8>,
			data: Vec<u8>,
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
				None,
				BumpNonce::No,
			);

			if let Ok(retval) = &output.result {
				if retval.result.did_revert() {
					output.result = Err(<Error<T>>::ContractReverted.into());
				}
			}
			dispatch_result(
				output.result.map(|result| result.result),
				output.gas_consumed,
				T::WeightInfo::eth_instantiate_with_code(
					code_len,
					data_len,
					Pallet::<T>::has_dust(value).into(),
				),
			)
		}

		/// Same as [`Self::call`], but intended to be dispatched **only**
		/// by an EVM transaction through the EVM compatibility layer.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::eth_call(Pallet::<T>::has_dust(*value).into()).saturating_add(*gas_limit))]
		pub fn eth_call(
			origin: OriginFor<T>,
			dest: H160,
			value: U256,
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
			dispatch_result(
				output.result,
				output.gas_consumed,
				T::WeightInfo::eth_call(Pallet::<T>::has_dust(value).into()),
			)
		}

		/// Upload new `code` without instantiating a contract from it.
		///
		/// If the code does not already exist a deposit is reserved from the caller
		/// The size of the reserve depends on the size of the supplied `code`.
		///
		/// # Note
		///
		/// Anyone can instantiate a contract from any uploaded code and thus prevent its removal.
		/// To avoid this situation a constructor could employ access control so that it can
		/// only be instantiated by permissioned entities. The same is true when uploading
		/// through [`Self::instantiate_with_code`].
		///
		/// If the refcount of the code reaches zero after terminating the last contract that
		/// references this code, the code will be removed automatically.
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
			<ContractBlob<T>>::remove(&origin, code_hash)?;
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
			<AccountInfoOf<T>>::try_mutate(&dest, |account| {
				let Some(account) = account else {
					return Err(<Error<T>>::ContractNotFound.into());
				};

				let AccountType::Contract(ref mut contract) = account.account_type else {
					return Err(<Error<T>>::ContractNotFound.into());
				};

				<CodeInfo<T>>::increment_refcount(code_hash)?;
				let _ = <CodeInfo<T>>::decrement_refcount(contract.code_hash)?;
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
		evm_value: U256,
		gas_limit: Weight,
		storage_deposit_limit: DepositLimit<BalanceOf<T>>,
		data: Vec<u8>,
	) -> ContractResult<ExecReturnValue, BalanceOf<T>> {
		if let Err(contract_result) = Self::ensure_non_contract_if_signed(&origin) {
			return contract_result;
		}
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();

		let try_call = || {
			let origin = Origin::from_runtime_origin(origin)?;
			let mut storage_meter = StorageMeter::new(storage_deposit_limit.limit());
			let result = ExecStack::<T, ContractBlob<T>>::run_call(
				origin.clone(),
				dest,
				&mut gas_meter,
				&mut storage_meter,
				evm_value,
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

	/// Prepare a dry run for the given account.
	///
	///
	/// This function is public because it is called by the runtime API implementation
	/// (see `impl_runtime_apis_plus_revive`).
	pub fn prepare_dry_run(account: &T::AccountId) {
		// Bump the  nonce to simulate what would happen
		// `pre-dispatch` if the transaction was executed.
		frame_system::Pallet::<T>::inc_account_nonce(account);
	}

	/// A generalized version of [`Self::instantiate`] or [`Self::instantiate_with_code`].
	///
	/// Identical to [`Self::instantiate`] or [`Self::instantiate_with_code`] but tailored towards
	/// being called by other code within the runtime as opposed to from an extrinsic. It returns
	/// more information to the caller useful to estimate the cost of the operation.
	pub fn bare_instantiate(
		origin: OriginFor<T>,
		evm_value: U256,
		gas_limit: Weight,
		storage_deposit_limit: DepositLimit<BalanceOf<T>>,
		code: Code,
		data: Vec<u8>,
		salt: Option<[u8; 32]>,
		bump_nonce: BumpNonce,
	) -> ContractResult<InstantiateReturnValue, BalanceOf<T>> {
		// Enforce EIP-3607 for top-level signed origins: deny signed contract addresses.
		if let Err(contract_result) = Self::ensure_non_contract_if_signed(&origin) {
			return contract_result;
		}
		let mut gas_meter = GasMeter::new(gas_limit);
		let mut storage_deposit = Default::default();
		let unchecked_deposit_limit = storage_deposit_limit.is_unchecked();
		let mut storage_deposit_limit = storage_deposit_limit.limit();
		let try_instantiate = || {
			let instantiate_account = T::InstantiateOrigin::ensure_origin(origin.clone())?;

			if_tracing(|t| t.instantiate_code(&code, salt.as_ref()));
			let (executable, upload_deposit) = match code {
				Code::Upload(code) if code.starts_with(&polkavm_common::program::BLOB_MAGIC) => {
					let upload_account = T::UploadOrigin::ensure_origin(origin)?;
					let (executable, upload_deposit) = Self::try_upload_pvm_code(
						upload_account,
						code,
						storage_deposit_limit,
						unchecked_deposit_limit,
					)?;
					storage_deposit_limit.saturating_reduce(upload_deposit);
					(executable, upload_deposit)
				},
				Code::Upload(code) =>
					if T::AllowEVMBytecode::get() {
						let origin = T::UploadOrigin::ensure_origin(origin)?;
						let executable = ContractBlob::from_evm_init_code(code, origin)?;
						(executable, Default::default())
					} else {
						return Err(<Error<T>>::CodeRejected.into())
					},
				Code::Existing(code_hash) =>
					(ContractBlob::from_storage(code_hash, &mut gas_meter)?, Default::default()),
			};
			let instantiate_origin = Origin::from_account_id(instantiate_account.clone());
			let mut storage_meter = StorageMeter::new(storage_deposit_limit);
			let result = ExecStack::<T, ContractBlob<T>>::run_instantiate(
				instantiate_account,
				executable,
				&mut gas_meter,
				&mut storage_meter,
				evm_value,
				data,
				salt.as_ref(),
				unchecked_deposit_limit,
				bump_nonce,
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

	/// Dry-run Ethereum calls.
	///
	/// # Parameters
	///
	/// - `tx`: The Ethereum transaction to simulate.
	/// - `gas_limit`: The gas limit enforced during contract execution.
	/// - `tx_fee`: A function that returns the fee for the computed eth_transact and actual
	/// dispatched call
	pub fn dry_run_eth_transact(
		mut tx: GenericTransaction,
		gas_limit: Weight,
		tx_fee: impl Fn(<T as Config>::RuntimeCall, <T as Config>::RuntimeCall) -> BalanceOf<T>,
	) -> Result<EthTransactInfo<BalanceOf<T>>, EthTransactError>
	where
		<T as frame_system::Config>::RuntimeCall:
			Dispatchable<Info = frame_support::dispatch::DispatchInfo>,
		T: pallet_transaction_payment::Config,
		OnChargeTransactionBalanceOf<T>: Into<BalanceOf<T>>,
		<T as Config>::RuntimeCall: From<crate::Call<T>>,
		<T as Config>::RuntimeCall: Encode,
		T::Nonce: Into<U256>,
		T::Hash: frame_support::traits::IsType<H256>,
	{
		log::trace!(target: LOG_TARGET, "dry_run_eth_transact: {tx:?} gas_limit: {gas_limit:?}");

		let from = tx.from.unwrap_or_default();
		let origin = T::AddressMapper::to_account_id(&from);
		Self::prepare_dry_run(&origin);

		let storage_deposit_limit = if tx.gas.is_some() {
			DepositLimit::Balance(BalanceOf::<T>::max_value())
		} else {
			DepositLimit::UnsafeOnlyForDryRun
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
		let value = tx.value.unwrap_or_default();
		let input = tx.input.clone().to_vec();

		let extract_error = |err| {
			if err == Error::<T>::TransferFailed.into() ||
				err == Error::<T>::StorageDepositNotEnoughFunds.into() ||
				err == Error::<T>::StorageDepositLimitExhausted.into()
			{
				let balance = Self::evm_balance(&from);
				return Err(EthTransactError::Message(format!(
					"insufficient funds for gas * price + value: address {from:?} have {balance} (supplied gas {})",
					tx.gas.unwrap_or_default()
				)));
			}

			return Err(EthTransactError::Message(format!(
				"Failed to instantiate contract: {err:?}"
			)));
		};

		// Dry run the call
		let (mut result, dispatch_call) = match tx.to {
			// A contract call.
			Some(dest) => {
				if dest == RUNTIME_PALLETS_ADDR {
					let Ok(dispatch_call) = <T as Config>::RuntimeCall::decode(&mut &input[..])
					else {
						return Err(EthTransactError::Message(format!(
							"Failed to decode pallet-call {input:?}"
						)));
					};

					if let Err(err) =
						dispatch_call.clone().dispatch(RawOrigin::Signed(origin).into())
					{
						return Err(EthTransactError::Message(format!(
							"Failed to dispatch call: {err:?}"
						)));
					};

					let result = EthTransactInfo {
						gas_required: dispatch_call.get_dispatch_info().total_weight(),
						..Default::default()
					};

					(result, dispatch_call)
				} else {
					// Dry run the call.
					let result = crate::Pallet::<T>::bare_call(
						T::RuntimeOrigin::signed(origin),
						dest,
						value,
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
							return extract_error(err);
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
					let dispatch_call: <T as Config>::RuntimeCall = crate::Call::<T>::eth_call {
						dest,
						value,
						gas_limit,
						storage_deposit_limit,
						data: input.clone(),
					}
					.into();
					(result, dispatch_call)
				}
			},
			// A contract deployment
			None => {
				// Extract code and data from the input.
				let (code, data) = if input.starts_with(&polkavm_common::program::BLOB_MAGIC) {
					extract_code_and_data(&input).unwrap_or_else(|| (input, Default::default()))
				} else {
					(input, vec![])
				};

				// Dry run the call.
				let result = crate::Pallet::<T>::bare_instantiate(
					T::RuntimeOrigin::signed(origin),
					value,
					gas_limit,
					storage_deposit_limit,
					Code::Upload(code.clone()),
					data.clone(),
					None,
					BumpNonce::No,
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
						return extract_error(err);
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
					crate::Call::<T>::eth_instantiate_with_code {
						value,
						gas_limit,
						storage_deposit_limit,
						code,
						data,
					}
					.into();
				(result, dispatch_call)
			},
		};

		let Ok(unsigned_tx) = tx.clone().try_into_unsigned() else {
			return Err(EthTransactError::Message("Invalid transaction".into()));
		};

		let eth_transact_call =
			crate::Call::<T>::eth_transact { payload: unsigned_tx.dummy_signed_payload() };
		let fee = tx_fee(eth_transact_call.into(), dispatch_call);
		let raw_gas = Self::evm_fee_to_gas(fee);
		let eth_gas =
			T::EthGasEncoder::encode(raw_gas, result.gas_required, result.storage_deposit);

		log::trace!(target: LOG_TARGET, "bare_eth_call: raw_gas: {raw_gas:?} eth_gas: {eth_gas:?}");
		result.eth_gas = eth_gas;
		Ok(result)
	}

	/// Get the balance with EVM decimals of the given `address`.
	///
	/// Returns the spendable balance excluding the existential deposit.
	pub fn evm_balance(address: &H160) -> U256 {
		let balance = AccountInfo::<T>::balance((*address).into());
		Self::convert_native_to_evm(balance)
	}

	/// Set the EVM balance of an account.
	///
	/// The account's total balance becomes the EVM value plus the existential deposit,
	/// consistent with `evm_balance` which returns the spendable balance excluding the existential
	/// deposit.
	pub fn set_evm_balance(address: &H160, evm_value: U256) -> Result<(), Error<T>> {
		let (balance, dust) = Self::new_balance_with_dust(evm_value)
			.map_err(|_| <Error<T>>::BalanceConversionFailed)?;
		let account_id = T::AddressMapper::to_account_id(&address);
		T::Currency::set_balance(&account_id, balance);
		AccountInfoOf::<T>::mutate(address, |account| {
			account.as_mut().map(|a| a.dust = dust);
		});

		Ok(())
	}

	/// Construct native balance from EVM balance.
	///
	/// Adds the existential deposit and returns the native balance plus the dust.
	pub fn new_balance_with_dust(
		evm_value: U256,
	) -> Result<(BalanceOf<T>, u32), BalanceConversionError> {
		let ed = T::Currency::minimum_balance();
		let balance_with_dust = BalanceWithDust::<BalanceOf<T>>::from_value::<T>(evm_value)?;
		let (value, dust) = balance_with_dust.deconstruct();

		Ok((ed.saturating_add(value), dust))
	}

	/// Get the nonce for the given `address`.
	pub fn evm_nonce(address: &H160) -> u32
	where
		T::Nonce: Into<u32>,
	{
		let account = T::AddressMapper::to_account_id(&address);
		System::<T>::account_nonce(account).into()
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
		let value = BalanceWithDust::<BalanceOf<T>>::from_value::<T>(fee)
			.map_err(|_| <Error<T>>::BalanceConversionFailed)?;
		Ok(value.into_rounded_balance())
	}

	/// Convert a weight to a gas value.
	pub fn evm_gas_from_weight(weight: Weight) -> U256 {
		let fee = T::WeightPrice::convert(weight);
		Self::evm_fee_to_gas(fee)
	}

	/// Get the block gas limit.
	pub fn evm_block_gas_limit() -> U256
	where
		<T as frame_system::Config>::RuntimeCall:
			Dispatchable<Info = frame_support::dispatch::DispatchInfo>,
		T: pallet_transaction_payment::Config,
		OnChargeTransactionBalanceOf<T>: Into<BalanceOf<T>>,
	{
		let max_block_weight = T::BlockWeights::get()
			.get(DispatchClass::Normal)
			.max_total
			.unwrap_or_else(|| T::BlockWeights::get().max_block);

		let length_fee = pallet_transaction_payment::Pallet::<T>::length_to_fee(
			5 * 1024 * 1024, // 5 MB
		);

		Self::evm_gas_from_weight(max_block_weight)
			.saturating_add(Self::evm_fee_to_gas(length_fee.into()))
	}

	/// Get the gas price.
	pub fn evm_gas_price() -> U256 {
		GAS_PRICE.into()
	}

	/// Build an EVM tracer from the given tracer type.
	pub fn evm_tracer(tracer_type: TracerType) -> Tracer<T>
	where
		T::Nonce: Into<u32>,
	{
		match tracer_type {
			TracerType::CallTracer(config) => CallTracer::new(
				config.unwrap_or_default(),
				Self::evm_gas_from_weight as fn(Weight) -> U256,
			)
			.into(),
			TracerType::PrestateTracer(config) =>
				PrestateTracer::new(config.unwrap_or_default()).into(),
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
		let (module, deposit) =
			Self::try_upload_pvm_code(origin, code, storage_deposit_limit, false)?;
		Ok(CodeUploadReturnValue { code_hash: *module.code_hash(), deposit })
	}

	/// Query storage of a specified contract under a specified key.
	pub fn get_storage(address: H160, key: [u8; 32]) -> GetStorageResult {
		let contract_info =
			AccountInfo::<T>::load_contract(&address).ok_or(ContractAccessError::DoesntExist)?;

		let maybe_value = contract_info.read(&Key::from_fixed(key));
		Ok(maybe_value)
	}

	/// Get the immutable data of a specified contract.
	///
	/// Returns `None` if the contract does not exist or has no immutable data.
	pub fn get_immutables(address: H160) -> Option<ImmutableData> {
		let immutable_data = <ImmutableDataOf<T>>::get(address);
		immutable_data
	}

	/// Sets immutable data of a contract
	///
	/// Returns an error if the contract does not exist.
	///
	/// # Warning
	///
	/// Does not collect any storage deposit. Not safe to be called by user controlled code.
	pub fn set_immutables(address: H160, data: ImmutableData) -> Result<(), ContractAccessError> {
		AccountInfo::<T>::load_contract(&address).ok_or(ContractAccessError::DoesntExist)?;
		<ImmutableDataOf<T>>::insert(address, data);
		Ok(())
	}

	/// Query storage of a specified contract under a specified variable-sized key.
	pub fn get_storage_var_key(address: H160, key: Vec<u8>) -> GetStorageResult {
		let contract_info =
			AccountInfo::<T>::load_contract(&address).ok_or(ContractAccessError::DoesntExist)?;

		let maybe_value = contract_info.read(
			&Key::try_from_var(key)
				.map_err(|_| ContractAccessError::KeyDecodingFailed)?
				.into(),
		);
		Ok(maybe_value)
	}

	/// Set storage of a specified contract under a specified key.
	///
	/// If the `value` is `None`, the storage entry is deleted.
	///
	/// Returns an error if the contract does not exist or if the write operation fails.
	///
	/// # Warning
	///
	/// Does not collect any storage deposit. Not safe to be called by user controlled code.
	pub fn set_storage(address: H160, key: [u8; 32], value: Option<Vec<u8>>) -> SetStorageResult {
		let contract_info =
			AccountInfo::<T>::load_contract(&address).ok_or(ContractAccessError::DoesntExist)?;

		contract_info
			.write(&Key::from_fixed(key), value, None, false)
			.map_err(ContractAccessError::StorageWriteFailed)
	}

	/// Set the storage of a specified contract under a specified variable-sized key.
	///
	/// If the `value` is `None`, the storage entry is deleted.
	///
	/// Returns an error if the contract does not exist, if the key decoding fails,
	/// or if the write operation fails.
	///
	/// # Warning
	///
	/// Does not collect any storage deposit. Not safe to be called by user controlled code.
	pub fn set_storage_var_key(
		address: H160,
		key: Vec<u8>,
		value: Option<Vec<u8>>,
	) -> SetStorageResult {
		let contract_info =
			AccountInfo::<T>::load_contract(&address).ok_or(ContractAccessError::DoesntExist)?;

		contract_info
			.write(
				&Key::try_from_var(key)
					.map_err(|_| ContractAccessError::KeyDecodingFailed)?
					.into(),
				value,
				None,
				false,
			)
			.map_err(ContractAccessError::StorageWriteFailed)
	}

	/// Uploads new code and returns the Vm binary contract blob and deposit amount collected.
	fn try_upload_pvm_code(
		origin: T::AccountId,
		code: Vec<u8>,
		storage_deposit_limit: BalanceOf<T>,
		skip_transfer: bool,
	) -> Result<(ContractBlob<T>, BalanceOf<T>), DispatchError> {
		let mut module = ContractBlob::from_pvm_code(code, origin)?;
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
	pub fn convert_native_to_evm(value: impl Into<BalanceWithDust<BalanceOf<T>>>) -> U256 {
		let (value, dust) = value.into().deconstruct();
		value
			.into()
			.saturating_mul(T::NativeToEthRatio::get().into())
			.saturating_add(dust.into())
	}

	/// Ensure the origin has no code deplyoyed if it is a signed origin.
	fn ensure_non_contract_if_signed<ReturnValue>(
		origin: &OriginFor<T>,
	) -> Result<(), ContractResult<ReturnValue, BalanceOf<T>>> {
		use crate::exec::is_precompile;
		let Ok(who) = ensure_signed(origin.clone()) else { return Ok(()) };
		let address = <T::AddressMapper as AddressMapper<T>>::to_address(&who);

		// EIP_1052: precompile can never be used as EOA.
		if is_precompile::<T, ContractBlob<T>>(&address) {
			log::debug!(
				target: crate::LOG_TARGET,
				"EIP-3607: reject externally-signed tx from precompile account {:?}",
				address
			);
			return Err(ContractResult {
				result: Err(DispatchError::BadOrigin),
				gas_consumed: Weight::default(),
				gas_required: Weight::default(),
				storage_deposit: Default::default(),
			});
		}

		// Deployed code exists when hash is neither zero (no account) nor EMPTY_CODE_HASH
		// (account exists but no code).
		if <AccountInfo<T>>::is_contract(&address) {
			log::debug!(
				target: crate::LOG_TARGET,
				"EIP-3607: reject externally-signed tx from contract account {:?}",
				address
			);
			return Err(ContractResult {
				result: Err(DispatchError::BadOrigin),
				gas_consumed: Weight::default(),
				gas_required: Weight::default(),
				storage_deposit: Default::default(),
			});
		}
		Ok(())
	}
}

impl<T: Config> Pallet<T> {
	/// Pallet account, used to hold funds for contracts upload deposit.
	pub fn account_id() -> T::AccountId {
		use frame_support::PalletId;
		use sp_runtime::traits::AccountIdConversion;
		PalletId(*b"py/reviv").into_account_truncating()
	}

	/// Returns true if the evm value carries dust.
	fn has_dust(value: U256) -> bool {
		value % U256::from(<T>::NativeToEthRatio::get()) != U256::zero()
	}

	/// Returns true if the evm value carries balance.
	fn has_balance(value: U256) -> bool {
		value >= U256::from(<T>::NativeToEthRatio::get())
	}

	/// Return the existential deposit of [`Config::Currency`].
	fn min_balance() -> BalanceOf<T> {
		<T::Currency as Inspect<AccountIdOf<T>>>::minimum_balance()
	}

	/// Deposit a pallet revive event.
	fn deposit_event(event: Event<T>) {
		<frame_system::Pallet<T>>::deposit_event(<T as Config>::RuntimeEvent::from(event))
	}

	/// The address of the validator that produced the current block.
	pub fn block_author() -> Option<H160> {
		use frame_support::traits::FindAuthor;

		let digest = <frame_system::Pallet<T>>::digest();
		let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());

		let account_id = T::FindAuthor::find_author(pre_runtime_digests)?;
		Some(T::AddressMapper::to_address(&account_id))
	}

	/// Returns the code at `address`.
	///
	/// This takes pre-compiles into account.
	pub fn code(address: &H160) -> Vec<u8> {
		use precompiles::{All, Precompiles};
		if let Some(code) = <All<T>>::code(address.as_fixed_bytes()) {
			return code.into()
		}
		AccountInfo::<T>::load_contract(&address)
			.and_then(|contract| <PristineCode<T>>::get(contract.code_hash))
			.map(|code| code.into())
			.unwrap_or_default()
	}
}

/// The address used to call the runtime's pallets dispatchables
///
/// Note:
/// computed with PalletId(*b"py/paddr").into_account_truncating();
pub const RUNTIME_PALLETS_ADDR: H160 =
	H160(hex_literal::hex!("6d6f646c70792f70616464720000000000000000"));

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
		/// See [`crate::Pallet::dry_run_eth_transact`]
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
			config: TracerType
		) -> Vec<(u32, Trace)>;

		/// Traces the execution of a specific transaction within a block.
		///
		/// This is intended to be called through `state_call` to replay the block from the
		/// parent hash up to the transaction.
		///
		/// See eth-rpc `debug_traceTransaction` for usage.
		fn trace_tx(
			block: Block,
			tx_index: u32,
			config: TracerType
		) -> Option<Trace>;

		/// Dry run and return the trace of the given call.
		///
		/// See eth-rpc `debug_traceCall` for usage.
		fn trace_call(tx: GenericTransaction, config: TracerType) -> Result<Trace, EthTransactError>;

		/// The address of the validator that produced the current block.
		fn block_author() -> Option<H160>;

		/// Get the H160 address associated to this account id
		fn address(account_id: AccountId) -> H160;

		/// Get the account id associated to this H160 address.
		fn account_id(address: H160) -> AccountId;

		/// The address used to call the runtime's pallets dispatchables
		fn runtime_pallets_address() -> H160;

		/// The code at the specified address taking pre-compiles into account.
		fn code(address: H160) -> Vec<u8>;

		/// Construct the new balance and dust components of this EVM balance.
		fn new_balance_with_dust(balance: U256) -> Result<(Balance, u32), BalanceConversionError>;
	}
}

/// This macro wraps substrate's `impl_runtime_apis!` and implements `pallet_revive` runtime APIs.
///
/// # Parameters
/// - `$Runtime`: The runtime type to implement the APIs for.
/// - `$Executive`: The Executive type of the runtime.
/// - `$EthExtra`: Type for additional Ethereum runtime extension.
/// - `$($rest:tt)*`: Remaining input to be forwarded to the underlying `impl_runtime_apis!`.
#[macro_export]
macro_rules! impl_runtime_apis_plus_revive {
	($Runtime: ty, $Executive: ty, $EthExtra: ty, $($rest:tt)*) => {

		impl_runtime_apis! {
			$($rest)*

			impl pallet_revive::ReviveApi<Block, AccountId, Balance, Nonce, BlockNumber> for $Runtime {
				fn balance(address: $crate::H160) -> $crate::U256 {
					$crate::Pallet::<Self>::evm_balance(&address)
				}

				fn block_author() -> Option<$crate::H160> {
					$crate::Pallet::<Self>::block_author()
				}

				fn block_gas_limit() -> $crate::U256 {
					$crate::Pallet::<Self>::evm_block_gas_limit()
				}

				fn gas_price() -> $crate::U256 {
					$crate::Pallet::<Self>::evm_gas_price()
				}

				fn nonce(address: $crate::H160) -> Nonce {
					use $crate::AddressMapper;
					let account = <Self as $crate::Config>::AddressMapper::to_account_id(&address);
					$crate::frame_system::Pallet::<Self>::account_nonce(account)
				}

				fn address(account_id: AccountId) -> $crate::H160 {
					use $crate::AddressMapper;
					<Self as $crate::Config>::AddressMapper::to_address(&account_id)
				}

				fn eth_transact(
					tx: $crate::evm::GenericTransaction,
				) -> Result<$crate::EthTransactInfo<Balance>, $crate::EthTransactError> {
					use $crate::{
						codec::Encode, evm::runtime::EthExtra, frame_support::traits::Get,
						sp_runtime::traits::TransactionExtension,
						sp_runtime::traits::Block as BlockT
					};

					let tx_fee = |call: <Self as $crate::frame_system::Config>::RuntimeCall, dispatch_call: <Self as $crate::frame_system::Config>::RuntimeCall| {
						use $crate::frame_support::dispatch::GetDispatchInfo;

						// Get the dispatch info of the actual call dispatched
						let mut dispatch_info = dispatch_call.get_dispatch_info();
						dispatch_info.extension_weight =
							<$EthExtra>::get_eth_extension(0, 0u32.into()).weight(&dispatch_call);

						// Build the extrinsic
						let uxt: <Block as BlockT>::Extrinsic =
							$crate::sp_runtime::generic::UncheckedExtrinsic::new_bare(call).into();

						// Compute the fee of the extrinsic
						$crate::pallet_transaction_payment::Pallet::<Self>::compute_fee(
							uxt.encoded_size() as u32,
							&dispatch_info,
							0u32.into(),
						)
					};

					let blockweights: $crate::BlockWeights =
						<Self as $crate::frame_system::Config>::BlockWeights::get();
					$crate::Pallet::<Self>::dry_run_eth_transact(tx, blockweights.max_block, tx_fee)
				}

				fn call(
					origin: AccountId,
					dest: $crate::H160,
					value: Balance,
					gas_limit: Option<$crate::Weight>,
					storage_deposit_limit: Option<Balance>,
					input_data: Vec<u8>,
				) -> $crate::ContractResult<$crate::ExecReturnValue, Balance> {
					use $crate::frame_support::traits::Get;
					let blockweights: $crate::BlockWeights =
						<Self as $crate::frame_system::Config>::BlockWeights::get();

					$crate::Pallet::<Self>::prepare_dry_run(&origin);
					$crate::Pallet::<Self>::bare_call(
						<Self as $crate::frame_system::Config>::RuntimeOrigin::signed(origin),
						dest,
						$crate::Pallet::<Self>::convert_native_to_evm(value),
						gas_limit.unwrap_or(blockweights.max_block),
						$crate::DepositLimit::Balance(storage_deposit_limit.unwrap_or(u128::MAX)),
						input_data,
					)
				}

				fn instantiate(
					origin: AccountId,
					value: Balance,
					gas_limit: Option<$crate::Weight>,
					storage_deposit_limit: Option<Balance>,
					code: $crate::Code,
					data: Vec<u8>,
					salt: Option<[u8; 32]>,
				) -> $crate::ContractResult<$crate::InstantiateReturnValue, Balance> {
					use $crate::frame_support::traits::Get;
					let blockweights: $crate::BlockWeights =
						<Self as $crate::frame_system::Config>::BlockWeights::get();

					$crate::Pallet::<Self>::prepare_dry_run(&origin);
					$crate::Pallet::<Self>::bare_instantiate(
						<Self as $crate::frame_system::Config>::RuntimeOrigin::signed(origin),
						$crate::Pallet::<Self>::convert_native_to_evm(value),
						gas_limit.unwrap_or(blockweights.max_block),
						$crate::DepositLimit::Balance(storage_deposit_limit.unwrap_or(u128::MAX)),
						code,
						data,
						salt,
						$crate::BumpNonce::Yes,
					)
				}

				fn upload_code(
					origin: AccountId,
					code: Vec<u8>,
					storage_deposit_limit: Option<Balance>,
				) -> $crate::CodeUploadResult<Balance> {
					let origin =
						<Self as $crate::frame_system::Config>::RuntimeOrigin::signed(origin);
					$crate::Pallet::<Self>::bare_upload_code(
						origin,
						code,
						storage_deposit_limit.unwrap_or(u128::MAX),
					)
				}

				fn get_storage_var_key(
					address: $crate::H160,
					key: Vec<u8>,
				) -> $crate::GetStorageResult {
					$crate::Pallet::<Self>::get_storage_var_key(address, key)
				}

				fn get_storage(address: $crate::H160, key: [u8; 32]) -> $crate::GetStorageResult {
					$crate::Pallet::<Self>::get_storage(address, key)
				}

				fn trace_block(
					block: Block,
					tracer_type: $crate::evm::TracerType,
				) -> Vec<(u32, $crate::evm::Trace)> {
					use $crate::{sp_runtime::traits::Block, tracing::trace};
					let mut traces = vec![];
					let (header, extrinsics) = block.deconstruct();
					<$Executive>::initialize_block(&header);
					for (index, ext) in extrinsics.into_iter().enumerate() {
						let mut tracer = $crate::Pallet::<Self>::evm_tracer(tracer_type.clone());
						let t = tracer.as_tracing();
						let _ = trace(t, || <$Executive>::apply_extrinsic(ext));

						if let Some(tx_trace) = tracer.collect_trace() {
							traces.push((index as u32, tx_trace));
						}
					}

					traces
				}

				fn trace_tx(
					block: Block,
					tx_index: u32,
					tracer_type: $crate::evm::TracerType,
				) -> Option<$crate::evm::Trace> {
					use $crate::{sp_runtime::traits::Block, tracing::trace};

					let mut tracer = $crate::Pallet::<Self>::evm_tracer(tracer_type);
					let (header, extrinsics) = block.deconstruct();

					<$Executive>::initialize_block(&header);
					for (index, ext) in extrinsics.into_iter().enumerate() {
						if index as u32 == tx_index {
							let t = tracer.as_tracing();
							let _ = trace(t, || <$Executive>::apply_extrinsic(ext));
							break;
						} else {
							let _ = <$Executive>::apply_extrinsic(ext);
						}
					}

					tracer.collect_trace()
				}

				fn trace_call(
					tx: $crate::evm::GenericTransaction,
					tracer_type: $crate::evm::TracerType,
				) -> Result<$crate::evm::Trace, $crate::EthTransactError> {
					use $crate::tracing::trace;
					let mut tracer = $crate::Pallet::<Self>::evm_tracer(tracer_type.clone());
					let t = tracer.as_tracing();

					t.watch_address(&tx.from.unwrap_or_default());
					t.watch_address(&$crate::Pallet::<Self>::block_author().unwrap_or_default());
					let result = trace(t, || Self::eth_transact(tx));

					if let Some(trace) = tracer.collect_trace() {
						Ok(trace)
					} else if let Err(err) = result {
						Err(err)
					} else {
						Ok($crate::Pallet::<Self>::evm_tracer(tracer_type).empty_trace())
					}
				}

				fn runtime_pallets_address() -> $crate::H160 {
					$crate::RUNTIME_PALLETS_ADDR
				}

				fn code(address: $crate::H160) -> Vec<u8> {
					$crate::Pallet::<Self>::code(&address)
				}

				fn account_id(address: $crate::H160) -> AccountId {
					use $crate::AddressMapper;
					<Self as $crate::Config>::AddressMapper::to_account_id(&address)
				}

				fn new_balance_with_dust(balance: $crate::U256) -> Result<(Balance, u32), $crate::BalanceConversionError> {
					$crate::Pallet::<Self>::new_balance_with_dust(balance)
				}
			}
		}
	};
}
