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

use crate::{
	address::{self, AddressMapper},
	debug::{CallInterceptor, CallSpan, Tracing},
	gas::GasMeter,
	limits,
	primitives::{ExecReturnValue, StorageDeposit},
	runtime_decl_for_revive_api::{Decode, Encode, RuntimeDebugNoBound, TypeInfo},
	storage::{self, meter::Diff, WriteOutcome},
	transient_storage::TransientStorage,
	BalanceOf, CodeInfo, CodeInfoOf, Config, ContractInfo, ContractInfoOf, DebugBuffer, Error,
	Event, ImmutableData, ImmutableDataOf, Pallet as Contracts, LOG_TARGET,
};
use alloc::vec::Vec;
use core::{fmt::Debug, marker::PhantomData, mem};
use frame_support::{
	crypto::ecdsa::ECDSAExt,
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	ensure,
	storage::{with_transaction, TransactionOutcome},
	traits::{
		fungible::{Inspect, Mutate},
		tokens::{Fortitude, Preservation},
		Contains, OriginTrait, Time,
	},
	weights::Weight,
	Blake2_128Concat, BoundedVec, StorageHasher,
};
use frame_system::{
	pallet_prelude::{BlockNumberFor, OriginFor},
	Pallet as System, RawOrigin,
};
use sp_core::{
	ecdsa::Public as ECDSAPublic,
	sr25519::{Public as SR25519Public, Signature as SR25519Signature},
	ConstU32, Get, H160, H256, U256,
};
use sp_io::{crypto::secp256k1_ecdsa_recover_compressed, hashing::blake2_256};
use sp_runtime::{
	traits::{BadOrigin, Convert, Dispatchable, Zero},
	DispatchError, SaturatedConversion,
};

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub type MomentOf<T> = <<T as Config>::Time as Time>::Moment;
pub type ExecResult = Result<ExecReturnValue, ExecError>;

/// Type for variable sized storage key. Used for transparent hashing.
type VarSizedKey = BoundedVec<u8, ConstU32<{ limits::STORAGE_KEY_BYTES }>>;

const FRAME_ALWAYS_EXISTS_ON_INSTANTIATE: &str = "The return value is only `None` if no contract exists at the specified address. This cannot happen on instantiate or delegate; qed";

/// Code hash of existing account without code (keccak256 hash of empty data).
pub const EMPTY_CODE_HASH: H256 =
	H256(sp_core::hex2array!("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"));

/// Combined key type for both fixed and variable sized storage keys.
pub enum Key {
	/// Variant for fixed sized keys.
	Fix([u8; 32]),
	/// Variant for variable sized keys.
	Var(VarSizedKey),
}

impl Key {
	/// Reference to the raw unhashed key.
	///
	/// # Note
	///
	/// Only used by benchmarking in order to generate storage collisions on purpose.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn unhashed(&self) -> &[u8] {
		match self {
			Key::Fix(v) => v.as_ref(),
			Key::Var(v) => v.as_ref(),
		}
	}

	/// The hashed key that has be used as actual key to the storage trie.
	pub fn hash(&self) -> Vec<u8> {
		match self {
			Key::Fix(v) => blake2_256(v.as_slice()).to_vec(),
			Key::Var(v) => Blake2_128Concat::hash(v.as_slice()),
		}
	}

	pub fn from_fixed(v: [u8; 32]) -> Self {
		Self::Fix(v)
	}

	pub fn try_from_var(v: Vec<u8>) -> Result<Self, ()> {
		VarSizedKey::try_from(v).map(Self::Var).map_err(|_| ())
	}
}

/// Origin of the error.
///
/// Call or instantiate both called into other contracts and pass through errors happening
/// in those to the caller. This enum is for the caller to distinguish whether the error
/// happened during the execution of the callee or in the current execution context.
#[derive(Copy, Clone, PartialEq, Eq, Debug, codec::Decode, codec::Encode)]
pub enum ErrorOrigin {
	/// Caller error origin.
	///
	/// The error happened in the current execution context rather than in the one
	/// of the contract that is called into.
	Caller,
	/// The error happened during execution of the called contract.
	Callee,
}

/// Error returned by contract execution.
#[derive(Copy, Clone, PartialEq, Eq, Debug, codec::Decode, codec::Encode)]
pub struct ExecError {
	/// The reason why the execution failed.
	pub error: DispatchError,
	/// Origin of the error.
	pub origin: ErrorOrigin,
}

impl<T: Into<DispatchError>> From<T> for ExecError {
	fn from(error: T) -> Self {
		Self { error: error.into(), origin: ErrorOrigin::Caller }
	}
}

/// The type of origins supported by the contracts pallet.
#[derive(Clone, Encode, Decode, PartialEq, TypeInfo, RuntimeDebugNoBound)]
pub enum Origin<T: Config> {
	Root,
	Signed(T::AccountId),
}

impl<T: Config> Origin<T> {
	/// Creates a new Signed Caller from an AccountId.
	pub fn from_account_id(account_id: T::AccountId) -> Self {
		Origin::Signed(account_id)
	}
	/// Creates a new Origin from a `RuntimeOrigin`.
	pub fn from_runtime_origin(o: OriginFor<T>) -> Result<Self, DispatchError> {
		match o.into() {
			Ok(RawOrigin::Root) => Ok(Self::Root),
			Ok(RawOrigin::Signed(t)) => Ok(Self::Signed(t)),
			_ => Err(BadOrigin.into()),
		}
	}
	/// Returns the AccountId of a Signed Origin or an error if the origin is Root.
	pub fn account_id(&self) -> Result<&T::AccountId, DispatchError> {
		match self {
			Origin::Signed(id) => Ok(id),
			Origin::Root => Err(DispatchError::RootNotAllowed),
		}
	}

	/// Make sure that this origin is mapped.
	///
	/// We require an origin to be mapped in order to be used in a `Stack`. Otherwise
	/// [`Stack::caller`] returns an address that can't be reverted to the original address.
	fn ensure_mapped(&self) -> DispatchResult {
		match self {
			Self::Root => Ok(()),
			Self::Signed(account_id) if T::AddressMapper::is_mapped(account_id) => Ok(()),
			Self::Signed(_) => Err(<Error<T>>::AccountUnmapped.into()),
		}
	}
}

/// An interface that provides access to the external environment in which the
/// smart-contract is executed.
///
/// This interface is specialized to an account of the executing code, so all
/// operations are implicitly performed on that account.
///
/// # Note
///
/// This trait is sealed and cannot be implemented by downstream crates.
pub trait Ext: sealing::Sealed {
	type T: Config;

	/// Call (possibly transferring some amount of funds) into the specified account.
	///
	/// Returns the code size of the called contract.
	fn call(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		to: &H160,
		value: U256,
		input_data: Vec<u8>,
		allows_reentry: bool,
		read_only: bool,
	) -> Result<(), ExecError>;

	/// Execute code in the current frame.
	///
	/// Returns the code size of the called contract.
	fn delegate_call(&mut self, code: H256, input_data: Vec<u8>) -> Result<(), ExecError>;

	/// Instantiate a contract from the given code.
	///
	/// Returns the original code size of the called contract.
	/// The newly created account will be associated with `code`. `value` specifies the amount of
	/// value transferred from the caller to the newly created account.
	fn instantiate(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		code: H256,
		value: U256,
		input_data: Vec<u8>,
		salt: Option<&[u8; 32]>,
	) -> Result<H160, ExecError>;

	/// Transfer all funds to `beneficiary` and delete the contract.
	///
	/// Since this function removes the self contract eagerly, if succeeded, no further actions
	/// should be performed on this `Ext` instance.
	///
	/// This function will fail if the same contract is present on the contract
	/// call stack.
	fn terminate(&mut self, beneficiary: &H160) -> DispatchResult;

	/// Transfer some amount of funds into the specified account.
	fn transfer(&mut self, to: &H160, value: U256) -> DispatchResult;

	/// Returns the storage entry of the executing account by the given `key`.
	///
	/// Returns `None` if the `key` wasn't previously set by `set_storage` or
	/// was deleted.
	fn get_storage(&mut self, key: &Key) -> Option<Vec<u8>>;

	/// Returns `Some(len)` (in bytes) if a storage item exists at `key`.
	///
	/// Returns `None` if the `key` wasn't previously set by `set_storage` or
	/// was deleted.
	fn get_storage_size(&mut self, key: &Key) -> Option<u32>;

	/// Sets the storage entry by the given key to the specified value. If `value` is `None` then
	/// the storage entry is deleted.
	fn set_storage(
		&mut self,
		key: &Key,
		value: Option<Vec<u8>>,
		take_old: bool,
	) -> Result<WriteOutcome, DispatchError>;

	/// Returns the transient storage entry of the executing account for the given `key`.
	///
	/// Returns `None` if the `key` wasn't previously set by `set_transient_storage` or
	/// was deleted.
	fn get_transient_storage(&self, key: &Key) -> Option<Vec<u8>>;

	/// Returns `Some(len)` (in bytes) if a transient storage item exists at `key`.
	///
	/// Returns `None` if the `key` wasn't previously set by `set_transient_storage` or
	/// was deleted.
	fn get_transient_storage_size(&self, key: &Key) -> Option<u32>;

	/// Sets the transient storage entry for the given key to the specified value. If `value` is
	/// `None` then the storage entry is deleted.
	fn set_transient_storage(
		&mut self,
		key: &Key,
		value: Option<Vec<u8>>,
		take_old: bool,
	) -> Result<WriteOutcome, DispatchError>;

	/// Returns the caller.
	fn caller(&self) -> Origin<Self::T>;

	/// Return the origin of the whole call stack.
	fn origin(&self) -> &Origin<Self::T>;

	/// Check if a contract lives at the specified `address`.
	fn is_contract(&self, address: &H160) -> bool;

	/// Returns the code hash of the contract for the given `address`.
	/// If not a contract but account exists then `keccak_256([])` is returned, otherwise `zero`.
	fn code_hash(&self, address: &H160) -> H256;

	/// Returns the code hash of the contract being executed.
	fn own_code_hash(&mut self) -> &H256;

	/// Check if the caller of the current contract is the origin of the whole call stack.
	///
	/// This can be checked with `is_contract(self.caller())` as well.
	/// However, this function does not require any storage lookup and therefore uses less weight.
	fn caller_is_origin(&self) -> bool;

	/// Check if the caller is origin, and this origin is root.
	fn caller_is_root(&self) -> bool;

	/// Returns a reference to the account id of the current contract.
	fn account_id(&self) -> &AccountIdOf<Self::T>;

	/// Returns a reference to the [`H160`] address of the current contract.
	fn address(&self) -> H160 {
		<Self::T as Config>::AddressMapper::to_address(self.account_id())
	}

	/// Returns the immutable data of the current contract.
	///
	/// Returns `Err(InvalidImmutableAccess)` if called from a constructor.
	fn get_immutable_data(&mut self) -> Result<ImmutableData, DispatchError>;

	/// Set the the immutable data of the current contract.
	///
	/// Returns `Err(InvalidImmutableAccess)` if not called from a constructor.
	///
	/// Note: Requires &mut self to access the contract info.
	fn set_immutable_data(&mut self, data: ImmutableData) -> Result<(), DispatchError>;

	/// Returns the balance of the current contract.
	///
	/// The `value_transferred` is already added.
	fn balance(&self) -> U256;

	/// Returns the balance of the supplied account.
	///
	/// The `value_transferred` is already added.
	fn balance_of(&self, address: &H160) -> U256;

	/// Returns the value transferred along with this call.
	fn value_transferred(&self) -> U256;

	/// Returns the timestamp of the current block
	fn now(&self) -> U256;

	/// Returns the minimum balance that is required for creating an account.
	fn minimum_balance(&self) -> U256;

	/// Deposit an event with the given topics.
	///
	/// There should not be any duplicates in `topics`.
	fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>);

	/// Returns the current block number.
	fn block_number(&self) -> U256;

	/// Returns the maximum allowed size of a storage item.
	fn max_value_size(&self) -> u32;

	/// Returns the price for the specified amount of weight.
	fn get_weight_price(&self, weight: Weight) -> U256;

	/// Get an immutable reference to the nested gas meter.
	fn gas_meter(&self) -> &GasMeter<Self::T>;

	/// Get a mutable reference to the nested gas meter.
	fn gas_meter_mut(&mut self) -> &mut GasMeter<Self::T>;

	/// Charges `diff` from the meter.
	fn charge_storage(&mut self, diff: &Diff);

	/// Append a string to the debug buffer.
	///
	/// It is added as-is without any additional new line.
	///
	/// This is a no-op if debug message recording is disabled which is always the case
	/// when the code is executing on-chain.
	///
	/// Returns `true` if debug message recording is enabled. Otherwise `false` is returned.
	fn append_debug_buffer(&mut self, msg: &str) -> bool;

	/// Returns `true` if debug message recording is enabled. Otherwise `false` is returned.
	fn debug_buffer_enabled(&self) -> bool;

	/// Call some dispatchable and return the result.
	fn call_runtime(&self, call: <Self::T as Config>::RuntimeCall) -> DispatchResultWithPostInfo;

	/// Recovers ECDSA compressed public key based on signature and message hash.
	fn ecdsa_recover(&self, signature: &[u8; 65], message_hash: &[u8; 32]) -> Result<[u8; 33], ()>;

	/// Verify a sr25519 signature.
	fn sr25519_verify(&self, signature: &[u8; 64], message: &[u8], pub_key: &[u8; 32]) -> bool;

	/// Returns Ethereum address from the ECDSA compressed public key.
	fn ecdsa_to_eth_address(&self, pk: &[u8; 33]) -> Result<[u8; 20], ()>;

	/// Tests sometimes need to modify and inspect the contract info directly.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	fn contract_info(&mut self) -> &mut ContractInfo<Self::T>;

	/// Get a mutable reference to the transient storage.
	/// Useful in benchmarks when it is sometimes necessary to modify and inspect the transient
	/// storage directly.
	#[cfg(feature = "runtime-benchmarks")]
	fn transient_storage(&mut self) -> &mut TransientStorage<Self::T>;

	/// Sets new code hash and immutable data for an existing contract.
	fn set_code_hash(&mut self, hash: H256) -> DispatchResult;

	/// Returns the number of times the specified contract exists on the call stack. Delegated calls
	/// Increment the reference count of a of a stored code by one.
	///
	/// # Errors
	///
	/// [`Error::CodeNotFound`] is returned if no stored code found having the specified
	/// `code_hash`.
	fn increment_refcount(code_hash: H256) -> DispatchResult;

	/// Decrement the reference count of a stored code by one.
	///
	/// # Note
	///
	/// A contract whose reference count dropped to zero isn't automatically removed. A
	/// `remove_code` transaction must be submitted by the original uploader to do so.
	fn decrement_refcount(code_hash: H256);

	/// Adds a delegate dependency to [`ContractInfo`]'s `delegate_dependencies` field.
	///
	/// This ensures that the delegated contract is not removed while it is still in use. It
	/// increases the reference count of the code hash and charges a fraction (see
	/// [`Config::CodeHashLockupDepositPercent`]) of the code deposit.
	///
	/// # Errors
	///
	/// - [`Error::MaxDelegateDependenciesReached`]
	/// - [`Error::CannotAddSelfAsDelegateDependency`]
	/// - [`Error::DelegateDependencyAlreadyExists`]
	fn lock_delegate_dependency(&mut self, code_hash: H256) -> DispatchResult;

	/// Removes a delegate dependency from [`ContractInfo`]'s `delegate_dependencies` field.
	///
	/// This is the counterpart of [`Self::lock_delegate_dependency`]. It decreases the reference
	/// count and refunds the deposit that was charged by [`Self::lock_delegate_dependency`].
	///
	/// # Errors
	///
	/// - [`Error::DelegateDependencyNotFound`]
	fn unlock_delegate_dependency(&mut self, code_hash: &H256) -> DispatchResult;

	/// Returns the number of locked delegate dependencies.
	///
	/// Note: Requires &mut self to access the contract info.
	fn locked_delegate_dependencies_count(&mut self) -> usize;

	/// Check if running in read-only context.
	fn is_read_only(&self) -> bool;

	/// Returns an immutable reference to the output of the last executed call frame.
	fn last_frame_output(&self) -> &ExecReturnValue;

	/// Returns a mutable reference to the output of the last executed call frame.
	fn last_frame_output_mut(&mut self) -> &mut ExecReturnValue;
}

