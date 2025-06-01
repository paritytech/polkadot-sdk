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
	gas::GasMeter,
	limits,
	precompiles::{All as AllPrecompiles, Instance as PrecompileInstance, Precompiles},
	primitives::{ExecReturnValue, StorageDeposit},
	runtime_decl_for_revive_api::{Decode, Encode, RuntimeDebugNoBound, TypeInfo},
	storage::{self, meter::Diff, WriteOutcome},
	tracing::if_tracing,
	transient_storage::TransientStorage,
	BalanceOf, CodeInfo, CodeInfoOf, Config, ContractInfo, ContractInfoOf, ConversionPrecision,
	Error, Event, ImmutableData, ImmutableDataOf, NonceAlreadyIncremented, Pallet as Contracts,
};
use alloc::vec::Vec;
use core::{fmt::Debug, marker::PhantomData, mem};
use frame_support::{
	crypto::ecdsa::ECDSAExt,
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	storage::{with_transaction, TransactionOutcome},
	traits::{
		fungible::{Inspect, Mutate},
		tokens::{Fortitude, Preservation},
		Contains, FindAuthor, OriginTrait, Time,
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
	ConstU32, H160, H256, U256,
};
use sp_io::{crypto::secp256k1_ecdsa_recover_compressed, hashing::blake2_256};
use sp_runtime::{
	traits::{BadOrigin, Bounded, Convert, Dispatchable, Saturating, Zero},
	DispatchError, SaturatedConversion,
};

#[cfg(test)]
mod tests;

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

/// Environment functions only available to host functions.
pub trait Ext: PrecompileWithInfoExt {
	/// Execute code in the current frame.
	///
	/// Returns the code size of the called contract.
	fn delegate_call(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		address: H160,
		input_data: Vec<u8>,
	) -> Result<(), ExecError>;

	/// Transfer all funds to `beneficiary` and delete the contract.
	///
	/// Since this function removes the self contract eagerly, if succeeded, no further actions
	/// should be performed on this `Ext` instance.
	///
	/// This function will fail if the same contract is present on the contract
	/// call stack.
	fn terminate(&mut self, beneficiary: &H160) -> DispatchResult;

	/// Returns the code hash of the contract being executed.
	fn own_code_hash(&mut self) -> &H256;

	/// Sets new code hash and immutable data for an existing contract.
	fn set_code_hash(&mut self, hash: H256) -> DispatchResult;

	/// Get the length of the immutable data.
	///
	/// This query is free as it does not need to load the immutable data from storage.
	/// Useful when we need a constant time lookup of the length.
	fn immutable_data_len(&mut self) -> u32;

	/// Returns the immutable data of the current contract.
	///
	/// Returns `Err(InvalidImmutableAccess)` if called from a constructor.
	fn get_immutable_data(&mut self) -> Result<ImmutableData, DispatchError>;

	/// Set the immutable data of the current contract.
	///
	/// Returns `Err(InvalidImmutableAccess)` if not called from a constructor.
	///
	/// Note: Requires &mut self to access the contract info.
	fn set_immutable_data(&mut self, data: ImmutableData) -> Result<(), DispatchError>;

	/// Call some dispatchable and return the result.
	fn call_runtime(&self, call: <Self::T as Config>::RuntimeCall) -> DispatchResultWithPostInfo;
}

/// Environment functions which are available to pre-compiles with `HAS_CONTRACT_INFO = true`.
pub trait PrecompileWithInfoExt: PrecompileExt {
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

	/// Charges `diff` from the meter.
	fn charge_storage(&mut self, diff: &Diff);

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
}

/// Environment functions which are available to all pre-compiles.
pub trait PrecompileExt: sealing::Sealed {
	type T: Config;

	/// Charges the gas meter with the given weight.
	fn charge(&mut self, weight: Weight) -> Result<crate::gas::ChargedAmount, DispatchError> {
		self.gas_meter_mut().charge(crate::RuntimeCosts::Precompile(weight))
	}

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

	/// Returns the account id for the given `address`.
	fn to_account_id(&self, address: &H160) -> AccountIdOf<Self::T>;

	/// Returns the code hash of the contract for the given `address`.
	/// If not a contract but account exists then `keccak_256([])` is returned, otherwise `zero`.
	fn code_hash(&self, address: &H160) -> H256;

	/// Returns the code size of the contract at the given `address` or zero.
	fn code_size(&self, address: &H160) -> u64;

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

	/// Returns the timestamp of the current block in seconds.
	fn now(&self) -> U256;

	/// Returns the minimum balance that is required for creating an account.
	fn minimum_balance(&self) -> U256;

	/// Deposit an event with the given topics.
	///
	/// There should not be any duplicates in `topics`.
	fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>);

	/// Returns the current block number.
	fn block_number(&self) -> U256;

	/// Returns the block hash at the given `block_number` or `None` if
	/// `block_number` isn't within the range of the previous 256 blocks.
	fn block_hash(&self, block_number: U256) -> Option<H256>;

	/// Returns the author of the current block.
	fn block_author(&self) -> Option<AccountIdOf<Self::T>>;

	/// Returns the maximum allowed size of a storage item.
	fn max_value_size(&self) -> u32;

	/// Returns the price for the specified amount of weight.
	fn get_weight_price(&self, weight: Weight) -> U256;

	/// Get an immutable reference to the nested gas meter.
	fn gas_meter(&self) -> &GasMeter<Self::T>;

	/// Get a mutable reference to the nested gas meter.
	fn gas_meter_mut(&mut self) -> &mut GasMeter<Self::T>;

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
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn transient_storage(&mut self) -> &mut TransientStorage<Self::T>;

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
	/// Transient storage used to store data, which is kept for the duration of a transaction.
	transient_storage: TransientStorage<T>,
	/// Whether or not actual transfer of funds should be performed.
	/// This is set to `true` exclusively when we simulate a call through eth_transact.
	skip_transfer: bool,
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
	/// The EVM balance transferred by the caller as part of the call.
	value_transferred: U256,
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
	/// The delegate call info of the currently executing frame which was spawned by
	/// `delegate_call`.
	delegate: Option<DelegateInfo<T>>,
	/// The output of the last executed call frame.
	last_frame_output: ExecReturnValue,
}

