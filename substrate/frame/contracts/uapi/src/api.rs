// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// TODO:
// - Add missing unstable methods

use crate::{CallFlags, Result, ReturnFlags};
use paste::paste;

macro_rules! hash_fn {
	( $name:ident, $bytes:literal ) => {
		paste! {
			#[doc = "Computes the " $name " " $bytes "-bit hash on the given input buffer."]
			#[doc = "\n# Notes\n"]
			#[doc = "- The `input` and `output` buffer may overlap."]
			#[doc = "- The output buffer is expected to hold at least " $bytes " bits."]
			#[doc = "- It is the callers responsibility to provide an output buffer that is large enough to hold the expected amount of bytes returned by the hash function."]
			#[doc = "\n# Parameters\n"]
			#[doc = "- `input`: The input data buffer."]
			#[doc = "- `output`: The output buffer to write the hash result to."]
			fn [<hash_ $name>](input: &[u8], output: &mut [u8; $bytes]);
		}
	};
}

pub trait Api {
	/// Instantiate a contract with the specified code hash.
	///
	/// This function creates an account and executes the constructor defined in the code specified
	/// by the code hash.
	///
	/// # Parameters
	///
	/// - `code_hash`: The hash of the code to be instantiated.
	/// - `gas_limit`: How much gas to devote for the execution.
	/// - `endowment`: The value to transfer into the contract.
	/// - `input`: The input data buffer.
	/// - `out_address`: A reference to the address buffer to write the address of the contract. If
	///   `None` is provided then the output buffer is not copied.
	/// - `out_return_value`: A reference to the return value buffer to write the constructor output
	///   buffer. If `None` is provided then the output buffer is not copied.
	/// - `salt`: The salt bytes to use for this instantiation.
	///
	/// # Errors
	///
	/// Please consult the [`ReturnErrorCode`] enum declaration for more information on those
	/// errors. Here we only note things specific to this function.
	///
	/// An error means that the account wasn't created and no address or output buffer
	/// is returned unless stated otherwise.
	///
	/// - [`ReturnErrorCode::CalleeReverted`]: Output buffer is returned.
	/// - [`ReturnErrorCode::CalleeTrapped`]
	/// - [`ReturnErrorCode::TransferFailed`]
	/// - [`ReturnErrorCode::CodeNotFound`]
	fn instantiate(
		code_hash: &[u8],
		gas_limit: u64,
		endowment: &[u8],
		input: &[u8],
		out_address: Option<&mut [u8]>,
		out_return_value: Option<&mut [u8]>,
		salt: &[u8],
	) -> Result;

	/// Call (possibly transferring some amount of funds) into the specified account.
	///
	/// # Parameters
	///
	/// - `flags`: See [`CallFlags`] for a documentation of the supported flags.
	/// - `callee`: The address of the callee. Should be decodable as an `T::AccountId`. Traps
	///   otherwise.
	/// - `gas_limit`: How much gas to devote for the execution.
	/// - `value`: The value to transfer into the contract. Should be decodable as a `T::Balance`.
	///   Traps otherwise.
	/// - `input`: The input data buffer used to call the contract.
	/// - `output`: A reference to the output data buffer to write the call output buffer. If `None`
	///   is provided then the output buffer is not copied.
	///
	/// # Errors
	///
	/// An error means that the call wasn't successful output buffer is returned unless
	/// stated otherwise.
	///
	/// - [`ReturnErrorCode::CalleeReverted`]: Output buffer is returned.
	/// - [`ReturnErrorCode::CalleeTrapped`]
	/// - [`ReturnErrorCode::TransferFailed`]
	/// - [`ReturnErrorCode::NotCallable`]
	fn call(
		flags: CallFlags,
		callee: &[u8],
		gas_limit: u64,
		value: &[u8],
		input: &[u8],
		output: Option<&mut [u8]>,
	) -> Result;