/// Describes the different functions that can be exported by an [`Executable`].
#[derive(
	Copy,
	Clone,
	PartialEq,
	Eq,
	sp_core::RuntimeDebug,
	codec::Decode,
	codec::Encode,
	codec::MaxEncodedLen,
	scale_info::TypeInfo,
)]
pub enum ExportedFunction {
	/// The constructor function which is executed on deployment of a contract.
	Constructor,
	/// The function which is executed when a contract is called.
	Call,
}

/// A trait that represents something that can be executed.
///
/// In the on-chain environment this would be represented by a wasm module. This trait exists in
/// order to be able to mock the wasm logic for testing.
pub trait Executable<T: Config>: Sized {
	/// Load the executable from storage.
	///
	/// # Note
	/// Charges size base load weight from the gas meter.
	fn from_storage(code_hash: H256, gas_meter: &mut GasMeter<T>) -> Result<Self, DispatchError>;

	/// Execute the specified exported function and return the result.
	///
	/// When the specified function is `Constructor` the executable is stored and its
	/// refcount incremented.
	///
	/// # Note
	///
	/// This functions expects to be executed in a storage transaction that rolls back
	/// all of its emitted storage changes.
	fn execute<E: Ext<T = T>>(
		self,
		ext: &mut E,
		function: ExportedFunction,
		input_data: Vec<u8>,
	) -> ExecResult;

	/// The code info of the executable.
	fn code_info(&self) -> &CodeInfo<T>;

	/// The raw code of the executable.
	fn code(&self) -> &[u8];

	/// The code hash of the executable.
	fn code_hash(&self) -> &H256;
}

/// The complete call stack of a contract execution.
///
/// The call stack is initiated by either a signed origin or one of the contract RPC calls.
/// This type implements `Ext` and by that exposes the business logic of contract execution to
/// the runtime module which interfaces with the contract (the wasm blob) itself.
pub struct Stack<'a, T: Config, E> {
	/// The origin that initiated the call stack. It could either be a Signed plain account that
	/// holds an account id or Root.
	///
	/// # Note
	///
	/// Please note that it is possible that the id of a Signed origin belongs to a contract rather
	/// than a plain account when being called through one of the contract RPCs where the
	/// client can freely choose the origin. This usually makes no sense but is still possible.
	origin: Origin<T>,
	/// The gas meter where costs are charged to.
	gas_meter: &'a mut GasMeter<T>,
	/// The storage meter makes sure that the storage deposit limit is obeyed.
	storage_meter: &'a mut storage::meter::Meter<T>,
	/// The timestamp at the point of call stack instantiation.
	timestamp: MomentOf<T>,
	/// The block number at the time of call stack instantiation.
	block_number: BlockNumberFor<T>,
	/// The actual call stack. One entry per nested contract called/instantiated.
	/// This does **not** include the [`Self::first_frame`].
	frames: BoundedVec<Frame<T>, ConstU32<{ limits::CALL_STACK_DEPTH }>>,
	/// Statically guarantee that each call stack has at least one frame.
	first_frame: Frame<T>,
	/// A text buffer used to output human readable information.
	///
	/// All the bytes added to this field should be valid UTF-8. The buffer has no defined
	/// structure and is intended to be shown to users as-is for debugging purposes.
	debug_message: Option<&'a mut DebugBuffer>,
	/// Transient storage used to store data, which is kept for the duration of a transaction.
	transient_storage: TransientStorage<T>,
	/// No executable is held by the struct but influences its behaviour.
	_phantom: PhantomData<E>,
}

/// Represents one entry in the call stack.
///
/// For each nested contract call or instantiate one frame is created. It holds specific
/// information for the said call and caches the in-storage `ContractInfo` data structure.
struct Frame<T: Config> {
	/// The address of the executing contract.
	account_id: T::AccountId,
	/// The cached in-storage data of the contract.
	contract_info: CachedContract<T>,
	/// The amount of balance transferred by the caller as part of the call.
	value_transferred: BalanceOf<T>,
	/// Determines whether this is a call or instantiate frame.
	entry_point: ExportedFunction,
	/// The gas meter capped to the supplied gas limit.
	nested_gas: GasMeter<T>,
	/// The storage meter for the individual call.
	nested_storage: storage::meter::NestedMeter<T>,
	/// If `false` the contract enabled its defense against reentrance attacks.
	allows_reentry: bool,
	/// If `true` subsequent calls cannot modify storage.
	read_only: bool,
	/// The caller of the currently executing frame which was spawned by `delegate_call`.
	delegate_caller: Option<Origin<T>>,
	/// The output of the last executed call frame.
	last_frame_output: ExecReturnValue,
}

/// Used in a delegate call frame arguments in order to override the executable and caller.
struct DelegatedCall<T: Config, E> {
	/// The executable which is run instead of the contracts own `executable`.
	executable: E,
	/// The caller of the contract.
	caller: Origin<T>,
}

/// Parameter passed in when creating a new `Frame`.
///
/// It determines whether the new frame is for a call or an instantiate.
enum FrameArgs<'a, T: Config, E> {
	Call {
		/// The account id of the contract that is to be called.
		dest: T::AccountId,
		/// If `None` the contract info needs to be reloaded from storage.
		cached_info: Option<ContractInfo<T>>,
		/// This frame was created by `seal_delegate_call` and hence uses different code than
		/// what is stored at [`Self::Call::dest`]. Its caller ([`DelegatedCall::caller`]) is the
		/// account which called the caller contract
		delegated_call: Option<DelegatedCall<T, E>>,
	},
	Instantiate {
		/// The contract or signed origin which instantiates the new contract.
		sender: T::AccountId,
		/// The executable whose `deploy` function is run.
		executable: E,
		/// A salt used in the contract address derivation of the new contract.
		salt: Option<&'a [u8; 32]>,
		/// The input data is used in the contract address derivation of the new contract.
		input_data: &'a [u8],
	},
}

/// Describes the different states of a contract as contained in a `Frame`.
enum CachedContract<T: Config> {
	/// The cached contract is up to date with the in-storage value.
	Cached(ContractInfo<T>),
	/// A recursive call into the same contract did write to the contract info.
	///
	/// In this case the cached contract is stale and needs to be reloaded from storage.
	Invalidated,
	/// The current contract executed `terminate` and removed the contract.
	///
	/// In this case a reload is neither allowed nor possible. Please note that recursive
	/// calls cannot remove a contract as this is checked and denied.
	Terminated,
}

impl<T: Config> Frame<T> {
	/// Return the `contract_info` of the current contract.
	fn contract_info(&mut self) -> &mut ContractInfo<T> {
		self.contract_info.get(&self.account_id)
	}

	/// Terminate and return the `contract_info` of the current contract.
	///
	/// # Note
	///
	/// Under no circumstances the contract is allowed to access the `contract_info` after
	/// a call to this function. This would constitute a programming error in the exec module.
	fn terminate(&mut self) -> ContractInfo<T> {
		self.contract_info.terminate(&self.account_id)
	}
}

/// Extract the contract info after loading it from storage.
///
/// This assumes that `load` was executed before calling this macro.
macro_rules! get_cached_or_panic_after_load {
	($c:expr) => {{
		if let CachedContract::Cached(contract) = $c {
			contract
		} else {
			panic!(
				"It is impossible to remove a contract that is on the call stack;\
				See implementations of terminate;\
				Therefore fetching a contract will never fail while using an account id
				that is currently active on the call stack;\
				qed"
			);
		}
	}};
}

/// Same as [`Stack::top_frame`].
///
/// We need this access as a macro because sometimes hiding the lifetimes behind
/// a function won't work out.
macro_rules! top_frame {
	($stack:expr) => {
		$stack.frames.last().unwrap_or(&$stack.first_frame)
	};
}

/// Same as [`Stack::top_frame_mut`].
///
/// We need this access as a macro because sometimes hiding the lifetimes behind
/// a function won't work out.
macro_rules! top_frame_mut {
	($stack:expr) => {
		$stack.frames.last_mut().unwrap_or(&mut $stack.first_frame)
	};
}

impl<T: Config> CachedContract<T> {
	/// Return `Some(ContractInfo)` if the contract is in cached state. `None` otherwise.
	fn into_contract(self) -> Option<ContractInfo<T>> {
		if let CachedContract::Cached(contract) = self {
			Some(contract)
		} else {
			None
		}
	}

	/// Return `Some(&mut ContractInfo)` if the contract is in cached state. `None` otherwise.
	fn as_contract(&mut self) -> Option<&mut ContractInfo<T>> {
		if let CachedContract::Cached(contract) = self {
			Some(contract)
		} else {
			None
		}
	}

	/// Load the `contract_info` from storage if necessary.
	fn load(&mut self, account_id: &T::AccountId) {
		if let CachedContract::Invalidated = self {
			let contract = <ContractInfoOf<T>>::get(T::AddressMapper::to_address(account_id));
			if let Some(contract) = contract {
				*self = CachedContract::Cached(contract);
			}
		}
	}

	/// Return the cached contract_info.
	fn get(&mut self, account_id: &T::AccountId) -> &mut ContractInfo<T> {
		self.load(account_id);
		get_cached_or_panic_after_load!(self)
	}

	/// Terminate and return the contract info.
	fn terminate(&mut self, account_id: &T::AccountId) -> ContractInfo<T> {
		self.load(account_id);
		get_cached_or_panic_after_load!(mem::replace(self, Self::Terminated))
	}
}

impl<'a, T, E> Stack<'a, T, E>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	E: Executable<T>,
{
	/// Create and run a new call stack by calling into `dest`.
	///
	/// # Note
	///
	/// `debug_message` should only ever be set to `Some` when executing as an RPC because
	/// it adds allocations and could be abused to drive the runtime into an OOM panic.
	///
	/// # Return Value
	///
	/// Result<(ExecReturnValue, CodeSize), (ExecError, CodeSize)>
	pub fn run_call(
		origin: Origin<T>,
		dest: H160,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: BalanceOf<T>,
		input_data: Vec<u8>,
		debug_message: Option<&'a mut DebugBuffer>,
	) -> ExecResult {
		let dest = T::AddressMapper::to_account_id(&dest);
		if let Some((mut stack, executable)) = Self::new(
			FrameArgs::Call { dest: dest.clone(), cached_info: None, delegated_call: None },
			origin.clone(),
			gas_meter,
			storage_meter,
			value,
			debug_message,
		)? {
			stack.run(executable, input_data).map(|_| stack.first_frame.last_frame_output)
		} else {
			Self::transfer_from_origin(&origin, &dest, value)
		}
	}

	/// Create and run a new call stack by instantiating a new contract.
	///
	/// # Note
	///
	/// `debug_message` should only ever be set to `Some` when executing as an RPC because
	/// it adds allocations and could be abused to drive the runtime into an OOM panic.
	///
	/// # Return Value
	///
	/// Result<(NewContractAccountId, ExecReturnValue), ExecError)>
	pub fn run_instantiate(
		origin: T::AccountId,
		executable: E,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: BalanceOf<T>,
		input_data: Vec<u8>,
		salt: Option<&[u8; 32]>,
		debug_message: Option<&'a mut DebugBuffer>,
	) -> Result<(H160, ExecReturnValue), ExecError> {
		let (mut stack, executable) = Self::new(
			FrameArgs::Instantiate {
				sender: origin.clone(),
				executable,
				salt,
				input_data: input_data.as_ref(),
			},
			Origin::from_account_id(origin),
			gas_meter,
			storage_meter,
			value,
			debug_message,
		)?
		.expect(FRAME_ALWAYS_EXISTS_ON_INSTANTIATE);
		let address = T::AddressMapper::to_address(&stack.top_frame().account_id);
		stack
			.run(executable, input_data)
			.map(|_| (address, stack.first_frame.last_frame_output))
	}

	#[cfg(all(feature = "runtime-benchmarks", feature = "riscv"))]
	pub fn bench_new_call(
		dest: H160,
		origin: Origin<T>,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: BalanceOf<T>,
		debug_message: Option<&'a mut DebugBuffer>,
	) -> (Self, E) {
		Self::new(
			FrameArgs::Call {
				dest: T::AddressMapper::to_account_id(&dest),
				cached_info: None,
				delegated_call: None,
			},
			origin,
			gas_meter,
			storage_meter,
			value,
			debug_message,
		)
		.unwrap()
		.unwrap()
	}

	/// Create a new call stack.
	///
	/// Returns `None` when calling a non existant contract. This is not an error case
	/// since this will result in a value transfer.
	fn new(
		args: FrameArgs<T, E>,
		origin: Origin<T>,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: BalanceOf<T>,
		debug_message: Option<&'a mut DebugBuffer>,
	) -> Result<Option<(Self, E)>, ExecError> {
		origin.ensure_mapped()?;
		let Some((first_frame, executable)) = Self::new_frame(
			args,
			value,
			gas_meter,
			Weight::zero(),
			storage_meter,
			BalanceOf::<T>::zero(),
			false,
			true,
		)?
		else {
			return Ok(None);
		};

		let stack = Self {
			origin,
			gas_meter,
			storage_meter,
			timestamp: T::Time::now(),
			block_number: <frame_system::Pallet<T>>::block_number(),
			first_frame,
			frames: Default::default(),
			debug_message,
			transient_storage: TransientStorage::new(limits::TRANSIENT_STORAGE_BYTES),
			_phantom: Default::default(),
		};

		Ok(Some((stack, executable)))
	}