/// This structure is used to represent the arguments in a delegate call frame in order to
/// distinguish who delegated the call and where it was delegated to.
struct DelegateInfo<T: Config> {
	/// The caller of the contract.
	pub caller: Origin<T>,
	/// The address of the contract the call was delegated to.
	pub callee: H160,
}

/// When calling an address it can either lead to execution of contract code or a pre-compile.
enum ExecutableOrPrecompile<T: Config, E: Executable<T>, Env> {
	/// Contract code.
	Executable(E),
	/// Code inside the runtime (so called pre-compile).
	Precompile { instance: PrecompileInstance<Env>, _phantom: PhantomData<T> },
}

impl<T: Config, E: Executable<T>, Env> ExecutableOrPrecompile<T, E, Env> {
	fn as_executable(&self) -> Option<&E> {
		if let Self::Executable(executable) = self {
			Some(executable)
		} else {
			None
		}
	}

	fn as_precompile(&self) -> Option<&PrecompileInstance<Env>> {
		if let Self::Precompile { instance, .. } = self {
			Some(instance)
		} else {
			None
		}
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn into_executable(self) -> Option<E> {
		if let Self::Executable(executable) = self {
			Some(executable)
		} else {
			None
		}
	}
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
		delegated_call: Option<DelegateInfo<T>>,
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
		nonce_already_incremented: NonceAlreadyIncremented,
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
	/// The frame is associated with pre-compile that has no contract info.
	None,
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

	/// Set the status to invalidate if is cached.
	fn invalidate(&mut self) {
		if matches!(self, CachedContract::Cached(_)) {
			*self = CachedContract::Invalidated;
		}
	}
}

impl<'a, T, E> Stack<'a, T, E>
where
	T: Config,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	E: Executable<T>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	/// Create and run a new call stack by calling into `dest`.
	///
	/// # Return Value
	///
	/// Result<(ExecReturnValue, CodeSize), (ExecError, CodeSize)>
	pub fn run_call(
		origin: Origin<T>,
		dest: H160,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: U256,
		input_data: Vec<u8>,
		skip_transfer: bool,
	) -> ExecResult {
		let dest = T::AddressMapper::to_account_id(&dest);
		if let Some((mut stack, executable)) = Self::new(
			FrameArgs::Call { dest: dest.clone(), cached_info: None, delegated_call: None },
			origin.clone(),
			gas_meter,
			storage_meter,
			value,
			skip_transfer,
		)? {
			stack.run(executable, input_data).map(|_| stack.first_frame.last_frame_output)
		} else {
			let result = Self::transfer_from_origin(&origin, &origin, &dest, value);
			if_tracing(|t| {
				t.enter_child_span(
					origin.account_id().map(T::AddressMapper::to_address).unwrap_or_default(),
					T::AddressMapper::to_address(&dest),
					false,
					false,
					value,
					&input_data,
					Weight::zero(),
				);
				match result {
					Ok(ref output) => t.exit_child_span(&output, Weight::zero()),
					Err(e) => t.exit_child_span_with_error(e.error.into(), Weight::zero()),
				}
			});

			result
		}
	}