	/// Execute code in the context (storage, caller, value) of the current contract.
	///
	/// Reentrancy protection is always disabled since the callee is allowed
	/// to modify the callers storage. This makes going through a reentrancy attack
	/// unnecessary for the callee when it wants to exploit the caller.
	///
	/// # Parameters
	///
	/// - `flags`: See [`CallFlags`] for a documentation of the supported flags.
	/// - `code_hash`: The hash of the code to be executed.
	/// - `input`: The input data buffer used to call the contract.
	/// - `output`: A reference to the output data buffer to write the call output buffer. If `None`
	///   is provided then the output buffer is not copied.
	///
	/// # Errors
	///
	/// An error means that the call wasn't successful and no output buffer is returned unless
	/// stated otherwise.
	///
	/// - [`ReturnErrorCode::CalleeReverted`]: Output buffer is returned.
	/// - [`ReturnErrorCode::CalleeTrapped`]
	/// - [`ReturnErrorCode::CodeNotFound`]
	fn delegate_call(
		flags: CallFlags,
		code_hash: &[u8],
		input: &[u8],
		output: Option<&mut [u8]>,
	) -> Result;

	/// Transfer some amount of funds into the specified account.
	///
	/// # Parameters
	///
	/// - `account_id`: The address of the account to transfer funds to. Should be decodable as an
	///   `T::AccountId`. Traps otherwise.
	/// - `value`: The value to transfer. Should be decodable as a `T::Balance`. Traps otherwise.
	///
	/// # Errors
	///
	/// - [`ReturnErrorCode::TransferFailed`]
	fn transfer(account_id: &[u8], value: &[u8]) -> Result;

	/// Deposit a contract event with the data buffer and optional list of topics. There is a limit
	/// on the maximum number of topics specified by `event_topics`.
	///
	/// There should not be any duplicates in `topics`.
	///
	/// # Parameters
	///
	/// - `topics`: The topics list encoded as `Vec<T::Hash>`. It can't contain duplicates.
	fn deposit_event(topics: &[u8], data: &[u8]);

	/// Set the value at the given key in the contract storage.
	///
	/// The key and value lengths must not exceed the maximums defined by the contracts module
	/// parameters.
	///
	/// # Parameters
	///
	/// - `key`: The storage key.
	/// - `encoded_value`: The storage value.
	///
	/// # Return
	///
	/// Returns the size of the pre-existing value at the specified key if any.
	fn set_storage(key: &[u8], value: &[u8]) -> Option<u32>;

	/// Clear the value at the given key in the contract storage.
	///
	/// # Parameters
	///
	/// - `key`: The storage key.
	///
	/// # Return
	///
	/// Returns the size of the pre-existing value at the specified key if any.
	fn clear_storage(key: &[u8]) -> Option<u32>;

	/// Retrieve the value under the given key from storage.
	///
	/// The key length must not exceed the maximum defined by the contracts module parameter.
	///
	/// # Parameters
	/// - `key`: The storage key.
	/// - `output`: A reference to the output data buffer to write the storage entry.
	///
	/// # Errors
	///
	/// [`ReturnErrorCode::KeyNotFound`]
	fn get_storage(key: &[u8], output: &mut &mut [u8]) -> Result;

	/// Retrieve and remove the value under the given key from storage.
	///
	/// # Parameters
	/// - `key`: The storage key.
	/// - `output`: A reference to the output data buffer to write the storage entry.
	///
	/// # Errors
	///
	/// [`ReturnErrorCode::KeyNotFound`]
	fn take_storage(key: &[u8], output: &mut &mut [u8]) -> Result;

	/// Checks whether there is a value stored under the given key.
	///
	/// The key length must not exceed the maximum defined by the contracts module parameter.
	///
	/// # Parameters
	/// - `key`: The storage key.
	///
	/// # Return
	///
	/// Returns the size of the pre-existing value at the specified key if any.
	fn storage_contains(key: &[u8]) -> Option<u32>;

	/// Remove the calling account and transfer remaining **free** balance.
	///
	/// This function never returns. Either the termination was successful and the
	/// execution of the destroyed contract is halted. Or it failed during the termination
	/// which is considered fatal and results in a trap + rollback.
	///
	/// # Parameters
	///
	/// - `beneficiary`: The address of the beneficiary account, Should be decodable as an
	/// `T::AccountId`.
	///
	/// # Traps
	///
	/// - The contract is live i.e is already on the call stack.
	/// - Failed to send the balance to the beneficiary.
	/// - The deletion queue is full.
	fn terminate(beneficiary: &[u8]) -> !;