	/// Construct a new frame.
	///
	/// This does not take `self` because when constructing the first frame `self` is
	/// not initialized, yet.
	fn new_frame<S: storage::meter::State + Default + Debug>(
		frame_args: FrameArgs<T, E>,
		value_transferred: BalanceOf<T>,
		gas_meter: &mut GasMeter<T>,
		gas_limit: Weight,
		storage_meter: &mut storage::meter::GenericMeter<T, S>,
		deposit_limit: BalanceOf<T>,
		read_only: bool,
		origin_is_caller: bool,
	) -> Result<Option<(Frame<T>, E)>, ExecError> {
		let (account_id, contract_info, executable, delegate_caller, entry_point) = match frame_args
		{
			FrameArgs::Call { dest, cached_info, delegated_call } => {
				let contract = if let Some(contract) = cached_info {
					contract
				} else {
					if let Some(contract) =
						<ContractInfoOf<T>>::get(T::AddressMapper::to_address(&dest))
					{
						contract
					} else {
						return Ok(None);
					}
				};

				let (executable, delegate_caller) =
					if let Some(DelegatedCall { executable, caller }) = delegated_call {
						(executable, Some(caller))
					} else {
						(E::from_storage(contract.code_hash, gas_meter)?, None)
					};

				(dest, contract, executable, delegate_caller, ExportedFunction::Call)
			},
			FrameArgs::Instantiate { sender, executable, salt, input_data } => {
				let deployer = T::AddressMapper::to_address(&sender);
				let account_nonce = <System<T>>::account_nonce(&sender);
				let address = if let Some(salt) = salt {
					address::create2(&deployer, executable.code(), input_data, salt)
				} else {
					use sp_runtime::Saturating;
					address::create1(
						&deployer,
						// the Nonce from the origin has been incremented pre-dispatch, so we need
						// to subtract 1 to get the nonce at the time of the call.
						if origin_is_caller {
							account_nonce.saturating_sub(1u32.into()).saturated_into()
						} else {
							account_nonce.saturated_into()
						},
					)
				};
				let contract = ContractInfo::new(
					&address,
					<System<T>>::account_nonce(&sender),
					*executable.code_hash(),
				)?;
				(
					T::AddressMapper::to_fallback_account_id(&address),
					contract,
					executable,
					None,
					ExportedFunction::Constructor,
				)
			},
		};

		let frame = Frame {
			delegate_caller,
			value_transferred,
			contract_info: CachedContract::Cached(contract_info),
			account_id,
			entry_point,
			nested_gas: gas_meter.nested(gas_limit),
			nested_storage: storage_meter.nested(deposit_limit),
			allows_reentry: true,
			read_only,
			last_frame_output: Default::default(),
		};

		Ok(Some((frame, executable)))
	}

	/// Create a subsequent nested frame.
	fn push_frame(
		&mut self,
		frame_args: FrameArgs<T, E>,
		value_transferred: BalanceOf<T>,
		gas_limit: Weight,
		deposit_limit: BalanceOf<T>,
		read_only: bool,
	) -> Result<Option<E>, ExecError> {
		if self.frames.len() as u32 == limits::CALL_STACK_DEPTH {
			return Err(Error::<T>::MaxCallDepthReached.into());
		}

		// We need to make sure that changes made to the contract info are not discarded.
		// See the `in_memory_changes_not_discarded` test for more information.
		// We do not store on instantiate because we do not allow to call into a contract
		// from its own constructor.
		let frame = self.top_frame();
		if let (CachedContract::Cached(contract), ExportedFunction::Call) =
			(&frame.contract_info, frame.entry_point)
		{
			<ContractInfoOf<T>>::insert(
				T::AddressMapper::to_address(&frame.account_id),
				contract.clone(),
			);
		}

		let frame = top_frame_mut!(self);
		let nested_gas = &mut frame.nested_gas;
		let nested_storage = &mut frame.nested_storage;
		if let Some((frame, executable)) = Self::new_frame(
			frame_args,
			value_transferred,
			nested_gas,
			gas_limit,
			nested_storage,
			deposit_limit,
			read_only,
			false,
		)? {
			self.frames.try_push(frame).map_err(|_| Error::<T>::MaxCallDepthReached)?;
			Ok(Some(executable))
		} else {
			Ok(None)
		}
	}

	/// Run the current (top) frame.
	///
	/// This can be either a call or an instantiate.
	fn run(&mut self, executable: E, input_data: Vec<u8>) -> Result<(), ExecError> {
		let frame = self.top_frame();
		let entry_point = frame.entry_point;
		let delegated_code_hash =
			if frame.delegate_caller.is_some() { Some(*executable.code_hash()) } else { None };

		// The output of the caller frame will be replaced by the output of this run.
		// It is also not accessible from nested frames.
		// Hence we drop it early to save the memory.
		let frames_len = self.frames.len();
		if let Some(caller_frame) = match frames_len {
			0 => None,
			1 => Some(&mut self.first_frame.last_frame_output),
			_ => self.frames.get_mut(frames_len - 2).map(|frame| &mut frame.last_frame_output),
		} {
			*caller_frame = Default::default();
		}

		self.transient_storage.start_transaction();

		let do_transaction = || {
			let caller = self.caller();
			let frame = top_frame_mut!(self);

			// We need to charge the storage deposit before the initial transfer so that
			// it can create the account in case the initial transfer is < ed.
			if entry_point == ExportedFunction::Constructor {
				// Root origin can't be used to instantiate a contract, so it is safe to assume that
				// if we reached this point the origin has an associated account.
				let origin = &self.origin.account_id()?;
				frame.nested_storage.charge_instantiate(
					origin,
					&frame.account_id,
					frame.contract_info.get(&frame.account_id),
					executable.code_info(),
				)?;
				// Needs to be incremented before calling into the code so that it is visible
				// in case of recursion.
				<System<T>>::inc_account_nonce(caller.account_id()?);
			}

			// Every non delegate call or instantiate also optionally transfers the balance.
			// If it is a delegate call, then we've already transferred tokens in the
			// last non-delegate frame.
			if delegated_code_hash.is_none() {
				Self::transfer_from_origin(&caller, &frame.account_id, frame.value_transferred)?;
			}

			let contract_address = T::AddressMapper::to_address(&top_frame!(self).account_id);

			let call_span = T::Debug::new_call_span(&contract_address, entry_point, &input_data);

			let output = T::Debug::intercept_call(&contract_address, entry_point, &input_data)
				.unwrap_or_else(|| {
					executable
						.execute(self, entry_point, input_data)
						.map_err(|e| ExecError { error: e.error, origin: ErrorOrigin::Callee })
				})?;

			call_span.after_call(&output);

			// Avoid useless work that would be reverted anyways.
			if output.did_revert() {
				return Ok(output);
			}

			// Storage limit is normally enforced as late as possible (when the last frame returns)
			// so that the ordering of storage accesses does not matter.
			// (However, if a special limit was set for a sub-call, it should be enforced right
			// after the sub-call returned. See below for this case of enforcement).
			if self.frames.is_empty() {
				let frame = &mut self.first_frame;
				frame.contract_info.load(&frame.account_id);
				let contract = frame.contract_info.as_contract();
				frame.nested_storage.enforce_limit(contract)?;
			}

			let frame = self.top_frame();
			let account_id = T::AddressMapper::to_address(&frame.account_id);
			match (entry_point, delegated_code_hash) {
				(ExportedFunction::Constructor, _) => {
					// It is not allowed to terminate a contract inside its constructor.
					if matches!(frame.contract_info, CachedContract::Terminated) {
						return Err(Error::<T>::TerminatedInConstructor.into());
					}

					// If a special limit was set for the sub-call, we enforce it here.
					// This is needed because contract constructor might write to storage.
					// The sub-call will be rolled back in case the limit is exhausted.
					let frame = self.top_frame_mut();
					let contract = frame.contract_info.as_contract();
					frame.nested_storage.enforce_subcall_limit(contract)?;

					let caller = T::AddressMapper::to_address(self.caller().account_id()?);

					// Deposit an instantiation event.
					Contracts::<T>::deposit_event(Event::Instantiated {
						deployer: caller,
						contract: account_id,
					});
				},
				(ExportedFunction::Call, Some(code_hash)) => {
					Contracts::<T>::deposit_event(Event::DelegateCalled {
						contract: account_id,
						code_hash,
					});
				},
				(ExportedFunction::Call, None) => {
					// If a special limit was set for the sub-call, we enforce it here.
					// The sub-call will be rolled back in case the limit is exhausted.
					let frame = self.top_frame_mut();
					let contract = frame.contract_info.as_contract();
					frame.nested_storage.enforce_subcall_limit(contract)?;

					let caller = self.caller();
					Contracts::<T>::deposit_event(Event::Called {
						caller: caller.clone(),
						contract: account_id,
					});
				},
			}

			Ok(output)
		};

		// All changes performed by the contract are executed under a storage transaction.
		// This allows for roll back on error. Changes to the cached contract_info are
		// committed or rolled back when popping the frame.
		//
		// `with_transactional` may return an error caused by a limit in the
		// transactional storage depth.
		let transaction_outcome =
			with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
				let output = do_transaction();
				match &output {
					Ok(result) if !result.did_revert() =>
						TransactionOutcome::Commit(Ok((true, output))),
					_ => TransactionOutcome::Rollback(Ok((false, output))),
				}
			});

		let (success, output) = match transaction_outcome {
			// `with_transactional` executed successfully, and we have the expected output.
			Ok((success, output)) => (success, output),
			// `with_transactional` returned an error, and we propagate that error and note no state
			// has changed.
			Err(error) => (false, Err(error.into())),
		};

		if success {
			self.transient_storage.commit_transaction();
		} else {
			self.transient_storage.rollback_transaction();
		}

		self.pop_frame(success);
		output.map(|output| {
			self.top_frame_mut().last_frame_output = output;
		})
	}

	/// Remove the current (top) frame from the stack.
	///
	/// This is called after running the current frame. It commits cached values to storage
	/// and invalidates all stale references to it that might exist further down the call stack.
	fn pop_frame(&mut self, persist: bool) {
		// Pop the current frame from the stack and return it in case it needs to interact
		// with duplicates that might exist on the stack.
		// A `None` means that we are returning from the `first_frame`.
		let frame = self.frames.pop();

		// Both branches do essentially the same with the exception. The difference is that
		// the else branch does consume the hardcoded `first_frame`.
		if let Some(mut frame) = frame {
			let account_id = &frame.account_id;
			let prev = top_frame_mut!(self);

			prev.nested_gas.absorb_nested(frame.nested_gas);

			// Only gas counter changes are persisted in case of a failure.
			if !persist {
				return;
			}

			// Record the storage meter changes of the nested call into the parent meter.
			// If the dropped frame's contract wasn't terminated we update the deposit counter
			// in its contract info. The load is necessary to pull it from storage in case
			// it was invalidated.
			frame.contract_info.load(account_id);
			let mut contract = frame.contract_info.into_contract();
			prev.nested_storage.absorb(frame.nested_storage, account_id, contract.as_mut());

			// In case the contract wasn't terminated we need to persist changes made to it.
			if let Some(contract) = contract {
				// optimization: Predecessor is the same contract.
				// We can just copy the contract into the predecessor without a storage write.
				// This is possible when there is no other contract in-between that could
				// trigger a rollback.
				if prev.account_id == *account_id {
					prev.contract_info = CachedContract::Cached(contract);
					return;
				}

				// Predecessor is a different contract: We persist the info and invalidate the first
				// stale cache we find. This triggers a reload from storage on next use. We skip(1)
				// because that case is already handled by the optimization above. Only the first
				// cache needs to be invalidated because that one will invalidate the next cache
				// when it is popped from the stack.
				<ContractInfoOf<T>>::insert(T::AddressMapper::to_address(account_id), contract);
				if let Some(c) = self.frames_mut().skip(1).find(|f| f.account_id == *account_id) {
					c.contract_info = CachedContract::Invalidated;
				}
			}
		} else {
			if let Some((msg, false)) = self.debug_message.as_ref().map(|m| (m, m.is_empty())) {
				log::debug!(
					target: LOG_TARGET,
					"Execution finished with debug buffer: {}",
					core::str::from_utf8(msg).unwrap_or("<Invalid UTF8>"),
				);
			}
			self.gas_meter.absorb_nested(mem::take(&mut self.first_frame.nested_gas));
			if !persist {
				return;
			}
			let mut contract = self.first_frame.contract_info.as_contract();
			self.storage_meter.absorb(
				mem::take(&mut self.first_frame.nested_storage),
				&self.first_frame.account_id,
				contract.as_deref_mut(),
			);
			if let Some(contract) = contract {
				<ContractInfoOf<T>>::insert(
					T::AddressMapper::to_address(&self.first_frame.account_id),
					contract,
				);
			}
		}
	}

	/// Transfer some funds from `from` to `to`.
	fn transfer(from: &T::AccountId, to: &T::AccountId, value: BalanceOf<T>) -> ExecResult {
		// this avoids events to be emitted for zero balance transfers
		if !value.is_zero() {
			T::Currency::transfer(from, to, value, Preservation::Preserve)
				.map_err(|_| Error::<T>::TransferFailed)?;
		}
		Ok(Default::default())
	}

	/// Same as `transfer` but `from` is an `Origin`.
	fn transfer_from_origin(
		from: &Origin<T>,
		to: &T::AccountId,
		value: BalanceOf<T>,
	) -> ExecResult {
		// If the from address is root there is no account to transfer from, and therefore we can't
		// take any `value` other than 0.
		let from = match from {
			Origin::Signed(caller) => caller,
			Origin::Root if value.is_zero() => return Ok(Default::default()),
			Origin::Root => return Err(DispatchError::RootNotAllowed.into()),
		};
		Self::transfer(from, to, value)
	}

	/// Reference to the current (top) frame.
	fn top_frame(&self) -> &Frame<T> {
		top_frame!(self)
	}

	/// Mutable reference to the current (top) frame.
	fn top_frame_mut(&mut self) -> &mut Frame<T> {
		top_frame_mut!(self)
	}

	/// Iterator over all frames.
	///
	/// The iterator starts with the top frame and ends with the root frame.
	fn frames(&self) -> impl Iterator<Item = &Frame<T>> {
		core::iter::once(&self.first_frame).chain(&self.frames).rev()
	}

	/// Same as `frames` but with a mutable reference as iterator item.
	fn frames_mut(&mut self) -> impl Iterator<Item = &mut Frame<T>> {
		core::iter::once(&mut self.first_frame).chain(&mut self.frames).rev()
	}

	/// Returns whether the current contract is on the stack multiple times.
	fn is_recursive(&self) -> bool {
		let account_id = &self.top_frame().account_id;
		self.frames().skip(1).any(|f| &f.account_id == account_id)
	}

	/// Returns whether the specified contract allows to be reentered right now.
	fn allows_reentry(&self, id: &T::AccountId) -> bool {
		!self.frames().any(|f| &f.account_id == id && !f.allows_reentry)
	}

	/// Returns the *free* balance of the supplied AccountId.
	fn account_balance(&self, who: &T::AccountId) -> U256 {
		T::Currency::reducible_balance(who, Preservation::Preserve, Fortitude::Polite).into()
	}

	/// Certain APIs, e.g. `{set,get}_immutable_data` behave differently depending
	/// on the configured entry point. Thus, we allow setting the export manually.
	#[cfg(all(feature = "runtime-benchmarks", feature = "riscv"))]
	pub(crate) fn override_export(&mut self, export: ExportedFunction) {
		self.top_frame_mut().entry_point = export;
	}
}