	/// Create and run a new call stack by instantiating a new contract.
	///
	/// # Return Value
	///
	/// Result<(NewContractAccountId, ExecReturnValue), ExecError)>
	pub fn run_instantiate(
		origin: T::AccountId,
		executable: E,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: U256,
		input_data: Vec<u8>,
		salt: Option<&[u8; 32]>,
		skip_transfer: bool,
		nonce_already_incremented: NonceAlreadyIncremented,
	) -> Result<(H160, ExecReturnValue), ExecError> {
		let (mut stack, executable) = Self::new(
			FrameArgs::Instantiate {
				sender: origin.clone(),
				executable,
				salt,
				input_data: input_data.as_ref(),
				nonce_already_incremented,
			},
			Origin::from_account_id(origin),
			gas_meter,
			storage_meter,
			value,
			skip_transfer,
		)?
		.expect(FRAME_ALWAYS_EXISTS_ON_INSTANTIATE);
		let address = T::AddressMapper::to_address(&stack.top_frame().account_id);
		stack
			.run(executable, input_data)
			.map(|_| (address, stack.first_frame.last_frame_output))
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub fn bench_new_call(
		dest: H160,
		origin: Origin<T>,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: BalanceOf<T>,
	) -> (Self, E) {
		let call = Self::new(
			FrameArgs::Call {
				dest: T::AddressMapper::to_account_id(&dest),
				cached_info: None,
				delegated_call: None,
			},
			origin,
			gas_meter,
			storage_meter,
			value.into(),
			false,
		)
		.unwrap()
		.unwrap();
		(call.0, call.1.into_executable().unwrap())
	}

	/// Create a new call stack.
	///
	/// Returns `None` when calling a non existent contract. This is not an error case
	/// since this will result in a value transfer.
	fn new(
		args: FrameArgs<T, E>,
		origin: Origin<T>,
		gas_meter: &'a mut GasMeter<T>,
		storage_meter: &'a mut storage::meter::Meter<T>,
		value: U256,
		skip_transfer: bool,
	) -> Result<Option<(Self, ExecutableOrPrecompile<T, E, Self>)>, ExecError> {
		origin.ensure_mapped()?;
		let Some((first_frame, executable)) = Self::new_frame(
			args,
			value,
			gas_meter,
			Weight::max_value(),
			storage_meter,
			BalanceOf::<T>::max_value(),
			false,
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
			transient_storage: TransientStorage::new(limits::TRANSIENT_STORAGE_BYTES),
			skip_transfer,
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
		value_transferred: U256,
		gas_meter: &mut GasMeter<T>,
		gas_limit: Weight,
		storage_meter: &mut storage::meter::GenericMeter<T, S>,
		deposit_limit: BalanceOf<T>,
		read_only: bool,
	) -> Result<Option<(Frame<T>, ExecutableOrPrecompile<T, E, Self>)>, ExecError> {
		let (account_id, contract_info, executable, delegate, entry_point) = match frame_args {
			FrameArgs::Call { dest, cached_info, delegated_call } => {
				let address = T::AddressMapper::to_address(&dest);
				let precompile = <AllPrecompiles<T>>::get(address.as_fixed_bytes());

				// which contract info to load is unaffected by the fact if this
				// is a delegate call or not
				let mut contract = match (cached_info, &precompile) {
					(Some(info), _) => CachedContract::Cached(info),
					(None, None) =>
						if let Some(info) = <ContractInfoOf<T>>::get(&address) {
							CachedContract::Cached(info)
						} else {
							return Ok(None)
						},
					(None, Some(precompile)) if precompile.has_contract_info() => {
						if let Some(info) = <ContractInfoOf<T>>::get(&address) {
							CachedContract::Cached(info)
						} else {
							let info = ContractInfo::new(&address, 0u32.into(), H256::zero())?;
							CachedContract::Cached(info)
						}
					},
					(None, Some(_)) => CachedContract::None,
				};

				// in case of delegate the executable is not the one at `address`
				let executable = if let Some(delegated_call) = &delegated_call {
					if let Some(precompile) =
						<AllPrecompiles<T>>::get(delegated_call.callee.as_fixed_bytes())
					{
						ExecutableOrPrecompile::Precompile {
							instance: precompile,
							_phantom: Default::default(),
						}
					} else {
						let Some(info) = ContractInfoOf::<T>::get(&delegated_call.callee) else {
							return Ok(None);
						};
						let executable = E::from_storage(info.code_hash, gas_meter)?;
						ExecutableOrPrecompile::Executable(executable)
					}
				} else {
					if let Some(precompile) = precompile {
						ExecutableOrPrecompile::Precompile {
							instance: precompile,
							_phantom: Default::default(),
						}
					} else {
						let executable = E::from_storage(
							contract
								.as_contract()
								.expect("When not a precompile the contract was loaded above; qed")
								.code_hash,
							gas_meter,
						)?;
						ExecutableOrPrecompile::Executable(executable)
					}
				};

				(dest, contract, executable, delegated_call, ExportedFunction::Call)
			},
			FrameArgs::Instantiate {
				sender,
				executable,
				salt,
				input_data,
				nonce_already_incremented,
			} => {
				let deployer = T::AddressMapper::to_address(&sender);
				let account_nonce = <System<T>>::account_nonce(&sender);
				let address = if let Some(salt) = salt {
					address::create2(&deployer, executable.code(), input_data, salt)
				} else {
					use sp_runtime::Saturating;
					address::create1(
						&deployer,
						// the Nonce from the origin has been incremented pre-dispatch, so we
						// need to subtract 1 to get the nonce at the time of the call.
						if matches!(nonce_already_incremented, NonceAlreadyIncremented::Yes) {
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
					CachedContract::Cached(contract),
					ExecutableOrPrecompile::Executable(executable),
					None,
					ExportedFunction::Constructor,
				)
			},
		};

		let frame = Frame {
			delegate,
			value_transferred,
			contract_info,
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
		value_transferred: U256,
		gas_limit: Weight,
		deposit_limit: BalanceOf<T>,
		read_only: bool,
	) -> Result<Option<ExecutableOrPrecompile<T, E, Self>>, ExecError> {
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
	fn run(
		&mut self,
		executable: ExecutableOrPrecompile<T, E, Self>,
		input_data: Vec<u8>,
	) -> Result<(), ExecError> {
		let frame = self.top_frame();
		let entry_point = frame.entry_point;

		if_tracing(|tracer| {
			tracer.enter_child_span(
				self.caller().account_id().map(T::AddressMapper::to_address).unwrap_or_default(),
				T::AddressMapper::to_address(&frame.account_id),
				frame.delegate.is_some(),
				frame.read_only,
				frame.value_transferred,
				&input_data,
				frame.nested_gas.gas_left(),
			);
		});

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

		let do_transaction = || -> ExecResult {
			let caller = self.caller();
			let frame = top_frame_mut!(self);
			let account_id = &frame.account_id.clone();

			// We need to make sure that the contract's account exists before calling its
			// constructor.
			if entry_point == ExportedFunction::Constructor {
				// Root origin can't be used to instantiate a contract, so it is safe to assume that
				// if we reached this point the origin has an associated account.
				let origin = &self.origin.account_id()?;

				let ed = <Contracts<T>>::min_balance();
				frame.nested_storage.record_charge(&StorageDeposit::Charge(ed));
				if self.skip_transfer {
					T::Currency::set_balance(account_id, ed);
				} else {
					T::Currency::transfer(origin, account_id, ed, Preservation::Preserve)
						.map_err(|_| <Error<T>>::StorageDepositNotEnoughFunds)?;
				}

				// A consumer is added at account creation and removed it on termination, otherwise
				// the runtime could remove the account. As long as a contract exists its
				// account must exist. With the consumer, a correct runtime cannot remove the
				// account.
				<System<T>>::inc_consumers(account_id)?;

				// Needs to be incremented before calling into the code so that it is visible
				// in case of recursion.
				<System<T>>::inc_account_nonce(caller.account_id()?);

				// The incremented refcount should be visible to the constructor.
				<CodeInfo<T>>::increment_refcount(
					*executable
						.as_executable()
						.expect("Precompiles cannot be instantiated; qed")
						.code_hash(),
				)?;
			}

			// Every non delegate call or instantiate also optionally transfers the balance.
			// If it is a delegate call, then we've already transferred tokens in the
			// last non-delegate frame.
			if frame.delegate.is_none() {
				Self::transfer_from_origin(
					&self.origin,
					&caller,
					account_id,
					frame.value_transferred,
				)?;
			}

			// We need to make sure that the pre-compiles contract exist before executing it.
			// A few more conditionals:
			// 	- Only contracts with extended API (has_contract_info) are guaranteed to have an
			//    account.
			//  - Only when not delegate calling we are executing in the context of the pre-compile.
			//    Pre-compiles itself cannot delegate call.
			if let Some(precompile) = executable.as_precompile() {
				if precompile.has_contract_info() &&
					frame.delegate.is_none() &&
					!<System<T>>::account_exists(account_id)
				{
					// prefix matching pre-compiles cannot have a contract info
					// hence we only mint once per pre-compile
					T::Currency::mint_into(account_id, T::Currency::minimum_balance())?;
					// make sure the pre-compile does not destroy its account by accident
					<System<T>>::inc_consumers(account_id)?;
				}
			}

			let code_deposit = executable
				.as_executable()
				.map(|exec| exec.code_info().deposit())
				.unwrap_or_default();

			let output = match executable {
				ExecutableOrPrecompile::Executable(executable) =>
					executable.execute(self, entry_point, input_data),
				ExecutableOrPrecompile::Precompile { instance, .. } =>
					instance.call(input_data, self),
			}
			.map_err(|e| ExecError { error: e.error, origin: ErrorOrigin::Callee })?;

			// Avoid useless work that would be reverted anyways.
			if output.did_revert() {
				return Ok(output);
			}

			let frame = self.top_frame_mut();

			// The deposit we charge for a contract depends on the size of the immutable data.
			// Hence we need to delay charging the base deposit after execution.
			if entry_point == ExportedFunction::Constructor {
				let deposit = frame.contract_info().update_base_deposit(code_deposit);
				frame
					.nested_storage
					.charge_deposit(frame.account_id.clone(), StorageDeposit::Charge(deposit));
			}

			// The storage deposit is only charged at the end of every call stack.
			// To make sure that no sub call uses more than it is allowed to,
			// the limit is manually enforced here.
			let contract = frame.contract_info.as_contract();
			frame
				.nested_storage
				.enforce_limit(contract)
				.map_err(|e| ExecError { error: e, origin: ErrorOrigin::Callee })?;

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
			Ok((success, output)) => {
				if_tracing(|tracer| {
					let gas_consumed = top_frame!(self).nested_gas.gas_consumed();
					match &output {
						Ok(output) => tracer.exit_child_span(&output, gas_consumed),
						Err(e) => tracer.exit_child_span_with_error(e.error.into(), gas_consumed),
					}
				});

				(success, output)
			},
			// `with_transactional` returned an error, and we propagate that error and note no state
			// has changed.
			Err(error) => {
				if_tracing(|tracer| {
					let gas_consumed = top_frame!(self).nested_gas.gas_consumed();
					tracer.exit_child_span_with_error(error.into(), gas_consumed);
				});

				(false, Err(error.into()))
			},
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
			// If the dropped frame's contract has a contract info we update the deposit
			// counter in its contract info. The load is necessary to pull it from storage in case
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
				if let Some(f) = self.frames_mut().skip(1).find(|f| f.account_id == *account_id) {
					f.contract_info.invalidate();
				}
			}
		} else {
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
	///
	/// This is a no-op for zero `value`, avoiding events to be emitted for zero balance transfers.
	///
	/// If the destination account does not exist, it is pulled into existence by transferring the
	/// ED from `origin` to the new account. The total amount transferred to `to` will be ED +
	/// `value`. This makes the ED fully transparent for contracts.
	/// The ED transfer is executed atomically with the actual transfer, avoiding the possibility of
	/// the ED transfer succeeding but the actual transfer failing. In other words, if the `to` does
	/// not exist, the transfer does fail and nothing will be sent to `to` if either `origin` can
	/// not provide the ED or transferring `value` from `from` to `to` fails.
	/// Note: This will also fail if `origin` is root.
	fn transfer(
		origin: &Origin<T>,
		from: &T::AccountId,
		to: &T::AccountId,
		value: U256,
	) -> ExecResult {
		let value = crate::Pallet::<T>::convert_evm_to_native(value, ConversionPrecision::Exact)?;
		if value.is_zero() {
			return Ok(Default::default());
		}

		if <System<T>>::account_exists(to) {
			return T::Currency::transfer(from, to, value, Preservation::Preserve)
				.map(|_| Default::default())
				.map_err(|_| Error::<T>::TransferFailed.into());
		}

		let origin = origin.account_id()?;
		let ed = <T as Config>::Currency::minimum_balance();
		with_transaction(|| -> TransactionOutcome<ExecResult> {
			match T::Currency::transfer(origin, to, ed, Preservation::Preserve)
				.map_err(|_| Error::<T>::StorageDepositNotEnoughFunds.into())
				.and_then(|_| {
					T::Currency::transfer(from, to, value, Preservation::Preserve)
						.map_err(|_| Error::<T>::TransferFailed.into())
				}) {
				Ok(_) => TransactionOutcome::Commit(Ok(Default::default())),
				Err(err) => TransactionOutcome::Rollback(Err(err)),
			}
		})
	}

	/// Same as `transfer` but `from` is an `Origin`.
	fn transfer_from_origin(
		origin: &Origin<T>,
		from: &Origin<T>,
		to: &T::AccountId,
		value: U256,
	) -> ExecResult {
		// If the from address is root there is no account to transfer from, and therefore we can't
		// take any `value` other than 0.
		let from = match from {
			Origin::Signed(caller) => caller,
			Origin::Root if value.is_zero() => return Ok(Default::default()),
			Origin::Root => return Err(DispatchError::RootNotAllowed.into()),
		};
		Self::transfer(origin, from, to, value)
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
		crate::Pallet::<T>::convert_native_to_evm(T::Currency::reducible_balance(
			who,
			Preservation::Preserve,
			Fortitude::Polite,
		))
	}

	/// Certain APIs, e.g. `{set,get}_immutable_data` behave differently depending
	/// on the configured entry point. Thus, we allow setting the export manually.
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn override_export(&mut self, export: ExportedFunction) {
		self.top_frame_mut().entry_point = export;
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn set_block_number(&mut self, block_number: BlockNumberFor<T>) {
		self.block_number = block_number;
	}

	fn block_hash(&self, block_number: U256) -> Option<H256> {
		let Ok(block_number) = BlockNumberFor::<T>::try_from(block_number) else {
			return None;
		};
		if block_number >= self.block_number {
			return None;
		}
		if block_number < self.block_number.saturating_sub(256u32.into()) {
			return None;
		}
		Some(System::<T>::block_hash(&block_number).into())
	}
}

impl<'a, T, E> Ext for Stack<'a, T, E>
where
	T: Config,
	E: Executable<T>,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	fn delegate_call(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		address: H160,
		input_data: Vec<u8>,
	) -> Result<(), ExecError> {
		// We reset the return data now, so it is cleared out even if no new frame was executed.
		// This is for example the case for unknown code hashes or creating the frame fails.
		*self.last_frame_output_mut() = Default::default();

		let top_frame = self.top_frame_mut();
		let contract_info = top_frame.contract_info().clone();
		let account_id = top_frame.account_id.clone();
		let value = top_frame.value_transferred;
		if let Some(executable) = self.push_frame(
			FrameArgs::Call {
				dest: account_id,
				cached_info: Some(contract_info),
				delegated_call: Some(DelegateInfo {
					caller: self.caller().clone(),
					callee: address,
				}),
			},
			value,
			gas_limit,
			deposit_limit.saturated_into::<BalanceOf<T>>(),
			self.is_read_only(),
		)? {
			self.run(executable, input_data)
		} else {
			// Delegate-calls to non-contract accounts are considered success.
			Ok(())
		}
	}

	fn terminate(&mut self, beneficiary: &H160) -> DispatchResult {
		if self.is_recursive() {
			return Err(Error::<T>::TerminatedWhileReentrant.into());
		}
		let frame = self.top_frame_mut();
		if frame.entry_point == ExportedFunction::Constructor {
			return Err(Error::<T>::TerminatedInConstructor.into());
		}
		let info = frame.terminate();
		let beneficiary_account = T::AddressMapper::to_account_id(beneficiary);
		frame.nested_storage.terminate(&info, beneficiary_account);

		info.queue_trie_for_deletion();
		let account_address = T::AddressMapper::to_address(&frame.account_id);
		ContractInfoOf::<T>::remove(&account_address);
		ImmutableDataOf::<T>::remove(&account_address);
		<CodeInfo<T>>::decrement_refcount(info.code_hash)?;

		Ok(())
	}

	fn own_code_hash(&mut self) -> &H256 {
		&self.top_frame_mut().contract_info().code_hash
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
		let new_base_deposit = info.update_base_deposit(code_info.deposit());
		let deposit = StorageDeposit::Charge(new_base_deposit)
			.saturating_sub(&StorageDeposit::Charge(old_base_deposit));

		frame.nested_storage.charge_deposit(frame.account_id.clone(), deposit);

		<CodeInfo<T>>::increment_refcount(hash)?;
		<CodeInfo<T>>::decrement_refcount(prev_hash)?;
		Ok(())
	}

	fn call_runtime(&self, call: <Self::T as Config>::RuntimeCall) -> DispatchResultWithPostInfo {
		let mut origin: T::RuntimeOrigin = RawOrigin::Signed(self.account_id().clone()).into();
		origin.add_filter(T::CallFilter::contains);
		call.dispatch(origin)
	}

	fn immutable_data_len(&mut self) -> u32 {
		self.top_frame_mut().contract_info().immutable_data_len()
	}

	fn get_immutable_data(&mut self) -> Result<ImmutableData, DispatchError> {
		if self.top_frame().entry_point == ExportedFunction::Constructor {
			return Err(Error::<T>::InvalidImmutableAccess.into());
		}

		// Immutable is read from contract code being executed
		let address = self
			.top_frame()
			.delegate
			.as_ref()
			.map(|d| d.callee)
			.unwrap_or(T::AddressMapper::to_address(self.account_id()));
		Ok(<ImmutableDataOf<T>>::get(address).ok_or_else(|| Error::<T>::InvalidImmutableAccess)?)
	}

	fn set_immutable_data(&mut self, data: ImmutableData) -> Result<(), DispatchError> {
		let frame = self.top_frame_mut();
		if frame.entry_point == ExportedFunction::Call || data.is_empty() {
			return Err(Error::<T>::InvalidImmutableAccess.into());
		}
		frame.contract_info().set_immutable_data_len(data.len() as u32);
		<ImmutableDataOf<T>>::insert(T::AddressMapper::to_address(&frame.account_id), &data);
		Ok(())
	}
}

impl<'a, T, E> PrecompileWithInfoExt for Stack<'a, T, E>
where
	T: Config,
	E: Executable<T>,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
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

	fn charge_storage(&mut self, diff: &Diff) {
		self.top_frame_mut().nested_storage.charge(diff)
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
				nonce_already_incremented: NonceAlreadyIncremented::No,
			},
			value.try_into().map_err(|_| Error::<T>::BalanceConversionFailed)?,
			gas_limit,
			deposit_limit.saturated_into::<BalanceOf<T>>(),
			self.is_read_only(),
		)?;
		let address = T::AddressMapper::to_address(&self.top_frame().account_id);
		self.run(executable.expect(FRAME_ALWAYS_EXISTS_ON_INSTANTIATE), input_data)
			.map(|_| address)
	}
}

impl<'a, T, E> PrecompileExt for Stack<'a, T, E>
where
	T: Config,
	E: Executable<T>,
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
{
	type T = T;

	fn call(
		&mut self,
		gas_limit: Weight,
		deposit_limit: U256,
		dest_addr: &H160,
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
			// Enable read-only access if requested; cannot disable it if already set.
			let is_read_only = read_only || self.is_read_only();

			// We can skip the stateful lookup for pre-compiles.
			let dest = if <AllPrecompiles<T>>::get::<Self>(dest_addr.as_fixed_bytes()).is_some() {
				T::AddressMapper::to_fallback_account_id(dest_addr)
			} else {
				T::AddressMapper::to_account_id(dest_addr)
			};

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
				deposit_limit.saturated_into::<BalanceOf<T>>(),
				is_read_only,
			)? {
				self.run(executable, input_data)
			} else {
				let result = if is_read_only && value.is_zero() {
					Ok(Default::default())
				} else if is_read_only {
					Err(Error::<T>::StateChangeDenied.into())
				} else {
					Self::transfer_from_origin(
						&self.origin,
						&Origin::from_account_id(self.account_id().clone()),
						&dest,
						value,
					)
				};

				if_tracing(|t| {
					t.enter_child_span(
						T::AddressMapper::to_address(self.account_id()),
						T::AddressMapper::to_address(&dest),
						false,
						is_read_only,
						value,
						&input_data,
						Weight::zero(),
					);
					match result {
						Ok(ref output) => t.exit_child_span(&output, Weight::zero()),
						Err(e) => t.exit_child_span_with_error(e.error.into(), Weight::zero()),
					}
				});
				result.map(|_| ())
			}
		};

		// We need to make sure to reset `allows_reentry` even on failure.
		let result = try_call();

		// Protection is on a per call basis.
		self.top_frame_mut().allows_reentry = true;

		result
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
		if let Some(DelegateInfo { caller, .. }) = &self.top_frame().delegate {
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

	fn to_account_id(&self, address: &H160) -> T::AccountId {
		T::AddressMapper::to_account_id(address)
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

	fn code_size(&self, address: &H160) -> u64 {
		<ContractInfoOf<T>>::get(&address)
			.and_then(|contract| CodeInfoOf::<T>::get(contract.code_hash))
			.map(|info| info.code_len())
			.unwrap_or_default()
	}

	fn caller_is_origin(&self) -> bool {
		self.origin == self.caller()
	}

	fn caller_is_root(&self) -> bool {
		// if the caller isn't origin, then it can't be root.
		self.caller_is_origin() && self.origin == Origin::Root
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
		(self.timestamp / 1000u32.into()).into()
	}

	fn minimum_balance(&self) -> U256 {
		T::Currency::minimum_balance().into()
	}

	fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>) {
		let contract = T::AddressMapper::to_address(self.account_id());
		if_tracing(|tracer| {
			tracer.log_event(contract, &topics, &data);
		});
		Contracts::<Self::T>::deposit_event(Event::ContractEmitted { contract, data, topics });
	}

	fn block_number(&self) -> U256 {
		self.block_number.into()
	}

	fn block_hash(&self, block_number: U256) -> Option<H256> {
		self.block_hash(block_number)
	}

	fn block_author(&self) -> Option<AccountIdOf<Self::T>> {
		let digest = <frame_system::Pallet<T>>::digest();
		let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());

		T::FindAuthor::find_author(pre_runtime_digests)
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

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn transient_storage(&mut self) -> &mut TransientStorage<Self::T> {
		&mut self.transient_storage
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