	/// Call into the chain extension provided by the chain if any.
	///
	/// Handling of the input values is up to the specific chain extension and so is the
	/// return value. The extension can decide to use the inputs as primitive inputs or as
	/// in/out arguments by interpreting them as pointers. Any caller of this function
	/// must therefore coordinate with the chain that it targets.
	///
	/// # Note
	///
	/// If no chain extension exists the contract will trap with the `NoChainExtension`
	/// module error.
	///
	/// # Parameters
	///
	/// - `func_id`: The function id of the chain extension.
	/// - `input`: The input data buffer.
	/// - `output`: A reference to the output data buffer to write the output data.
	///
	/// # Return
	///
	/// The chain extension returned value, if executed successfully.
	fn call_chain_extension(func_id: u32, input: &[u8], output: &mut &mut [u8]) -> u32;

	/// Stores the input passed by the caller into the supplied buffer.
	///
	/// # Note
	///
	/// This function traps if:
	/// - the input is larger than the available space.
	/// - the input was previously forwarded by a [`call()`][`Self::call()`].
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the input data.
	fn input(output: &mut &mut [u8]);

	/// Cease contract execution and save a data buffer as a result of the execution.
	///
	/// This function never returns as it stops execution of the caller.
	/// This is the only way to return a data buffer to the caller. Returning from
	/// execution without calling this function is equivalent to calling:
	/// ```nocompile
	/// return_value(ReturnFlags::empty(), &[])
	/// ```
	///
	/// Using an unnamed non empty `ReturnFlags` triggers a trap.
	///
	/// # Parameters
	///
	/// - `flags`: Flag used to signal special return conditions to the supervisor. See
	///   [`ReturnFlags`] for a documentation of the supported flags.
	/// - `return_value`: The return value buffer.
	fn return_value(flags: ReturnFlags, return_value: &[u8]) -> !;

	/// Call some dispatchable of the runtime.
	///
	/// # Parameters
	///
	/// - `call`: The call data.
	///
	/// # Return
	///
	/// Returns `Error::Success` when the dispatchable was successfully executed and
	/// returned `Ok`. When the dispatchable was executed but returned an error
	/// `Error::CallRuntimeFailed` is returned. The full error is not
	/// provided because it is not guaranteed to be stable.
	///
	/// # Comparison with `ChainExtension`
	///
	/// Just as a chain extension this API allows the runtime to extend the functionality
	/// of contracts. While making use of this function is generally easier it cannot be
	/// used in all cases. Consider writing a chain extension if you need to do perform
	/// one of the following tasks:
	///
	/// - Return data.
	/// - Provide functionality **exclusively** to contracts.
	/// - Provide custom weights.
	/// - Avoid the need to keep the `Call` data structure stable.
	fn call_runtime(call: &[u8]) -> Result;

	/// Stores the address of the caller into the supplied buffer.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// If this is a top-level call (i.e. initiated by an extrinsic) the origin address of the
	/// extrinsic will be returned. Otherwise, if this call is initiated by another contract then
	/// the address of the contract will be returned.
	///
	/// If there is no address associated with the caller (e.g. because the caller is root) then
	/// it traps with `BadOrigin`.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the caller address.
	fn caller(output: &mut &mut [u8]);

	/// Stores the current block number of the current contract into the supplied buffer.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the block number.
	fn block_number(output: &mut &mut [u8]);

	/// Stores the address of the current contract into the supplied buffer.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the address.
	fn address(output: &mut &mut [u8]);

	/// Stores the *free* balance of the current account into the supplied buffer.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the balance.
	fn balance(output: &mut &mut [u8]);

	/// Stores the amount of weight left into the supplied buffer.
	/// The data is encoded as Weight.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the weight left.
	fn gas_left(output: &mut &mut [u8]);

	/// Stores the value transferred along with this call/instantiate into the supplied buffer.
	/// The data is encoded as `T::Balance`.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the transferred value.
	fn value_transferred(output: &mut &mut [u8]);

	/// Load the latest block timestamp into the supplied buffer
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the timestamp.
	fn now(output: &mut &mut [u8]);

	/// Stores the minimum balance (a.k.a. existential deposit) into the supplied buffer.
	/// The data is encoded as `T::Balance`.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the minimum balance.
	fn minimum_balance(output: &mut &mut [u8]);

	/// Stores the price for the specified amount of gas into the supplied buffer.
	/// The data is encoded as `T::Balance`.
	///
	/// If the available space in `output` is less than the size of the value a trap is triggered.
	///
	/// # Parameters
	///
	/// - `gas`: The amount of gas to query the price for.
	/// - `output`: A reference to the output data buffer to write the price.
	fn weight_to_fee(gas: u64, output: &mut &mut [u8]);