impl<'a, T, E> Ext for Stack<'a, T, E>
where
	T: Config,
	E: Executable<T>,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
{
	type T = T;

	fn call(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		dest: &H160,
		value: U256,
		input_data: Vec<u8>,
		allows_reentry: bool,
		read_only: bool,
	) -> Result<(), ExecError> {
		// Before pushing the new frame: Protect the caller contract against reentrancy attacks.
		// It is important to do this before calling `allows_reentry` so that a direct recursion
		// is caught by it.
		self.top_frame_mut().allows_reentry = allows_reentry;

		// We reset the return data now, so it is cleared out even if no new frame was executed.
		// This is for example the case for balance transfers or when creating the frame fails.
		*self.last_frame_output_mut() = Default::default();

		let try_call = || {
			let dest = T::AddressMapper::to_account_id(dest);
			if !self.allows_reentry(&dest) {
				return Err(<Error<T>>::ReentranceDenied.into());
			}

			let value = value.try_into().map_err(|_| Error::<T>::BalanceConversionFailed)?;

			// We ignore instantiate frames in our search for a cached contract.
			// Otherwise it would be possible to recursively call a contract from its own
			// constructor: We disallow calling not fully constructed contracts.
			let cached_info = self
				.frames()
				.find(|f| f.entry_point == ExportedFunction::Call && f.account_id == dest)
				.and_then(|f| match &f.contract_info {
					CachedContract::Cached(contract) => Some(contract.clone()),
					_ => None,
				});
			if let Some(executable) = self.push_frame(
				FrameArgs::Call { dest: dest.clone(), cached_info, delegated_call: None },
				value,
				gas_limit,
				deposit_limit.try_into().map_err(|_| Error::<T>::BalanceConversionFailed)?,
				// Enable read-only access if requested; cannot disable it if already set.
				read_only || self.is_read_only(),
			)? {
				self.run(executable, input_data)
			} else {
				Self::transfer(&self.account_id(), &dest, value).map(|_| ())
			}
		};

		// We need to make sure to reset `allows_reentry` even on failure.
		let result = try_call();

		// Protection is on a per call basis.
		self.top_frame_mut().allows_reentry = true;

		result
	}

	fn delegate_call(&mut self, code_hash: H256, input_data: Vec<u8>) -> Result<(), ExecError> {
		// We reset the return data now, so it is cleared out even if no new frame was executed.
		// This is for example the case for unknown code hashes or creating the frame fails.
		*self.last_frame_output_mut() = Default::default();

		let executable = E::from_storage(code_hash, self.gas_meter_mut())?;
		let top_frame = self.top_frame_mut();
		let contract_info = top_frame.contract_info().clone();
		let account_id = top_frame.account_id.clone();
		let value = top_frame.value_transferred;
		let executable = self.push_frame(
			FrameArgs::Call {
				dest: account_id,
				cached_info: Some(contract_info),
				delegated_call: Some(DelegatedCall { executable, caller: self.caller().clone() }),
			},
			value,
			Weight::zero(),
			BalanceOf::<T>::zero(),
			self.is_read_only(),
		)?;
		self.run(executable.expect(FRAME_ALWAYS_EXISTS_ON_INSTANTIATE), input_data)
	}

	fn instantiate(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		code_hash: H256,
		value: U256,
		input_data: Vec<u8>,
		salt: Option<&[u8; 32]>,
	) -> Result<H160, ExecError> {
		// We reset the return data now, so it is cleared out even if no new frame was executed.
		// This is for example the case when creating the frame fails.
		*self.last_frame_output_mut() = Default::default();

		let executable = E::from_storage(code_hash, self.gas_meter_mut())?;
		let sender = &self.top_frame().account_id;
		let executable = self.push_frame(
			FrameArgs::Instantiate {
				sender: sender.clone(),
				executable,
				salt,
				input_data: input_data.as_ref(),
			},
			value.try_into().map_err(|_| Error::<T>::BalanceConversionFailed)?,
			gas_limit,
			deposit_limit.try_into().map_err(|_| Error::<T>::BalanceConversionFailed)?,
			self.is_read_only(),
		)?;
		let address = T::AddressMapper::to_address(&self.top_frame().account_id);
		self.run(executable.expect(FRAME_ALWAYS_EXISTS_ON_INSTANTIATE), input_data)
			.map(|_| address)
	}

	fn terminate(&mut self, beneficiary: &H160) -> DispatchResult {
		if self.is_recursive() {
			return Err(Error::<T>::TerminatedWhileReentrant.into());
		}
		let frame = self.top_frame_mut();
		let info = frame.terminate();
		let beneficiary_account = T::AddressMapper::to_account_id(beneficiary);
		frame.nested_storage.terminate(&info, beneficiary_account);

		info.queue_trie_for_deletion();
		let account_address = T::AddressMapper::to_address(&frame.account_id);
		ContractInfoOf::<T>::remove(&account_address);
		ImmutableDataOf::<T>::remove(&account_address);
		Self::decrement_refcount(info.code_hash);

		for (code_hash, deposit) in info.delegate_dependencies() {
			Self::decrement_refcount(*code_hash);
			frame
				.nested_storage
				.charge_deposit(frame.account_id.clone(), StorageDeposit::Refund(*deposit));
		}

		Contracts::<T>::deposit_event(Event::Terminated {
			contract: account_address,
			beneficiary: *beneficiary,
		});
		Ok(())
	}

	fn transfer(&mut self, to: &H160, value: U256) -> DispatchResult {
		Self::transfer(
			&self.top_frame().account_id,
			&T::AddressMapper::to_account_id(to),
			value.try_into().map_err(|_| Error::<T>::BalanceConversionFailed)?,
		)
		.map(|_| ())
		.map_err(|error| error.error)
	}

	fn get_storage(&mut self, key: &Key) -> Option<Vec<u8>> {
		self.top_frame_mut().contract_info().read(key)
	}

	fn get_storage_size(&mut self, key: &Key) -> Option<u32> {
		self.top_frame_mut().contract_info().size(key.into())
	}

	fn set_storage(
		&mut self,
		key: &Key,
		value: Option<Vec<u8>>,
		take_old: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let frame = self.top_frame_mut();
		frame.contract_info.get(&frame.account_id).write(
			key.into(),
			value,
			Some(&mut frame.nested_storage),
			take_old,
		)
	}

	fn get_transient_storage(&self, key: &Key) -> Option<Vec<u8>> {
		self.transient_storage.read(self.account_id(), key)
	}

	fn get_transient_storage_size(&self, key: &Key) -> Option<u32> {
		self.transient_storage
			.read(self.account_id(), key)
			.map(|value| value.len() as _)
	}

	fn set_transient_storage(
		&mut self,
		key: &Key,
		value: Option<Vec<u8>>,
		take_old: bool,
	) -> Result<WriteOutcome, DispatchError> {
		let account_id = self.account_id().clone();
		self.transient_storage.write(&account_id, key, value, take_old)
	}

	fn account_id(&self) -> &T::AccountId {
		&self.top_frame().account_id
	}

	fn caller(&self) -> Origin<T> {
		if let Some(caller) = &self.top_frame().delegate_caller {
			caller.clone()
		} else {
			self.frames()
				.nth(1)
				.map(|f| Origin::from_account_id(f.account_id.clone()))
				.unwrap_or(self.origin.clone())
		}
	}

	fn origin(&self) -> &Origin<T> {
		&self.origin
	}

	fn is_contract(&self, address: &H160) -> bool {
		ContractInfoOf::<T>::contains_key(&address)
	}

	fn code_hash(&self, address: &H160) -> H256 {
		<ContractInfoOf<T>>::get(&address)
			.map(|contract| contract.code_hash)
			.unwrap_or_else(|| {
				if System::<T>::account_exists(&T::AddressMapper::to_account_id(address)) {
					return EMPTY_CODE_HASH;
				}
				H256::zero()
			})
	}

	fn own_code_hash(&mut self) -> &H256 {
		&self.top_frame_mut().contract_info().code_hash
	}

	fn caller_is_origin(&self) -> bool {
		self.origin == self.caller()
	}

	fn caller_is_root(&self) -> bool {
		// if the caller isn't origin, then it can't be root.
		self.caller_is_origin() && self.origin == Origin::Root
	}

	fn get_immutable_data(&mut self) -> Result<ImmutableData, DispatchError> {
		if self.top_frame().entry_point == ExportedFunction::Constructor {
			return Err(Error::<T>::InvalidImmutableAccess.into());
		}

		let address = T::AddressMapper::to_address(self.account_id());
		Ok(<ImmutableDataOf<T>>::get(address).ok_or_else(|| Error::<T>::InvalidImmutableAccess)?)
	}

	fn set_immutable_data(&mut self, data: ImmutableData) -> Result<(), DispatchError> {
		if self.top_frame().entry_point == ExportedFunction::Call {
			return Err(Error::<T>::InvalidImmutableAccess.into());
		}

		let account_id = self.account_id().clone();
		let len = data.len() as u32;
		let amount = self.top_frame_mut().contract_info().set_immutable_data_len(len)?;
		self.top_frame_mut().nested_storage.charge_deposit(account_id.clone(), amount);

		<ImmutableDataOf<T>>::insert(T::AddressMapper::to_address(&account_id), &data);

		Ok(())
	}

	fn balance(&self) -> U256 {
		self.account_balance(&self.top_frame().account_id)
	}

	fn balance_of(&self, address: &H160) -> U256 {
		self.account_balance(&<Self::T as Config>::AddressMapper::to_account_id(address))
	}

	fn value_transferred(&self) -> U256 {
		self.top_frame().value_transferred.into()
	}

	fn now(&self) -> U256 {
		self.timestamp.into()
	}

	fn minimum_balance(&self) -> U256 {
		T::Currency::minimum_balance().into()
	}

	fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>) {
		Contracts::<Self::T>::deposit_event(Event::ContractEmitted {
			contract: T::AddressMapper::to_address(self.account_id()),
			data,
			topics,
		});
	}

	fn block_number(&self) -> U256 {
		self.block_number.into()
	}

	fn max_value_size(&self) -> u32 {
		limits::PAYLOAD_BYTES
	}

	fn get_weight_price(&self, weight: Weight) -> U256 {
		T::WeightPrice::convert(weight).into()
	}

	fn gas_meter(&self) -> &GasMeter<Self::T> {
		&self.top_frame().nested_gas
	}

	fn gas_meter_mut(&mut self) -> &mut GasMeter<Self::T> {
		&mut self.top_frame_mut().nested_gas
	}

	fn charge_storage(&mut self, diff: &Diff) {
		self.top_frame_mut().nested_storage.charge(diff)
	}

	fn debug_buffer_enabled(&self) -> bool {
		self.debug_message.is_some()
	}

	fn append_debug_buffer(&mut self, msg: &str) -> bool {
		if let Some(buffer) = &mut self.debug_message {
			buffer
				.try_extend(&mut msg.bytes())
				.map_err(|_| {
					log::debug!(
						target: LOG_TARGET,
						"Debug buffer (of {} bytes) exhausted!",
						limits::DEBUG_BUFFER_BYTES,
					)
				})
				.ok();
			true
		} else {
			false
		}
	}

	fn call_runtime(&self, call: <Self::T as Config>::RuntimeCall) -> DispatchResultWithPostInfo {
		let mut origin: T::RuntimeOrigin = RawOrigin::Signed(self.account_id().clone()).into();
		origin.add_filter(T::CallFilter::contains);
		call.dispatch(origin)
	}

	fn ecdsa_recover(&self, signature: &[u8; 65], message_hash: &[u8; 32]) -> Result<[u8; 33], ()> {
		secp256k1_ecdsa_recover_compressed(signature, message_hash).map_err(|_| ())
	}

	fn sr25519_verify(&self, signature: &[u8; 64], message: &[u8], pub_key: &[u8; 32]) -> bool {
		sp_io::crypto::sr25519_verify(
			&SR25519Signature::from(*signature),
			message,
			&SR25519Public::from(*pub_key),
		)
	}

	fn ecdsa_to_eth_address(&self, pk: &[u8; 33]) -> Result<[u8; 20], ()> {
		ECDSAPublic::from(*pk).to_eth_address()
	}

	#[cfg(any(test, feature = "runtime-benchmarks"))]
	fn contract_info(&mut self) -> &mut ContractInfo<Self::T> {
		self.top_frame_mut().contract_info()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn transient_storage(&mut self) -> &mut TransientStorage<Self::T> {
		&mut self.transient_storage
	}

	/// TODO: This should be changed to run the constructor of the supplied `hash`.
	///
	/// Because the immutable data is attached to a contract and not a code,
	/// we need to update the immutable data too.
	///
	/// Otherwise we open a massive footgun:
	/// If the immutables changed in the new code, the contract will brick.
	///
	/// A possible implementation strategy is to add a flag to `FrameArgs::Instantiate`,
	/// so that `fn run()` will roll back any changes if this flag is set.
	///
	/// After running the constructor, the new immutable data is already stored in
	/// `self.immutable_data` at the address of the (reverted) contract instantiation.
	///
	/// The `set_code_hash` contract API stays disabled until this change is implemented.
	fn set_code_hash(&mut self, hash: H256) -> DispatchResult {
		let frame = top_frame_mut!(self);

		let info = frame.contract_info();

		let prev_hash = info.code_hash;
		info.code_hash = hash;

		let code_info = CodeInfoOf::<T>::get(hash).ok_or(Error::<T>::CodeNotFound)?;

		let old_base_deposit = info.storage_base_deposit();
		let new_base_deposit = info.update_base_deposit(&code_info);
		let deposit = StorageDeposit::Charge(new_base_deposit)
			.saturating_sub(&StorageDeposit::Charge(old_base_deposit));

		frame.nested_storage.charge_deposit(frame.account_id.clone(), deposit);

		Self::increment_refcount(hash)?;
		Self::decrement_refcount(prev_hash);
		Contracts::<Self::T>::deposit_event(Event::ContractCodeUpdated {
			contract: T::AddressMapper::to_address(&frame.account_id),
			new_code_hash: hash,
			old_code_hash: prev_hash,
		});
		Ok(())
	}

	fn increment_refcount(code_hash: H256) -> DispatchResult {
		<CodeInfoOf<Self::T>>::mutate(code_hash, |existing| -> Result<(), DispatchError> {
			if let Some(info) = existing {
				*info.refcount_mut() = info.refcount().saturating_add(1);
				Ok(())
			} else {
				Err(Error::<T>::CodeNotFound.into())
			}
		})
	}

	fn decrement_refcount(code_hash: H256) {
		<CodeInfoOf<T>>::mutate(code_hash, |existing| {
			if let Some(info) = existing {
				*info.refcount_mut() = info.refcount().saturating_sub(1);
			}
		});
	}

	fn lock_delegate_dependency(&mut self, code_hash: H256) -> DispatchResult {
		let frame = self.top_frame_mut();
		let info = frame.contract_info.get(&frame.account_id);
		ensure!(code_hash != info.code_hash, Error::<T>::CannotAddSelfAsDelegateDependency);

		let code_info = CodeInfoOf::<T>::get(code_hash).ok_or(Error::<T>::CodeNotFound)?;
		let deposit = T::CodeHashLockupDepositPercent::get().mul_ceil(code_info.deposit());

		info.lock_delegate_dependency(code_hash, deposit)?;
		Self::increment_refcount(code_hash)?;
		frame
			.nested_storage
			.charge_deposit(frame.account_id.clone(), StorageDeposit::Charge(deposit));
		Ok(())
	}

	fn unlock_delegate_dependency(&mut self, code_hash: &H256) -> DispatchResult {
		let frame = self.top_frame_mut();
		let info = frame.contract_info.get(&frame.account_id);

		let deposit = info.unlock_delegate_dependency(code_hash)?;
		Self::decrement_refcount(*code_hash);
		frame
			.nested_storage
			.charge_deposit(frame.account_id.clone(), StorageDeposit::Refund(deposit));
		Ok(())
	}

	fn locked_delegate_dependencies_count(&mut self) -> usize {
		self.top_frame_mut().contract_info().delegate_dependencies_count()
	}

	fn is_read_only(&self) -> bool {
		self.top_frame().read_only
	}

	fn last_frame_output(&self) -> &ExecReturnValue {
		&self.top_frame().last_frame_output
	}

	fn last_frame_output_mut(&mut self) -> &mut ExecReturnValue {
		&mut self.top_frame_mut().last_frame_output
	}
}

mod sealing {
	use super::*;

	pub trait Sealed {}

	impl<'a, T: Config, E> Sealed for Stack<'a, T, E> {}
}

/// These tests exercise the executive layer.
///
/// In these tests the VM/loader are mocked. Instead of dealing with wasm bytecode they use simple
/// closures. This allows you to tackle executive logic more thoroughly without writing a
/// wasm VM code.
#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		exec::ExportedFunction::*,
		gas::GasMeter,
		test_utils::*,
		tests::{
			test_utils::{get_balance, place_contract, set_balance},
			ExtBuilder, RuntimeCall, RuntimeEvent as MetaEvent, Test, TestFilter,
		},
		AddressMapper, Error,
	};
	use assert_matches::assert_matches;
	use frame_support::{assert_err, assert_ok, parameter_types};
	use frame_system::{AccountInfo, EventRecord, Phase};
	use pallet_revive_uapi::ReturnFlags;
	use pretty_assertions::assert_eq;
	use sp_io::hashing::keccak_256;
	use sp_runtime::{traits::Hash, DispatchError};
	use std::{cell::RefCell, collections::hash_map::HashMap, rc::Rc};

	type System = frame_system::Pallet<Test>;

	type MockStack<'a> = Stack<'a, Test, MockExecutable>;

	parameter_types! {
		static Loader: MockLoader = MockLoader::default();
	}

	fn events() -> Vec<Event<Test>> {
		System::events()
			.into_iter()
			.filter_map(|meta| match meta.event {
				MetaEvent::Contracts(contract_event) => Some(contract_event),
				_ => None,
			})
			.collect()
	}

	struct MockCtx<'a> {
		ext: &'a mut MockStack<'a>,
		input_data: Vec<u8>,
	}

	#[derive(Clone)]
	struct MockExecutable {
		func: Rc<dyn for<'a> Fn(MockCtx<'a>, &Self) -> ExecResult + 'static>,
		func_type: ExportedFunction,
		code_hash: H256,
		code_info: CodeInfo<Test>,
	}

	#[derive(Default, Clone)]
	pub struct MockLoader {
		map: HashMap<H256, MockExecutable>,
		counter: u64,
	}

	impl MockLoader {
		fn code_hashes() -> Vec<H256> {
			Loader::get().map.keys().copied().collect()
		}

		fn insert(
			func_type: ExportedFunction,
			f: impl Fn(MockCtx, &MockExecutable) -> ExecResult + 'static,
		) -> H256 {
			Loader::mutate(|loader| {
				// Generate code hashes from contract index value.
				let hash = H256(keccak_256(&loader.counter.to_le_bytes()));
				loader.counter += 1;
				loader.map.insert(
					hash,
					MockExecutable {
						func: Rc::new(f),
						func_type,
						code_hash: hash,
						code_info: CodeInfo::<Test>::new(ALICE),
					},
				);
				hash
			})
		}
	}

	impl Executable<Test> for MockExecutable {
		fn from_storage(
			code_hash: H256,
			_gas_meter: &mut GasMeter<Test>,
		) -> Result<Self, DispatchError> {
			Loader::mutate(|loader| {
				loader.map.get(&code_hash).cloned().ok_or(Error::<Test>::CodeNotFound.into())
			})
		}

		fn execute<E: Ext<T = Test>>(
			self,
			ext: &mut E,
			function: ExportedFunction,
			input_data: Vec<u8>,
		) -> ExecResult {
			if let Constructor = function {
				E::increment_refcount(self.code_hash).unwrap();
			}
			// # Safety
			//
			// We know that we **always** call execute with a `MockStack` in this test.
			//
			// # Note
			//
			// The transmute is necessary because `execute` has to be generic over all
			// `E: Ext`. However, `MockExecutable` can't be generic over `E` as it would
			// constitute a cycle.
			let ext = unsafe { mem::transmute(ext) };
			if function == self.func_type {
				(self.func)(MockCtx { ext, input_data }, &self)
			} else {
				exec_success()
			}
		}

		fn code(&self) -> &[u8] {
			// The mock executable doesn't have code", so we return the code hash.
			self.code_hash.as_ref()
		}

		fn code_hash(&self) -> &H256 {
			&self.code_hash
		}

		fn code_info(&self) -> &CodeInfo<Test> {
			&self.code_info
		}
	}

	fn exec_success() -> ExecResult {
		Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
	}

	fn exec_trapped() -> ExecResult {
		Err(ExecError { error: <Error<Test>>::ContractTrapped.into(), origin: ErrorOrigin::Callee })
	}

	#[test]
	fn it_works() {
		parameter_types! {
			static TestData: Vec<usize> = vec![0];
		}

		let value = Default::default();
		let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
		let exec_ch = MockLoader::insert(Call, |_ctx, _executable| {
			TestData::mutate(|data| data.push(1));
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, exec_ch);
			let mut storage_meter =
				storage::meter::Meter::new(&Origin::from_account_id(ALICE), 0, value).unwrap();

			assert_matches!(
				MockStack::run_call(
					Origin::from_account_id(ALICE),
					BOB_ADDR,
					&mut gas_meter,
					&mut storage_meter,
					value,
					vec![],
					None,
				),
				Ok(_)
			);
		});

		assert_eq!(TestData::get(), vec![0, 1]);
	}

	#[test]
	fn transfer_works() {
		// This test verifies that a contract is able to transfer
		// some funds to another account.
		ExtBuilder::default().build().execute_with(|| {
			set_balance(&ALICE, 100);
			set_balance(&BOB, 0);

			MockStack::transfer(&ALICE, &BOB, 55).unwrap();

			assert_eq!(get_balance(&ALICE), 45);
			assert_eq!(get_balance(&BOB), 55);
		});
	}

	#[test]
	fn correct_transfer_on_call() {
		let value = 55;

		let success_ch = MockLoader::insert(Call, move |ctx, _| {
			assert_eq!(ctx.ext.value_transferred(), U256::from(value));
			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, success_ch);
			set_balance(&ALICE, 100);
			let balance = get_balance(&BOB_FALLBACK);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, value).unwrap();

			let _ = MockStack::run_call(
				origin.clone(),
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				value,
				vec![],
				None,
			)
			.unwrap();

			assert_eq!(get_balance(&ALICE), 100 - value);
			assert_eq!(get_balance(&BOB_FALLBACK), balance + value);
		});
	}

	#[test]
	fn correct_transfer_on_delegate_call() {
		let value = 35;

		let success_ch = MockLoader::insert(Call, move |ctx, _| {
			assert_eq!(ctx.ext.value_transferred(), U256::from(value));
			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
		});

		let delegate_ch = MockLoader::insert(Call, move |ctx, _| {
			assert_eq!(ctx.ext.value_transferred(), U256::from(value));
			let _ = ctx.ext.delegate_call(success_ch, Vec::new())?;
			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: Vec::new() })
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, delegate_ch);
			set_balance(&ALICE, 100);
			let balance = get_balance(&BOB_FALLBACK);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 55).unwrap();

			let _ = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				value,
				vec![],
				None,
			)
			.unwrap();

			assert_eq!(get_balance(&ALICE), 100 - value);
			assert_eq!(get_balance(&BOB_FALLBACK), balance + value);
		});
	}

	#[test]
	fn changes_are_reverted_on_failing_call() {
		// This test verifies that changes are reverted on a call which fails (or equally, returns
		// a non-zero status code).

		let return_ch = MockLoader::insert(Call, |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() })
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, return_ch);
			set_balance(&ALICE, 100);
			let balance = get_balance(&BOB);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 55).unwrap();

			let output = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				55,
				vec![],
				None,
			)
			.unwrap();

			assert!(output.did_revert());
			assert_eq!(get_balance(&ALICE), 100);
			assert_eq!(get_balance(&BOB), balance);
		});
	}

	#[test]
	fn balance_too_low() {
		// This test verifies that a contract can't send value if it's
		// balance is too low.
		let origin = ALICE;
		let dest = BOB;

		ExtBuilder::default().build().execute_with(|| {
			set_balance(&origin, 0);

			let result = MockStack::transfer(&origin, &dest, 100);

			assert_eq!(result, Err(Error::<Test>::TransferFailed.into()));
			assert_eq!(get_balance(&origin), 0);
			assert_eq!(get_balance(&dest), 0);
		});
	}

	#[test]
	fn output_is_returned_on_success() {
		// Verifies that if a contract returns data with a successful exit status, this data
		// is returned from the execution context.
		let return_ch = MockLoader::insert(Call, |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![1, 2, 3, 4] })
		});

		ExtBuilder::default().build().execute_with(|| {
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			place_contract(&BOB, return_ch);

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);

			let output = result.unwrap();
			assert!(!output.did_revert());
			assert_eq!(output.data, vec![1, 2, 3, 4]);
		});
	}

	#[test]
	fn output_is_returned_on_failure() {
		// Verifies that if a contract returns data with a failing exit status, this data
		// is returned from the execution context.
		let return_ch = MockLoader::insert(Call, |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![1, 2, 3, 4] })
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, return_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);

			let output = result.unwrap();
			assert!(output.did_revert());
			assert_eq!(output.data, vec![1, 2, 3, 4]);
		});
	}

	#[test]
	fn input_data_to_call() {
		let input_data_ch = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(ctx.input_data, &[1, 2, 3, 4]);
			exec_success()
		});

		// This one tests passing the input data into a contract via call.
		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, input_data_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![1, 2, 3, 4],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn input_data_to_instantiate() {
		let input_data_ch = MockLoader::insert(Constructor, |ctx, _| {
			assert_eq!(ctx.input_data, &[1, 2, 3, 4]);
			exec_success()
		});

		// This one tests passing the input data into a contract via instantiate.
		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				let executable =
					MockExecutable::from_storage(input_data_ch, &mut gas_meter).unwrap();
				set_balance(&ALICE, min_balance * 10_000);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, deposit_limit::<Test>(), min_balance)
						.unwrap();

				let result = MockStack::run_instantiate(
					ALICE,
					executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance,
					vec![1, 2, 3, 4],
					Some(&[0; 32]),
					None,
				);
				assert_matches!(result, Ok(_));
			});
	}

	#[test]
	fn max_depth() {
		// This test verifies that when we reach the maximal depth creation of an
		// yet another context fails.
		parameter_types! {
			static ReachedBottom: bool = false;
		}
		let value = Default::default();
		let recurse_ch = MockLoader::insert(Call, |ctx, _| {
			// Try to call into yourself.
			let r = ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&BOB_ADDR,
				U256::zero(),
				vec![],
				true,
				false,
			);

			ReachedBottom::mutate(|reached_bottom| {
				if !*reached_bottom {
					// We are first time here, it means we just reached bottom.
					// Verify that we've got proper error and set `reached_bottom`.
					assert_eq!(r, Err(Error::<Test>::MaxCallDepthReached.into()));
					*reached_bottom = true;
				} else {
					// We just unwinding stack here.
					assert_matches!(r, Ok(_));
				}
			});

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			set_balance(&BOB, 1);
			place_contract(&BOB, recurse_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, value).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				value,
				vec![],
				None,
			);

			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn caller_returns_proper_values() {
		parameter_types! {
			static WitnessedCallerBob: Option<H160> = None;
			static WitnessedCallerCharlie: Option<H160> = None;
		}

		let bob_ch = MockLoader::insert(Call, |ctx, _| {
			// Record the caller for bob.
			WitnessedCallerBob::mutate(|caller| {
				let origin = ctx.ext.caller();
				*caller =
					Some(<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(
						&origin.account_id().unwrap(),
					));
			});

			// Call into CHARLIE contract.
			assert_matches!(
				ctx.ext.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false
				),
				Ok(_)
			);
			exec_success()
		});
		let charlie_ch = MockLoader::insert(Call, |ctx, _| {
			// Record the caller for charlie.
			WitnessedCallerCharlie::mutate(|caller| {
				let origin = ctx.ext.caller();
				*caller =
					Some(<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(
						&origin.account_id().unwrap(),
					));
			});
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, bob_ch);
			place_contract(&CHARLIE, charlie_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);

			assert_matches!(result, Ok(_));
		});

		assert_eq!(WitnessedCallerBob::get(), Some(ALICE_ADDR));
		assert_eq!(WitnessedCallerCharlie::get(), Some(BOB_ADDR));
	}

	#[test]
	fn origin_returns_proper_values() {
		parameter_types! {
			static WitnessedCallerBob: Option<H160> = None;
			static WitnessedCallerCharlie: Option<H160> = None;
		}

		let bob_ch = MockLoader::insert(Call, |ctx, _| {
			// Record the origin for bob.
			WitnessedCallerBob::mutate(|witness| {
				let origin = ctx.ext.origin();
				*witness = Some(<Test as Config>::AddressMapper::to_address(
					&origin.account_id().unwrap(),
				));
			});

			// Call into CHARLIE contract.
			assert_matches!(
				ctx.ext.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false
				),
				Ok(_)
			);
			exec_success()
		});
		let charlie_ch = MockLoader::insert(Call, |ctx, _| {
			// Record the origin for charlie.
			WitnessedCallerCharlie::mutate(|witness| {
				let origin = ctx.ext.origin();
				*witness = Some(<Test as Config>::AddressMapper::to_address(
					&origin.account_id().unwrap(),
				));
			});
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, bob_ch);
			place_contract(&CHARLIE, charlie_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);

			assert_matches!(result, Ok(_));
		});

		assert_eq!(WitnessedCallerBob::get(), Some(ALICE_ADDR));
		assert_eq!(WitnessedCallerCharlie::get(), Some(ALICE_ADDR));
	}

	#[test]
	fn is_contract_returns_proper_values() {
		let bob_ch = MockLoader::insert(Call, |ctx, _| {
			// Verify that BOB is a contract
			assert!(ctx.ext.is_contract(&BOB_ADDR));
			// Verify that ALICE is not a contract
			assert!(!ctx.ext.is_contract(&ALICE_ADDR));
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, bob_ch);

			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn code_hash_returns_proper_values() {
		let bob_code_hash = MockLoader::insert(Call, |ctx, _| {
			// ALICE is not a contract but account exists so it returns hash of empty data
			assert_eq!(ctx.ext.code_hash(&ALICE_ADDR), EMPTY_CODE_HASH);
			// BOB is a contract (this function) and hence it has a code_hash.
			// `MockLoader` uses contract index to generate the code hash.
			assert_eq!(ctx.ext.code_hash(&BOB_ADDR), H256(keccak_256(&0u64.to_le_bytes())));
			// [0xff;20] doesn't exist and returns hash zero
			assert!(ctx.ext.code_hash(&H160([0xff; 20])).is_zero());

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			// add alice account info to test case EOA code hash
			frame_system::Account::<Test>::insert(
				<Test as Config>::AddressMapper::to_account_id(&ALICE_ADDR),
				AccountInfo { consumers: 1, providers: 1, ..Default::default() },
			);
			place_contract(&BOB, bob_code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			// ALICE (not contract) -> BOB (contract)
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn own_code_hash_returns_proper_values() {
		let bob_ch = MockLoader::insert(Call, |ctx, _| {
			let code_hash = ctx.ext.code_hash(&BOB_ADDR);
			assert_eq!(*ctx.ext.own_code_hash(), code_hash);
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, bob_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			// ALICE (not contract) -> BOB (contract)
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn caller_is_origin_returns_proper_values() {
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			// BOB is not the origin of the stack call
			assert!(!ctx.ext.caller_is_origin());
			exec_success()
		});

		let code_bob = MockLoader::insert(Call, |ctx, _| {
			// ALICE is the origin of the call stack
			assert!(ctx.ext.caller_is_origin());
			// BOB calls CHARLIE
			ctx.ext
				.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false,
				)
				.map(|_| ctx.ext.last_frame_output().clone())
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			// ALICE -> BOB (caller is origin) -> CHARLIE (caller is not origin)
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn root_caller_succeeds() {
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			// root is the origin of the call stack.
			assert!(ctx.ext.caller_is_root());
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			let origin = Origin::Root;
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			// root -> BOB (caller is root)
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn root_caller_does_not_succeed_when_value_not_zero() {
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			// root is the origin of the call stack.
			assert!(ctx.ext.caller_is_root());
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			let origin = Origin::Root;
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			// root -> BOB (caller is root)
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				1,
				vec![0],
				None,
			);
			assert_matches!(result, Err(_));
		});
	}

	#[test]
	fn root_caller_succeeds_with_consecutive_calls() {
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			// BOB is not root, even though the origin is root.
			assert!(!ctx.ext.caller_is_root());
			exec_success()
		});

		let code_bob = MockLoader::insert(Call, |ctx, _| {
			// root is the origin of the call stack.
			assert!(ctx.ext.caller_is_root());
			// BOB calls CHARLIE.
			ctx.ext
				.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false,
				)
				.map(|_| ctx.ext.last_frame_output().clone())
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::Root;
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			// root -> BOB (caller is root) -> CHARLIE (caller is not root)
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn address_returns_proper_values() {
		let bob_ch = MockLoader::insert(Call, |ctx, _| {
			// Verify that address matches BOB.
			assert_eq!(ctx.ext.address(), BOB_ADDR);

			// Call into charlie contract.
			assert_matches!(
				ctx.ext.call(
					Weight::zero(),
					U256::zero(),
					&CHARLIE_ADDR,
					U256::zero(),
					vec![],
					true,
					false
				),
				Ok(_)
			);
			exec_success()
		});
		let charlie_ch = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(ctx.ext.address(), CHARLIE_ADDR);
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, bob_ch);
			place_contract(&CHARLIE, charlie_ch);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);

			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn refuse_instantiate_with_value_below_existential_deposit() {
		let dummy_ch = MockLoader::insert(Constructor, |_, _| exec_success());

		ExtBuilder::default().existential_deposit(15).build().execute_with(|| {
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			let executable = MockExecutable::from_storage(dummy_ch, &mut gas_meter).unwrap();
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			assert_matches!(
				MockStack::run_instantiate(
					ALICE,
					executable,
					&mut gas_meter,
					&mut storage_meter,
					0, // <- zero value
					vec![],
					Some(&[0; 32]),
					None,
				),
				Err(_)
			);
		});
	}

	#[test]
	fn instantiation_work_with_success_output() {
		let dummy_ch = MockLoader::insert(Constructor, |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![80, 65, 83, 83] })
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				let executable = MockExecutable::from_storage(dummy_ch, &mut gas_meter).unwrap();
				set_balance(&ALICE, min_balance * 1000);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, min_balance * 100, min_balance).unwrap();

				let instantiated_contract_address = assert_matches!(
					MockStack::run_instantiate(
						ALICE,
						executable,
						&mut gas_meter,
						&mut storage_meter,

						min_balance,
						vec![],
						Some(&[0 ;32]),
						None,
					),
					Ok((address, ref output)) if output.data == vec![80, 65, 83, 83] => address
				);
				let instantiated_contract_id = <<Test as Config>::AddressMapper as AddressMapper<
					Test,
				>>::to_fallback_account_id(
					&instantiated_contract_address
				);

				// Check that the newly created account has the expected code hash and
				// there are instantiation event.
				assert_eq!(
					ContractInfo::<Test>::load_code_hash(&instantiated_contract_id).unwrap(),
					dummy_ch
				);
				assert_eq!(
					&events(),
					&[Event::Instantiated {
						deployer: ALICE_ADDR,
						contract: instantiated_contract_address
					}]
				);
			});
	}

	#[test]
	fn instantiation_fails_with_failing_output() {
		let dummy_ch = MockLoader::insert(Constructor, |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70, 65, 73, 76] })
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				let executable = MockExecutable::from_storage(dummy_ch, &mut gas_meter).unwrap();
				set_balance(&ALICE, min_balance * 1000);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, min_balance * 100, min_balance).unwrap();

				let instantiated_contract_address = assert_matches!(
					MockStack::run_instantiate(
						ALICE,
						executable,
						&mut gas_meter,
						&mut storage_meter,

						min_balance,
						vec![],
						Some(&[0; 32]),
						None,
					),
					Ok((address, ref output)) if output.data == vec![70, 65, 73, 76] => address
				);

				let instantiated_contract_id = <<Test as Config>::AddressMapper as AddressMapper<
					Test,
				>>::to_fallback_account_id(
					&instantiated_contract_address
				);

				// Check that the account has not been created.
				assert!(ContractInfo::<Test>::load_code_hash(&instantiated_contract_id).is_none());
				assert!(events().is_empty());
			});
	}

	#[test]
	fn instantiation_from_contract() {
		let dummy_ch = MockLoader::insert(Call, |_, _| exec_success());
		let instantiated_contract_address = Rc::new(RefCell::new(None::<H160>));
		let instantiator_ch = MockLoader::insert(Call, {
			let instantiated_contract_address = Rc::clone(&instantiated_contract_address);
			move |ctx, _| {
				// Instantiate a contract and save it's address in `instantiated_contract_address`.
				let (address, output) = ctx
					.ext
					.instantiate(
						Weight::zero(),
						U256::zero(),
						dummy_ch,
						<Test as Config>::Currency::minimum_balance().into(),
						vec![],
						Some(&[48; 32]),
					)
					.map(|address| (address, ctx.ext.last_frame_output().clone()))
					.unwrap();

				*instantiated_contract_address.borrow_mut() = Some(address);
				Ok(output)
			}
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				set_balance(&ALICE, min_balance * 100);
				place_contract(&BOB, instantiator_ch);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, min_balance * 10, min_balance * 10)
						.unwrap();

				assert_matches!(
					MockStack::run_call(
						origin,
						BOB_ADDR,
						&mut GasMeter::<Test>::new(GAS_LIMIT),
						&mut storage_meter,
						min_balance * 10,
						vec![],
						None,
					),
					Ok(_)
				);

				let instantiated_contract_address =
					*instantiated_contract_address.borrow().as_ref().unwrap();

				let instantiated_contract_id = <<Test as Config>::AddressMapper as AddressMapper<
					Test,
				>>::to_fallback_account_id(
					&instantiated_contract_address
				);

				// Check that the newly created account has the expected code hash and
				// there are instantiation event.
				assert_eq!(
					ContractInfo::<Test>::load_code_hash(&instantiated_contract_id).unwrap(),
					dummy_ch
				);
				assert_eq!(
					&events(),
					&[
						Event::Instantiated {
							deployer: BOB_ADDR,
							contract: instantiated_contract_address
						},
						Event::Called {
							caller: Origin::from_account_id(ALICE),
							contract: BOB_ADDR
						},
					]
				);
			});
	}

	#[test]
	fn instantiation_traps() {
		let dummy_ch = MockLoader::insert(Constructor, |_, _| Err("It's a trap!".into()));
		let instantiator_ch = MockLoader::insert(Call, {
			move |ctx, _| {
				// Instantiate a contract and save it's address in `instantiated_contract_address`.
				assert_matches!(
					ctx.ext.instantiate(
						Weight::zero(),
						U256::zero(),
						dummy_ch,
						<Test as Config>::Currency::minimum_balance().into(),
						vec![],
						Some(&[0; 32]),
					),
					Err(ExecError {
						error: DispatchError::Other("It's a trap!"),
						origin: ErrorOrigin::Callee,
					})
				);

				exec_success()
			}
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				set_balance(&ALICE, 1000);
				set_balance(&BOB_FALLBACK, 100);
				place_contract(&BOB, instantiator_ch);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

				assert_matches!(
					MockStack::run_call(
						origin,
						BOB_ADDR,
						&mut GasMeter::<Test>::new(GAS_LIMIT),
						&mut storage_meter,
						0,
						vec![],
						None,
					),
					Ok(_)
				);

				// The contract wasn't instantiated so we don't expect to see an instantiation
				// event here.
				assert_eq!(
					&events(),
					&[Event::Called { caller: Origin::from_account_id(ALICE), contract: BOB_ADDR },]
				);
			});
	}

	#[test]
	fn termination_from_instantiate_fails() {
		let terminate_ch = MockLoader::insert(Constructor, |ctx, _| {
			ctx.ext.terminate(&ALICE_ADDR).unwrap();
			exec_success()
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				let executable =
					MockExecutable::from_storage(terminate_ch, &mut gas_meter).unwrap();
				set_balance(&ALICE, 10_000);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 100).unwrap();

				assert_eq!(
					MockStack::run_instantiate(
						ALICE,
						executable,
						&mut gas_meter,
						&mut storage_meter,
						100,
						vec![],
						Some(&[0; 32]),
						None,
					),
					Err(Error::<Test>::TerminatedInConstructor.into())
				);

				assert_eq!(&events(), &[]);
			});
	}

	#[test]
	fn in_memory_changes_not_discarded() {
		// Call stack: BOB -> CHARLIE (trap) -> BOB' (success)
		// This tests verifies some edge case of the contract info cache:
		// We change some value in our contract info before calling into a contract
		// that calls into ourself. This triggers a case where BOBs contract info
		// is written to storage and invalidated by the successful execution of BOB'.
		// The trap of CHARLIE reverts the storage changes to BOB. When the root BOB regains
		// control it reloads its contract info from storage. We check that changes that
		// are made before calling into CHARLIE are not discarded.
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			if ctx.input_data[0] == 0 {
				let info = ctx.ext.contract_info();
				assert_eq!(info.storage_byte_deposit, 0);
				info.storage_byte_deposit = 42;
				assert_eq!(
					ctx.ext
						.call(
							Weight::zero(),
							U256::zero(),
							&CHARLIE_ADDR,
							U256::zero(),
							vec![],
							true,
							false
						)
						.map(|_| ctx.ext.last_frame_output().clone()),
					exec_trapped()
				);
				assert_eq!(ctx.ext.contract_info().storage_byte_deposit, 42);
			}
			exec_success()
		});
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			assert!(ctx
				.ext
				.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
				.is_ok());
			exec_trapped()
		});

		// This one tests passing the input data into a contract via call.
		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn recursive_call_during_constructor_is_balance_transfer() {
		let code = MockLoader::insert(Constructor, |ctx, _| {
			let account_id = ctx.ext.account_id().clone();
			let addr =
				<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(&account_id);
			let balance = ctx.ext.balance();

			// Calling ourselves during the constructor will trigger a balance
			// transfer since no contract exist yet.
			assert_ok!(ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&addr,
				(balance - 1).into(),
				vec![],
				true,
				false
			));

			// Should also work with call data set as it is ignored when no
			// contract is deployed.
			assert_ok!(ctx.ext.call(
				Weight::zero(),
				U256::zero(),
				&addr,
				1u32.into(),
				vec![1, 2, 3, 4],
				true,
				false
			));
			exec_success()
		});

		// This one tests passing the input data into a contract via instantiate.
		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				let executable = MockExecutable::from_storage(code, &mut gas_meter).unwrap();
				set_balance(&ALICE, min_balance * 10_000);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, deposit_limit::<Test>(), min_balance)
						.unwrap();

				let result = MockStack::run_instantiate(
					ALICE,
					executable,
					&mut gas_meter,
					&mut storage_meter,
					10,
					vec![],
					Some(&[0; 32]),
					None,
				);
				assert_matches!(result, Ok(_));
			});
	}

	#[test]
	fn cannot_send_more_balance_than_available_to_self() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			let account_id = ctx.ext.account_id().clone();
			let addr =
				<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_address(&account_id);
			let balance = ctx.ext.balance();

			assert_err!(
				ctx.ext.call(
					Weight::zero(),
					U256::zero(),
					&addr,
					(balance + 1).into(),
					vec![],
					true,
					false
				),
				<Error<Test>>::TransferFailed
			);
			exec_success()
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				set_balance(&ALICE, min_balance * 10);
				place_contract(&BOB, code_hash);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut gas_meter,
					&mut storage_meter,
					0,
					vec![],
					None,
				)
				.unwrap();
			});
	}

	#[test]
	fn printing_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			ctx.ext.append_debug_buffer("This is a test");
			ctx.ext.append_debug_buffer("More text");
			exec_success()
		});

		let mut debug_buffer = DebugBuffer::try_from(Vec::new()).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 10);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				Some(&mut debug_buffer),
			)
			.unwrap();
		});

		assert_eq!(&String::from_utf8(debug_buffer.to_vec()).unwrap(), "This is a testMore text");
	}

	#[test]
	fn printing_works_on_fail() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			ctx.ext.append_debug_buffer("This is a test");
			ctx.ext.append_debug_buffer("More text");
			exec_trapped()
		});

		let mut debug_buffer = DebugBuffer::try_from(Vec::new()).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 10);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				Some(&mut debug_buffer),
			);
			assert!(result.is_err());
		});

		assert_eq!(&String::from_utf8(debug_buffer.to_vec()).unwrap(), "This is a testMore text");
	}

	#[test]
	fn debug_buffer_is_limited() {
		let code_hash = MockLoader::insert(Call, move |ctx, _| {
			ctx.ext.append_debug_buffer("overflowing bytes");
			exec_success()
		});

		// Pre-fill the buffer almost up to its limit, leaving not enough space to the message
		let debug_buf_before = DebugBuffer::try_from(vec![0u8; DebugBuffer::bound() - 5]).unwrap();
		let mut debug_buf_after = debug_buf_before.clone();

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();
			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 10);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				Some(&mut debug_buf_after),
			)
			.unwrap();
			assert_eq!(debug_buf_before, debug_buf_after);
		});
	}

	#[test]
	fn call_reentry_direct_recursion() {
		// call the contract passed as input with disabled reentry
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			let dest = H160::from_slice(ctx.input_data.as_ref());
			ctx.ext
				.call(Weight::zero(), U256::zero(), &dest, U256::zero(), vec![], false, false)
				.map(|_| ctx.ext.last_frame_output().clone())
		});

		let code_charlie = MockLoader::insert(Call, |_, _| exec_success());

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			// Calling another contract should succeed
			assert_ok!(MockStack::run_call(
				origin.clone(),
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				CHARLIE_ADDR.as_bytes().to_vec(),
				None,
			));

			// Calling into oneself fails
			assert_err!(
				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					BOB_ADDR.as_bytes().to_vec(),
					None,
				)
				.map_err(|e| e.error),
				<Error<Test>>::ReentranceDenied,
			);
		});
	}

	#[test]
	fn call_deny_reentry() {
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			if ctx.input_data[0] == 0 {
				ctx.ext
					.call(
						Weight::zero(),
						U256::zero(),
						&CHARLIE_ADDR,
						U256::zero(),
						vec![],
						false,
						false,
					)
					.map(|_| ctx.ext.last_frame_output().clone())
			} else {
				exec_success()
			}
		});

		// call BOB with input set to '1'
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			ctx.ext
				.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![1], true, false)
				.map(|_| ctx.ext.last_frame_output().clone())
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			// BOB -> CHARLIE -> BOB fails as BOB denies reentry.
			assert_err!(
				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					vec![0],
					None,
				)
				.map_err(|e| e.error),
				<Error<Test>>::ReentranceDenied,
			);
		});
	}

	#[test]
	fn call_runtime_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			let call = RuntimeCall::System(frame_system::Call::remark_with_event {
				remark: b"Hello World".to_vec(),
			});
			ctx.ext.call_runtime(call).unwrap();
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 10);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			System::reset_events();
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			)
			.unwrap();

			let remark_hash = <Test as frame_system::Config>::Hashing::hash(b"Hello World");
			assert_eq!(
				System::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: MetaEvent::System(frame_system::Event::Remarked {
							sender: BOB_FALLBACK,
							hash: remark_hash
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: MetaEvent::Contracts(crate::Event::Called {
							caller: Origin::from_account_id(ALICE),
							contract: BOB_ADDR,
						}),
						topics: vec![],
					},
				]
			);
		});
	}

	#[test]
	fn call_runtime_filter() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			use frame_system::Call as SysCall;
			use pallet_balances::Call as BalanceCall;
			use pallet_utility::Call as UtilCall;

			// remark should still be allowed
			let allowed_call =
				RuntimeCall::System(SysCall::remark_with_event { remark: b"Hello".to_vec() });

			// transfers are disallowed by the `TestFiler` (see below)
			let forbidden_call = RuntimeCall::Balances(BalanceCall::transfer_allow_death {
				dest: CHARLIE,
				value: 22,
			});

			// simple cases: direct call
			assert_err!(
				ctx.ext.call_runtime(forbidden_call.clone()),
				frame_system::Error::<Test>::CallFiltered
			);

			// as part of a patch: return is OK (but it interrupted the batch)
			assert_ok!(ctx.ext.call_runtime(RuntimeCall::Utility(UtilCall::batch {
				calls: vec![allowed_call.clone(), forbidden_call, allowed_call]
			})),);

			// the transfer wasn't performed
			assert_eq!(get_balance(&CHARLIE), 0);

			exec_success()
		});

		TestFilter::set_filter(|call| match call {
			RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death { .. }) => false,
			_ => true,
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 10);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			System::reset_events();
			MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			)
			.unwrap();

			let remark_hash = <Test as frame_system::Config>::Hashing::hash(b"Hello");
			assert_eq!(
				System::events(),
				vec![
					EventRecord {
						phase: Phase::Initialization,
						event: MetaEvent::System(frame_system::Event::Remarked {
							sender: BOB_FALLBACK,
							hash: remark_hash
						}),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: MetaEvent::Utility(pallet_utility::Event::ItemCompleted),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: MetaEvent::Utility(pallet_utility::Event::BatchInterrupted {
							index: 1,
							error: frame_system::Error::<Test>::CallFiltered.into()
						},),
						topics: vec![],
					},
					EventRecord {
						phase: Phase::Initialization,
						event: MetaEvent::Contracts(crate::Event::Called {
							caller: Origin::from_account_id(ALICE),
							contract: BOB_ADDR,
						}),
						topics: vec![],
					},
				]
			);
		});
	}

	#[test]
	fn nonce() {
		let fail_code = MockLoader::insert(Constructor, |_, _| exec_trapped());
		let success_code = MockLoader::insert(Constructor, |_, _| exec_success());
		let succ_fail_code = MockLoader::insert(Constructor, move |ctx, _| {
			ctx.ext
				.instantiate(
					Weight::zero(),
					U256::zero(),
					fail_code,
					ctx.ext.minimum_balance() * 100,
					vec![],
					Some(&[0; 32]),
				)
				.ok();
			exec_success()
		});
		let succ_succ_code = MockLoader::insert(Constructor, move |ctx, _| {
			let alice_nonce = System::account_nonce(&ALICE);
			assert_eq!(System::account_nonce(ctx.ext.account_id()), 0);
			assert_eq!(ctx.ext.caller().account_id().unwrap(), &ALICE);
			let addr = ctx
				.ext
				.instantiate(
					Weight::zero(),
					U256::zero(),
					success_code,
					ctx.ext.minimum_balance() * 100,
					vec![],
					Some(&[0; 32]),
				)
				.unwrap();

			let account_id =
				<<Test as Config>::AddressMapper as AddressMapper<Test>>::to_fallback_account_id(
					&addr,
				);

			assert_eq!(System::account_nonce(&ALICE), alice_nonce);
			assert_eq!(System::account_nonce(ctx.ext.account_id()), 1);
			assert_eq!(System::account_nonce(&account_id), 0);

			// a plain call should not influence the account counter
			ctx.ext
				.call(Weight::zero(), U256::zero(), &addr, U256::zero(), vec![], false, false)
				.unwrap();

			assert_eq!(System::account_nonce(ALICE), alice_nonce);
			assert_eq!(System::account_nonce(ctx.ext.account_id()), 1);
			assert_eq!(System::account_nonce(&account_id), 0);

			exec_success()
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.build()
			.execute_with(|| {
				let min_balance = <Test as Config>::Currency::minimum_balance();
				let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
				let fail_executable =
					MockExecutable::from_storage(fail_code, &mut gas_meter).unwrap();
				let success_executable =
					MockExecutable::from_storage(success_code, &mut gas_meter).unwrap();
				let succ_fail_executable =
					MockExecutable::from_storage(succ_fail_code, &mut gas_meter).unwrap();
				let succ_succ_executable =
					MockExecutable::from_storage(succ_succ_code, &mut gas_meter).unwrap();
				set_balance(&ALICE, min_balance * 10_000);
				set_balance(&BOB, min_balance * 10_000);
				let origin = Origin::from_account_id(BOB);
				let mut storage_meter =
					storage::meter::Meter::new(&origin, deposit_limit::<Test>(), min_balance * 100)
						.unwrap();

				// fail should not increment
				MockStack::run_instantiate(
					ALICE,
					fail_executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance * 100,
					vec![],
					Some(&[0; 32]),
					None,
				)
				.ok();
				assert_eq!(System::account_nonce(&ALICE), 0);

				assert_ok!(MockStack::run_instantiate(
					ALICE,
					success_executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance * 100,
					vec![],
					Some(&[0; 32]),
					None,
				));
				assert_eq!(System::account_nonce(&ALICE), 1);

				assert_ok!(MockStack::run_instantiate(
					ALICE,
					succ_fail_executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance * 200,
					vec![],
					Some(&[0; 32]),
					None,
				));
				assert_eq!(System::account_nonce(&ALICE), 2);

				assert_ok!(MockStack::run_instantiate(
					ALICE,
					succ_succ_executable,
					&mut gas_meter,
					&mut storage_meter,
					min_balance * 200,
					vec![],
					Some(&[0; 32]),
					None,
				));
				assert_eq!(System::account_nonce(&ALICE), 3);
			});
	}

	#[test]
	fn set_storage_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			// Write
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![4, 5, 6]), true),
				Ok(WriteOutcome::New)
			);
			assert_eq!(ctx.ext.set_storage(&Key::Fix([3; 32]), None, false), Ok(WriteOutcome::New));
			assert_eq!(ctx.ext.set_storage(&Key::Fix([4; 32]), None, true), Ok(WriteOutcome::New));
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([5; 32]), Some(vec![]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([6; 32]), Some(vec![]), true),
				Ok(WriteOutcome::New)
			);

			// Overwrite
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![42]), false),
				Ok(WriteOutcome::Overwritten(3))
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![48]), true),
				Ok(WriteOutcome::Taken(vec![4, 5, 6]))
			);
			assert_eq!(ctx.ext.set_storage(&Key::Fix([3; 32]), None, false), Ok(WriteOutcome::New));
			assert_eq!(ctx.ext.set_storage(&Key::Fix([4; 32]), None, true), Ok(WriteOutcome::New));
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([5; 32]), Some(vec![]), false),
				Ok(WriteOutcome::Overwritten(0))
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([6; 32]), Some(vec![]), true),
				Ok(WriteOutcome::Taken(vec![]))
			);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 1000);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn set_storage_varsized_key_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			// Write
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([1; 64].to_vec()).unwrap(),
					Some(vec![1, 2, 3]),
					false
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([2; 19].to_vec()).unwrap(),
					Some(vec![4, 5, 6]),
					true
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::try_from_var([3; 19].to_vec()).unwrap(), None, false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::try_from_var([4; 64].to_vec()).unwrap(), None, true),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([5; 30].to_vec()).unwrap(),
					Some(vec![]),
					false
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([6; 128].to_vec()).unwrap(),
					Some(vec![]),
					true
				),
				Ok(WriteOutcome::New)
			);

			// Overwrite
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([1; 64].to_vec()).unwrap(),
					Some(vec![42, 43, 44]),
					false
				),
				Ok(WriteOutcome::Overwritten(3))
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([2; 19].to_vec()).unwrap(),
					Some(vec![48]),
					true
				),
				Ok(WriteOutcome::Taken(vec![4, 5, 6]))
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::try_from_var([3; 19].to_vec()).unwrap(), None, false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::try_from_var([4; 64].to_vec()).unwrap(), None, true),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([5; 30].to_vec()).unwrap(),
					Some(vec![]),
					false
				),
				Ok(WriteOutcome::Overwritten(0))
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([6; 128].to_vec()).unwrap(),
					Some(vec![]),
					true
				),
				Ok(WriteOutcome::Taken(vec![]))
			);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 1000);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn get_storage_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(ctx.ext.get_storage(&Key::Fix([1; 32])), Some(vec![1, 2, 3]));
			assert_eq!(ctx.ext.get_storage(&Key::Fix([2; 32])), Some(vec![]));
			assert_eq!(ctx.ext.get_storage(&Key::Fix([3; 32])), None);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 1000);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn get_storage_size_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(&Key::Fix([2; 32]), Some(vec![]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(ctx.ext.get_storage_size(&Key::Fix([1; 32])), Some(3));
			assert_eq!(ctx.ext.get_storage_size(&Key::Fix([2; 32])), Some(0));
			assert_eq!(ctx.ext.get_storage_size(&Key::Fix([3; 32])), None);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 1000);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn get_storage_varsized_key_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([1; 19].to_vec()).unwrap(),
					Some(vec![1, 2, 3]),
					false
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([2; 16].to_vec()).unwrap(),
					Some(vec![]),
					false
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.get_storage(&Key::try_from_var([1; 19].to_vec()).unwrap()),
				Some(vec![1, 2, 3])
			);
			assert_eq!(
				ctx.ext.get_storage(&Key::try_from_var([2; 16].to_vec()).unwrap()),
				Some(vec![])
			);
			assert_eq!(ctx.ext.get_storage(&Key::try_from_var([3; 8].to_vec()).unwrap()), None);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 1000);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn get_storage_size_varsized_key_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([1; 19].to_vec()).unwrap(),
					Some(vec![1, 2, 3]),
					false
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_storage(
					&Key::try_from_var([2; 16].to_vec()).unwrap(),
					Some(vec![]),
					false
				),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.get_storage_size(&Key::try_from_var([1; 19].to_vec()).unwrap()),
				Some(3)
			);
			assert_eq!(
				ctx.ext.get_storage_size(&Key::try_from_var([2; 16].to_vec()).unwrap()),
				Some(0)
			);
			assert_eq!(
				ctx.ext.get_storage_size(&Key::try_from_var([3; 8].to_vec()).unwrap()),
				None
			);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			let min_balance = <Test as Config>::Currency::minimum_balance();

			let mut gas_meter = GasMeter::<Test>::new(GAS_LIMIT);
			set_balance(&ALICE, min_balance * 1000);
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut gas_meter,
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn set_transient_storage_works() {
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			// Write
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([1; 32]), Some(vec![1, 2, 3]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([2; 32]), Some(vec![4, 5, 6]), true),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([3; 32]), None, false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([4; 32]), None, true),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([5; 32]), Some(vec![]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([6; 32]), Some(vec![]), true),
				Ok(WriteOutcome::New)
			);

			// Overwrite
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([1; 32]), Some(vec![42]), false),
				Ok(WriteOutcome::Overwritten(3))
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([2; 32]), Some(vec![48]), true),
				Ok(WriteOutcome::Taken(vec![4, 5, 6]))
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([3; 32]), None, false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([4; 32]), None, true),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([5; 32]), Some(vec![]), false),
				Ok(WriteOutcome::Overwritten(0))
			);
			assert_eq!(
				ctx.ext.set_transient_storage(&Key::Fix([6; 32]), Some(vec![]), true),
				Ok(WriteOutcome::Taken(vec![]))
			);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter =
				storage::meter::Meter::new(&origin, deposit_limit::<Test>(), 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn get_transient_storage_works() {
		// Call stack: BOB -> CHARLIE(success) -> BOB' (success)
		let storage_key_1 = &Key::Fix([1; 32]);
		let storage_key_2 = &Key::Fix([2; 32]);
		let storage_key_3 = &Key::Fix([3; 32]);
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			if ctx.input_data[0] == 0 {
				assert_eq!(
					ctx.ext.set_transient_storage(storage_key_1, Some(vec![1, 2]), false),
					Ok(WriteOutcome::New)
				);
				assert_eq!(
					ctx.ext
						.call(
							Weight::zero(),
							U256::zero(),
							&CHARLIE_ADDR,
							U256::zero(),
							vec![],
							true,
							false,
						)
						.map(|_| ctx.ext.last_frame_output().clone()),
					exec_success()
				);
				assert_eq!(ctx.ext.get_transient_storage(storage_key_1), Some(vec![3]));
				assert_eq!(ctx.ext.get_transient_storage(storage_key_2), Some(vec![]));
				assert_eq!(ctx.ext.get_transient_storage(storage_key_3), None);
			} else {
				assert_eq!(
					ctx.ext.set_transient_storage(storage_key_1, Some(vec![3]), true),
					Ok(WriteOutcome::Taken(vec![1, 2]))
				);
				assert_eq!(
					ctx.ext.set_transient_storage(storage_key_2, Some(vec![]), false),
					Ok(WriteOutcome::New)
				);
			}
			exec_success()
		});
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			assert!(ctx
				.ext
				.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
				.is_ok());
			// CHARLIE can not read BOB`s storage.
			assert_eq!(ctx.ext.get_transient_storage(storage_key_1), None);
			exec_success()
		});

		// This one tests passing the input data into a contract via call.
		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn get_transient_storage_size_works() {
		let storage_key_1 = &Key::Fix([1; 32]);
		let storage_key_2 = &Key::Fix([2; 32]);
		let storage_key_3 = &Key::Fix([3; 32]);
		let code_hash = MockLoader::insert(Call, |ctx, _| {
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key_1, Some(vec![1, 2, 3]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(
				ctx.ext.set_transient_storage(storage_key_2, Some(vec![]), false),
				Ok(WriteOutcome::New)
			);
			assert_eq!(ctx.ext.get_transient_storage_size(storage_key_1), Some(3));
			assert_eq!(ctx.ext.get_transient_storage_size(storage_key_2), Some(0));
			assert_eq!(ctx.ext.get_transient_storage_size(storage_key_3), None);

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_hash);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			assert_ok!(MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			));
		});
	}

	#[test]
	fn rollback_transient_storage_works() {
		// Call stack: BOB -> CHARLIE (trap) -> BOB' (success)
		let storage_key = &Key::Fix([1; 32]);
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			if ctx.input_data[0] == 0 {
				assert_eq!(
					ctx.ext.set_transient_storage(storage_key, Some(vec![1, 2]), false),
					Ok(WriteOutcome::New)
				);
				assert_eq!(
					ctx.ext
						.call(
							Weight::zero(),
							U256::zero(),
							&CHARLIE_ADDR,
							U256::zero(),
							vec![],
							true,
							false
						)
						.map(|_| ctx.ext.last_frame_output().clone()),
					exec_trapped()
				);
				assert_eq!(ctx.ext.get_transient_storage(storage_key), Some(vec![1, 2]));
			} else {
				let overwritten_length = ctx.ext.get_transient_storage_size(storage_key).unwrap();
				assert_eq!(
					ctx.ext.set_transient_storage(storage_key, Some(vec![3]), false),
					Ok(WriteOutcome::Overwritten(overwritten_length))
				);
				assert_eq!(ctx.ext.get_transient_storage(storage_key), Some(vec![3]));
			}
			exec_success()
		});
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			assert!(ctx
				.ext
				.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
				.is_ok());
			exec_trapped()
		});

		// This one tests passing the input data into a contract via call.
		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn ecdsa_to_eth_address_returns_proper_value() {
		let bob_ch = MockLoader::insert(Call, |ctx, _| {
			let pubkey_compressed = array_bytes::hex2array_unchecked(
				"028db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd91",
			);
			assert_eq!(
				ctx.ext.ecdsa_to_eth_address(&pubkey_compressed).unwrap(),
				array_bytes::hex2array_unchecked::<_, 20>(
					"09231da7b19A016f9e576d23B16277062F4d46A8"
				)
			);
			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, bob_ch);

			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();
			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn last_frame_output_works_on_instantiate() {
		let ok_ch = MockLoader::insert(Constructor, move |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] })
		});
		let revert_ch = MockLoader::insert(Constructor, move |_, _| {
			Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] })
		});
		let trap_ch = MockLoader::insert(Constructor, |_, _| Err("It's a trap!".into()));
		let instantiator_ch = MockLoader::insert(Call, {
			move |ctx, _| {
				let value = <Test as Config>::Currency::minimum_balance().into();

				// Successful instantiation should set the output
				let address = ctx
					.ext
					.instantiate(Weight::zero(), U256::zero(), ok_ch, value, vec![], None)
					.unwrap();
				assert_eq!(
					ctx.ext.last_frame_output(),
					&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] }
				);

				// Plain transfers should not set the output
				ctx.ext.transfer(&address, U256::from(1)).unwrap();
				assert_eq!(
					ctx.ext.last_frame_output(),
					&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] }
				);

				// Reverted instantiation should set the output
				ctx.ext
					.instantiate(Weight::zero(), U256::zero(), revert_ch, value, vec![], None)
					.unwrap();
				assert_eq!(
					ctx.ext.last_frame_output(),
					&ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] }
				);

				// Trapped instantiation should clear the output
				ctx.ext
					.instantiate(Weight::zero(), U256::zero(), trap_ch, value, vec![], None)
					.unwrap_err();
				assert_eq!(
					ctx.ext.last_frame_output(),
					&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![] }
				);

				exec_success()
			}
		});

		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				set_balance(&ALICE, 1000);
				set_balance(&BOB, 100);
				place_contract(&BOB, instantiator_ch);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					vec![],
					None,
				)
				.unwrap()
			});
	}

	#[test]
	fn last_frame_output_works_on_nested_call() {
		// Call stack: BOB -> CHARLIE(revert) -> BOB' (success)
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			if ctx.input_data.is_empty() {
				// We didn't do anything yet
				assert_eq!(
					ctx.ext.last_frame_output(),
					&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![] }
				);

				ctx.ext
					.call(
						Weight::zero(),
						U256::zero(),
						&CHARLIE_ADDR,
						U256::zero(),
						vec![],
						true,
						false,
					)
					.unwrap();
				assert_eq!(
					ctx.ext.last_frame_output(),
					&ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] }
				);
			}

			Ok(ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] })
		});
		let code_charlie = MockLoader::insert(Call, |ctx, _| {
			// We didn't do anything yet
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![] }
			);

			assert!(ctx
				.ext
				.call(Weight::zero(), U256::zero(), &BOB_ADDR, U256::zero(), vec![99], true, false)
				.is_ok());
			assert_eq!(
				ctx.ext.last_frame_output(),
				&ExecReturnValue { flags: ReturnFlags::empty(), data: vec![127] }
			);

			Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![70] })
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			place_contract(&CHARLIE, code_charlie);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![0],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn last_frame_output_is_always_reset() {
		let code_bob = MockLoader::insert(Call, |ctx, _| {
			let invalid_code_hash = H256::from_low_u64_le(u64::MAX);
			let output_revert = || ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![1] };

			// A value of u256::MAX to fail the call on the first condition.
			*ctx.ext.last_frame_output_mut() = output_revert();
			assert_eq!(
				ctx.ext.call(
					Weight::zero(),
					U256::zero(),
					&H160::zero(),
					U256::max_value(),
					vec![],
					true,
					false,
				),
				Err(Error::<Test>::BalanceConversionFailed.into())
			);
			assert_eq!(ctx.ext.last_frame_output(), &Default::default());

			// An unknown code hash to fail the delegate_call on the first condition.
			*ctx.ext.last_frame_output_mut() = output_revert();
			assert_eq!(
				ctx.ext.delegate_call(invalid_code_hash, Default::default()),
				Err(Error::<Test>::CodeNotFound.into())
			);
			assert_eq!(ctx.ext.last_frame_output(), &Default::default());

			// An unknown code hash to fail instantiation on the first condition.
			*ctx.ext.last_frame_output_mut() = output_revert();
			assert_eq!(
				ctx.ext.instantiate(
					Weight::zero(),
					U256::zero(),
					invalid_code_hash,
					U256::zero(),
					vec![],
					None,
				),
				Err(Error::<Test>::CodeNotFound.into())
			);
			assert_eq!(ctx.ext.last_frame_output(), &Default::default());

			exec_success()
		});

		ExtBuilder::default().build().execute_with(|| {
			place_contract(&BOB, code_bob);
			let origin = Origin::from_account_id(ALICE);
			let mut storage_meter = storage::meter::Meter::new(&origin, 0, 0).unwrap();

			let result = MockStack::run_call(
				origin,
				BOB_ADDR,
				&mut GasMeter::<Test>::new(GAS_LIMIT),
				&mut storage_meter,
				0,
				vec![],
				None,
			);
			assert_matches!(result, Ok(_));
		});
	}

	#[test]
	fn immutable_data_access_checks_work() {
		let dummy_ch = MockLoader::insert(Constructor, move |ctx, _| {
			// Calls can not store immutable data
			assert_eq!(
				ctx.ext.get_immutable_data(),
				Err(Error::<Test>::InvalidImmutableAccess.into())
			);
			exec_success()
		});
		let instantiator_ch = MockLoader::insert(Call, {
			move |ctx, _| {
				let value = <Test as Config>::Currency::minimum_balance().into();

				assert_eq!(
					ctx.ext.set_immutable_data(vec![0, 1, 2, 3].try_into().unwrap()),
					Err(Error::<Test>::InvalidImmutableAccess.into())
				);

				// Constructors can not access the immutable data
				ctx.ext
					.instantiate(Weight::zero(), U256::zero(), dummy_ch, value, vec![], None)
					.unwrap();

				exec_success()
			}
		});
		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				set_balance(&ALICE, 1000);
				set_balance(&BOB, 100);
				place_contract(&BOB, instantiator_ch);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					vec![],
					None,
				)
				.unwrap()
			});
	}

	#[test]
	fn correct_immutable_data_in_delegate_call() {
		let charlie_ch = MockLoader::insert(Call, |ctx, _| {
			Ok(ExecReturnValue {
				flags: ReturnFlags::empty(),
				data: ctx.ext.get_immutable_data()?.to_vec(),
			})
		});
		let bob_ch = MockLoader::insert(Call, move |ctx, _| {
			// In a regular call, we should witness the callee immutable data
			assert_eq!(
				ctx.ext
					.call(
						Weight::zero(),
						U256::zero(),
						&CHARLIE_ADDR,
						U256::zero(),
						vec![],
						true,
						false,
					)
					.map(|_| ctx.ext.last_frame_output().data.clone()),
				Ok(vec![2]),
			);

			// In a delegate call, we should witness the caller immutable data
			assert_eq!(
				ctx.ext.delegate_call(charlie_ch, Vec::new()).map(|_| ctx
					.ext
					.last_frame_output()
					.data
					.clone()),
				Ok(vec![1])
			);

			exec_success()
		});
		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				place_contract(&BOB, bob_ch);
				place_contract(&CHARLIE, charlie_ch);

				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

				// Place unique immutable data for each contract
				<ImmutableDataOf<Test>>::insert::<_, ImmutableData>(
					BOB_ADDR,
					vec![1].try_into().unwrap(),
				);
				<ImmutableDataOf<Test>>::insert::<_, ImmutableData>(
					CHARLIE_ADDR,
					vec![2].try_into().unwrap(),
				);

				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					vec![],
					None,
				)
				.unwrap()
			});
	}

	#[test]
	fn immutable_data_set_works_only_once() {
		let dummy_ch = MockLoader::insert(Constructor, move |ctx, _| {
			// Calling `set_immutable_data` the first time should work
			assert_ok!(ctx.ext.set_immutable_data(vec![0, 1, 2, 3].try_into().unwrap()));
			// Calling `set_immutable_data` the second time should error out
			assert_eq!(
				ctx.ext.set_immutable_data(vec![0, 1, 2, 3].try_into().unwrap()),
				Err(Error::<Test>::InvalidImmutableAccess.into())
			);
			exec_success()
		});
		let instantiator_ch = MockLoader::insert(Call, {
			move |ctx, _| {
				let value = <Test as Config>::Currency::minimum_balance().into();
				ctx.ext
					.instantiate(Weight::zero(), U256::zero(), dummy_ch, value, vec![], None)
					.unwrap();

				exec_success()
			}
		});
		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				set_balance(&ALICE, 1000);
				set_balance(&BOB, 100);
				place_contract(&BOB, instantiator_ch);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					vec![],
					None,
				)
				.unwrap()
			});
	}

	#[test]
	fn immutable_data_set_errors_with_empty_data() {
		let dummy_ch = MockLoader::insert(Constructor, move |ctx, _| {
			// Calling `set_immutable_data` with empty data should error out
			assert_eq!(
				ctx.ext.set_immutable_data(Default::default()),
				Err(Error::<Test>::InvalidImmutableAccess.into())
			);
			exec_success()
		});
		let instantiator_ch = MockLoader::insert(Call, {
			move |ctx, _| {
				let value = <Test as Config>::Currency::minimum_balance().into();
				ctx.ext
					.instantiate(Weight::zero(), U256::zero(), dummy_ch, value, vec![], None)
					.unwrap();

				exec_success()
			}
		});
		ExtBuilder::default()
			.with_code_hashes(MockLoader::code_hashes())
			.existential_deposit(15)
			.build()
			.execute_with(|| {
				set_balance(&ALICE, 1000);
				set_balance(&BOB, 100);
				place_contract(&BOB, instantiator_ch);
				let origin = Origin::from_account_id(ALICE);
				let mut storage_meter = storage::meter::Meter::new(&origin, 200, 0).unwrap();

				MockStack::run_call(
					origin,
					BOB_ADDR,
					&mut GasMeter::<Test>::new(GAS_LIMIT),
					&mut storage_meter,
					0,
					vec![],
					None,
				)
				.unwrap()
			});
	}
}