	hash_fn!(sha2_256, 32);
	hash_fn!(keccak_256, 32);
	hash_fn!(blake2_256, 32);
	hash_fn!(blake2_128, 16);

	/// Recovers the ECDSA public key from the given message hash and signature.
	///
	/// Writes the public key into the given output buffer.
	/// Assumes the secp256k1 curve.
	///
	/// # Parameters
	///
	/// - `signature`: The signature bytes.
	/// - `message_hash`: The message hash bytes.
	/// - `output`: A reference to the output data buffer to write the public key.
	///
	/// # Errors
	///
	/// - [`ReturnErrorCode::EcdsaRecoveryFailed`]
	fn ecdsa_recover(
		signature: &[u8; 65],
		message_hash: &[u8; 32],
		output: &mut [u8; 33],
	) -> Result;

	/// Calculates Ethereum address from the ECDSA compressed public key and stores
	/// it into the supplied buffer.
	///
	/// # Parameters
	///
	/// - `pubkey`: The public key bytes.
	/// - `output`: A reference to the output data buffer to write the address.
	///
	/// # Errors
	///
	/// - [`ReturnErrorCode::EcdsaRecoveryFailed`]
	fn ecdsa_to_eth_address(pubkey: &[u8; 33], output: &mut [u8; 20]) -> Result;

	/// Verify a sr25519 signature
	///
	/// # Parameters
	///
	/// - `signature`: The signature bytes.
	/// - `message`: The message bytes.
	///
	/// # Errors
	///
	/// - [`ReturnErrorCode::Sr25519VerifyFailed`]
	fn sr25519_verify(signature: &[u8; 64], message: &[u8], pub_key: &[u8; 32]) -> Result;

	/// Checks whether a specified address belongs to a contract.
	///
	/// # Parameters
	///
	/// - `account_id`: The address to check. Should be decodable as an `T::AccountId`. Traps
	///   otherwise.
	///
	/// # Return
	///
	/// Returns `true` if the address belongs to a contract.
	fn is_contract(account_id: &[u8]) -> bool;

	/// Checks whether the caller of the current contract is the origin of the whole call stack.
	///
	/// Prefer this over [`is_contract()`][`Self::is_contract`] when checking whether your contract
	/// is being called by a contract or a plain account. The reason is that it performs better
	/// since it does not need to do any storage lookups.
	///
	/// # Return
	///
	/// A return value of `true` indicates that this contract is being called by a plain account
	/// and `false` indicates that the caller is another contract.
	fn caller_is_origin() -> bool;

	/// Replace the contract code at the specified address with new code.
	///
	/// # Note
	///
	/// There are a couple of important considerations which must be taken into account when
	/// using this API:
	///
	/// 1. The storage at the code address will remain untouched. This means that contract
	/// developers must ensure that the storage layout of the new code is compatible with that of
	/// the old code.
	///
	/// 2. Contracts using this API can't be assumed as having deterministic addresses. Said another
	/// way, when using this API you lose the guarantee that an address always identifies a specific
	/// code hash.
	///
	/// 3. If a contract calls into itself after changing its code the new call would use
	/// the new code. However, if the original caller panics after returning from the sub call it
	/// would revert the changes made by [`set_code_hash()`][`Self::set_code_hash`] and the next
	/// caller would use the old code.
	///
	/// # Parameters
	///
	/// - `code_hash`: The hash of the new code.
	///
	/// # Errors
	///
	/// - [`ReturnErrorCode::CodeNotFound`]
	fn set_code_hash(code_hash: &[u8]) -> Result;

	/// Retrieve the code hash for a specified contract address.
	///
	/// # Parameters
	///
	/// - `account_id`: The address of the contract.Should be decodable as an `T::AccountId`. Traps
	///   otherwise.
	/// - `output`: A reference to the output data buffer to write the code hash.
	///
	///
	/// # Errors
	///
	/// - [`ReturnErrorCode::CodeNotFound`]
	fn code_hash(account_id: &[u8], output: &mut [u8]) -> Result;

	/// Retrieve the code hash of the currently executing contract.
	///
	/// # Parameters
	///
	/// - `output`: A reference to the output data buffer to write the code hash.
	fn own_code_hash(output: &mut [u8]);
}
