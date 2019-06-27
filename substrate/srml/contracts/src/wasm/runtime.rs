// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <http://www.gnu.org/licenses/>.

//! Environment definition of the wasm smart-contract runtime.

use crate::{Schedule, Trait, CodeHash, ComputeDispatchFee, BalanceOf};
use crate::exec::{
	Ext, VmExecResult, OutputBuf, EmptyOutputBuf, CallReceipt, InstantiateReceipt, StorageKey,
	TopicOf,
};
use crate::gas::{Gas, GasMeter, Token, GasMeterResult, approx_gas_for_balance};
use sandbox;
use system;
use rstd::prelude::*;
use rstd::mem;
use parity_codec::{Decode, Encode};
use runtime_primitives::traits::{Bounded, SaturatedConversion};

/// Enumerates all possible *special* trap conditions.
///
/// In this runtime traps used not only for signaling about errors but also
/// to just terminate quickly in some cases.
enum SpecialTrap {
	/// Signals that trap was generated in response to call `ext_return` host function.
	Return(OutputBuf),
}

/// Can only be used for one call.
pub(crate) struct Runtime<'a, E: Ext + 'a> {
	ext: &'a mut E,
	// A VM can return a result only once and only by value. So
	// we wrap output buffer to make it possible to take the buffer out.
	empty_output_buf: Option<EmptyOutputBuf>,
	scratch_buf: Vec<u8>,
	schedule: &'a Schedule,
	memory: sandbox::Memory,
	gas_meter: &'a mut GasMeter<E::T>,
	special_trap: Option<SpecialTrap>,
}
impl<'a, E: Ext + 'a> Runtime<'a, E> {
	pub(crate) fn new(
		ext: &'a mut E,
		input_data: Vec<u8>,
		empty_output_buf: EmptyOutputBuf,
		schedule: &'a Schedule,
		memory: sandbox::Memory,
		gas_meter: &'a mut GasMeter<E::T>,
	) -> Self {
		Runtime {
			ext,
			empty_output_buf: Some(empty_output_buf),
			// Put the input data into the scratch buffer immediately.
			scratch_buf: input_data,
			schedule,
			memory,
			gas_meter,
			special_trap: None,
		}
	}

	fn memory(&self) -> &sandbox::Memory {
		&self.memory
	}
}

pub(crate) fn to_execution_result<E: Ext>(
	runtime: Runtime<E>,
	sandbox_err: Option<sandbox::Error>,
) -> VmExecResult {
	// Check the exact type of the error. It could be plain trap or
	// special runtime trap the we must recognize.
	match (sandbox_err, runtime.special_trap) {
		// No traps were generated. Proceed normally.
		(None, None) => VmExecResult::Ok,
		// Special case. The trap was the result of the execution `return` host function.
		(Some(sandbox::Error::Execution), Some(SpecialTrap::Return(buf))) => VmExecResult::Returned(buf),
		// Any other kind of a trap should result in a failure.
		(Some(_), _) => VmExecResult::Trap("during execution"),
		// Any other case (such as special trap flag without actual trap) signifies
		// a logic error.
		_ => unreachable!(),
	}
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Copy, Clone)]
pub enum RuntimeToken {
	/// Explicit call to the `gas` function. Charge the gas meter
	/// with the value provided.
	Explicit(u32),
	/// The given number of bytes is read from the sandbox memory.
	ReadMemory(u32),
	/// The given number of bytes is written to the sandbox memory.
	WriteMemory(u32),
	/// The given number of bytes is read from the sandbox memory and
	/// is returned as the return data buffer of the call.
	ReturnData(u32),
	/// Dispatch fee calculated by `T::ComputeDispatchFee`.
	ComputedDispatchFee(Gas),
	/// (topic_count, data_bytes): A buffer of the given size is posted as an event indexed with the
	/// given number of topics.
	DepositEvent(u32, u32),
}

impl<T: Trait> Token<T> for RuntimeToken {
	type Metadata = Schedule;

	fn calculate_amount(&self, metadata: &Schedule) -> Gas {
		use self::RuntimeToken::*;
		let value = match *self {
			Explicit(amount) => Some(amount.into()),
			ReadMemory(byte_count) => metadata
				.sandbox_data_read_cost
				.checked_mul(byte_count.into()),
			WriteMemory(byte_count) => metadata
				.sandbox_data_write_cost
				.checked_mul(byte_count.into()),
			ReturnData(byte_count) => metadata
				.return_data_per_byte_cost
				.checked_mul(byte_count.into()),
			DepositEvent(topic_count, data_byte_count) => {
				let data_cost = metadata
					.event_data_per_byte_cost
					.checked_mul(data_byte_count.into());

				let topics_cost = metadata
					.event_per_topic_cost
					.checked_mul(topic_count.into());

				data_cost
					.and_then(|data_cost| {
						topics_cost.and_then(|topics_cost| {
							data_cost.checked_add(topics_cost)
						})
					})
					.and_then(|data_and_topics_cost|
						data_and_topics_cost.checked_add(metadata.event_base_cost)
					)
			},
			ComputedDispatchFee(gas) => Some(gas),
		};

		value.unwrap_or_else(|| Bounded::max_value())
	}
}

/// Charge the gas meter with the specified token.
///
/// Returns `Err(HostError)` if there is not enough gas.
fn charge_gas<T: Trait, Tok: Token<T>>(
	gas_meter: &mut GasMeter<T>,
	metadata: &Tok::Metadata,
	token: Tok,
) -> Result<(), sandbox::HostError> {
	match gas_meter.charge(metadata, token) {
		GasMeterResult::Proceed => Ok(()),
		GasMeterResult::OutOfGas => Err(sandbox::HostError),
	}
}

/// Read designated chunk from the sandbox memory, consuming an appropriate amount of
/// gas.
///
/// Returns `Err` if one of the following conditions occurs:
///
/// - calculating the gas cost resulted in overflow.
/// - out of gas
/// - requested buffer is not within the bounds of the sandbox memory.
fn read_sandbox_memory<E: Ext>(
	ctx: &mut Runtime<E>,
	ptr: u32,
	len: u32,
) -> Result<Vec<u8>, sandbox::HostError> {
	charge_gas(ctx.gas_meter, ctx.schedule, RuntimeToken::ReadMemory(len))?;

	let mut buf = Vec::new();
	buf.resize(len as usize, 0);

	ctx.memory().get(ptr, &mut buf)?;

	Ok(buf)
}

/// Read designated chunk from the sandbox memory into the supplied buffer, consuming
/// an appropriate amount of gas.
///
/// Returns `Err` if one of the following conditions occurs:
///
/// - calculating the gas cost resulted in overflow.
/// - out of gas
/// - requested buffer is not within the bounds of the sandbox memory.
fn read_sandbox_memory_into_buf<E: Ext>(
	ctx: &mut Runtime<E>,
	ptr: u32,
	buf: &mut [u8],
) -> Result<(), sandbox::HostError> {
	charge_gas(ctx.gas_meter, ctx.schedule, RuntimeToken::ReadMemory(buf.len() as u32))?;

	ctx.memory().get(ptr, buf).map_err(Into::into)
}

/// Write the given buffer to the designated location in the sandbox memory, consuming
/// an appropriate amount of gas.
///
/// Returns `Err` if one of the following conditions occurs:
///
/// - calculating the gas cost resulted in overflow.
/// - out of gas
/// - designated area is not within the bounds of the sandbox memory.
fn write_sandbox_memory<T: Trait>(
	schedule: &Schedule,
	gas_meter: &mut GasMeter<T>,
	memory: &sandbox::Memory,
	ptr: u32,
	buf: &[u8],
) -> Result<(), sandbox::HostError> {
	charge_gas(gas_meter, schedule, RuntimeToken::WriteMemory(buf.len() as u32))?;

	memory.set(ptr, buf)?;

	Ok(())
}

// ***********************************************************
// * AFTER MAKING A CHANGE MAKE SURE TO UPDATE COMPLEXITY.MD *
// ***********************************************************

// Define a function `fn init_env<E: Ext>() -> HostFunctionSet<E>` that returns
// a function set which can be imported by an executed contract.
define_env!(Env, <E: Ext>,

	// Account for used gas. Traps if gas used is greater than gas limit.
	//
	// NOTE: This is a implementation defined call and is NOT a part of the public API.
	// This call is supposed to be called only by instrumentation injected code.
	//
	// - amount: How much gas is used.
	gas(ctx, amount: u32) => {
		charge_gas(&mut ctx.gas_meter, ctx.schedule, RuntimeToken::Explicit(amount))?;
		Ok(())
	},

	// Change the value at the given key in the storage or remove the entry.
	//
	// - key_ptr: pointer into the linear
	//   memory where the location of the requested value is placed.
	// - value_non_null: if set to 0, then the entry
	//   at the given location will be removed.
	// - value_ptr: pointer into the linear memory
	//   where the value to set is placed. If `value_non_null` is set to 0, then this parameter is ignored.
	// - value_len: the length of the value. If `value_non_null` is set to 0, then this parameter is ignored.
	ext_set_storage(ctx, key_ptr: u32, value_non_null: u32, value_ptr: u32, value_len: u32) => {
		let mut key: StorageKey = [0; 32];
		read_sandbox_memory_into_buf(ctx, key_ptr, &mut key)?;
		let value =
			if value_non_null != 0 {
				Some(read_sandbox_memory(ctx, value_ptr, value_len)?)
			} else {
				None
			};
		ctx.ext.set_storage(key, value);

		Ok(())
	},

	// Retrieve the value at the given location from the storage and return 0.
	// If there is no entry at the given location then this function will return 1 and
	// clear the scratch buffer.
	//
	// - key_ptr: pointer into the linear memory where the key
	//   of the requested value is placed.
	ext_get_storage(ctx, key_ptr: u32) -> u32 => {
		let mut key: StorageKey = [0; 32];
		read_sandbox_memory_into_buf(ctx, key_ptr, &mut key)?;
		if let Some(value) = ctx.ext.get_storage(&key) {
			ctx.scratch_buf = value;
			Ok(0)
		} else {
			ctx.scratch_buf.clear();
			Ok(1)
		}
	},

	// Make a call to another contract.
	//
	// Returns 0 on the successful execution and puts the result data returned
	// by the callee into the scratch buffer. Otherwise, returns 1 and clears the scratch
	// buffer.
	//
	// - callee_ptr: a pointer to the address of the callee contract.
	//   Should be decodable as an `T::AccountId`. Traps otherwise.
	// - callee_len: length of the address buffer.
	// - gas: how much gas to devote to the execution.
	// - value_ptr: a pointer to the buffer with value, how much value to send.
	//   Should be decodable as a `T::Balance`. Traps otherwise.
	// - value_len: length of the value buffer.
	// - input_data_ptr: a pointer to a buffer to be used as input data to the callee.
	// - input_data_len: length of the input data buffer.
	ext_call(
		ctx,
		callee_ptr: u32,
		callee_len: u32,
		gas: u64,
		value_ptr: u32,
		value_len: u32,
		input_data_ptr: u32,
		input_data_len: u32
	) -> u32 => {
		let callee = {
			let callee_buf = read_sandbox_memory(ctx, callee_ptr, callee_len)?;
			<<E as Ext>::T as system::Trait>::AccountId::decode(&mut &callee_buf[..])
				.ok_or_else(|| sandbox::HostError)?
		};
		let value = {
			let value_buf = read_sandbox_memory(ctx, value_ptr, value_len)?;
			BalanceOf::<<E as Ext>::T>::decode(&mut &value_buf[..])
				.ok_or_else(|| sandbox::HostError)?
		};
		let input_data = read_sandbox_memory(ctx, input_data_ptr, input_data_len)?;

		// Grab the scratch buffer and put in its' place an empty one.
		// We will use it for creating `EmptyOutputBuf` container for the call.
		let scratch_buf = mem::replace(&mut ctx.scratch_buf, Vec::new());
		let empty_output_buf = EmptyOutputBuf::from_spare_vec(scratch_buf);

		let nested_gas_limit = if gas == 0 {
			ctx.gas_meter.gas_left()
		} else {
			gas.saturated_into()
		};
		let ext = &mut ctx.ext;
		let call_outcome = ctx.gas_meter.with_nested(nested_gas_limit, |nested_meter| {
			match nested_meter {
				Some(nested_meter) => {
					ext.call(
						&callee,
						value,
						nested_meter,
						&input_data,
						empty_output_buf
					)
					.map_err(|_| ())
				}
				// there is not enough gas to allocate for the nested call.
				None => Err(()),
			}
		});

		match call_outcome {
			Ok(CallReceipt { output_data }) => {
				ctx.scratch_buf = output_data;
				Ok(0)
			},
			Err(_) => Ok(1),
		}
	},

	// Instantiate a contract with code returned by the specified initializer code.
	//
	// This function creates an account and executes initializer code. After the execution,
	// the returned buffer is saved as the code of the created account.
	//
	// Returns 0 on the successful contract creation and puts the address
	// of the created contract into the scratch buffer.
	// Otherwise, returns 1 and clears the scratch buffer.
	//
	// - init_code_ptr: a pointer to the buffer that contains the initializer code.
	// - init_code_len: length of the initializer code buffer.
	// - gas: how much gas to devote to the execution of the initializer code.
	// - value_ptr: a pointer to the buffer with value, how much value to send.
	//   Should be decodable as a `T::Balance`. Traps otherwise.
	// - value_len: length of the value buffer.
	// - input_data_ptr: a pointer to a buffer to be used as input data to the initializer code.
	// - input_data_len: length of the input data buffer.
	ext_create(
		ctx,
		init_code_ptr: u32,
		init_code_len: u32,
		gas: u64,
		value_ptr: u32,
		value_len: u32,
		input_data_ptr: u32,
		input_data_len: u32
	) -> u32 => {
		let code_hash = {
			let code_hash_buf = read_sandbox_memory(ctx, init_code_ptr, init_code_len)?;
			<CodeHash<<E as Ext>::T>>::decode(&mut &code_hash_buf[..]).ok_or_else(|| sandbox::HostError)?
		};
		let value = {
			let value_buf = read_sandbox_memory(ctx, value_ptr, value_len)?;
			BalanceOf::<<E as Ext>::T>::decode(&mut &value_buf[..])
				.ok_or_else(|| sandbox::HostError)?
		};
		let input_data = read_sandbox_memory(ctx, input_data_ptr, input_data_len)?;

		// Clear the scratch buffer in any case.
		ctx.scratch_buf.clear();

		let nested_gas_limit = if gas == 0 {
			ctx.gas_meter.gas_left()
		} else {
			gas.saturated_into()
		};
		let ext = &mut ctx.ext;
		let instantiate_outcome = ctx.gas_meter.with_nested(nested_gas_limit, |nested_meter| {
			match nested_meter {
				Some(nested_meter) => {
					ext.instantiate(
						&code_hash,
						value,
						nested_meter,
						&input_data
					)
					.map_err(|_| ())
				}
				// there is not enough gas to allocate for the nested call.
				None => Err(()),
			}
		});
		match instantiate_outcome {
			Ok(InstantiateReceipt { address }) => {
				// Write the address to the scratch buffer.
				address.encode_to(&mut ctx.scratch_buf);
				Ok(0)
			},
			Err(_) => Ok(1),
		}
	},

	// Save a data buffer as a result of the execution, terminate the execution and return a
	// successful result to the caller.
	//
	// This is the only way to return a data buffer to the caller.
	ext_return(ctx, data_ptr: u32, data_len: u32) => {
		match ctx
			.gas_meter
			.charge(
				ctx.schedule,
				RuntimeToken::ReturnData(data_len)
			)
		{
			GasMeterResult::Proceed => (),
			GasMeterResult::OutOfGas => return Err(sandbox::HostError),
		}

		let empty_output_buf = ctx
			.empty_output_buf
			.take()
			.expect(
				"`empty_output_buf` is taken only here;
				`ext_return` traps;
				`Runtime` can only be used only for one execution;
				qed"
			);
		let output_buf = empty_output_buf.fill(
			data_len as usize,
			|slice_mut| {
				// Read the memory at the specified pointer to the provided slice.
				ctx.memory.get(data_ptr, slice_mut)
			}
		)?;
		ctx.special_trap = Some(SpecialTrap::Return(output_buf));

		// The trap mechanism is used to immediately terminate the execution.
		// This trap should be handled appropriately before returning the result
		// to the user of this crate.
		Err(sandbox::HostError)
	},

	// Stores the address of the caller into the scratch buffer.
	//
	// If this is a top-level call (i.e. initiated by an extrinsic) the origin address of the extrinsic
	// will be returned. Otherwise, if this call is initiated by another contract then the address
	// of the contract will be returned.
	ext_caller(ctx) => {
		ctx.scratch_buf.clear();
		ctx.ext.caller().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Stores the address of the current contract into the scratch buffer.
	ext_address(ctx) => {
		ctx.scratch_buf.clear();
		ctx.ext.address().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Stores the gas price for the current transaction into the scratch buffer.
	//
	// The data is encoded as T::Balance. The current contents of the scratch buffer are overwritten.
	ext_gas_price(ctx) => {
		ctx.scratch_buf.clear();
		ctx.gas_meter.gas_price().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Stores the amount of gas left into the scratch buffer.
	//
	// The data is encoded as Gas. The current contents of the scratch buffer are overwritten.
	ext_gas_left(ctx) => {
		ctx.scratch_buf.clear();
		ctx.gas_meter.gas_left().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Stores the balance of the current account into the scratch buffer.
	//
	// The data is encoded as T::Balance. The current contents of the scratch buffer are overwritten.
	ext_balance(ctx) => {
		ctx.scratch_buf.clear();
		ctx.ext.balance().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Stores the value transferred along with this call or as endowment into the scratch buffer.
	//
	// The data is encoded as T::Balance. The current contents of the scratch buffer are overwritten.
	ext_value_transferred(ctx) => {
		ctx.scratch_buf.clear();
		ctx.ext.value_transferred().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Stores the random number for the current block for the given subject into the scratch
	// buffer.
	//
	// The data is encoded as T::Hash. The current contents of the scratch buffer are
	// overwritten.
	ext_random(ctx, subject_ptr: u32, subject_len: u32) => {
		// The length of a subject can't exceed `max_subject_len`.
		if subject_len > ctx.schedule.max_subject_len {
			return Err(sandbox::HostError);
		}

		let subject_buf = read_sandbox_memory(ctx, subject_ptr, subject_len)?;
		ctx.scratch_buf.clear();
		ctx.ext.random(&subject_buf).encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Load the latest block timestamp into the scratch buffer
	ext_now(ctx) => {
		ctx.scratch_buf.clear();
		ctx.ext.now().encode_to(&mut ctx.scratch_buf);
		Ok(())
	},

	// Decodes the given buffer as a `T::Call` and adds it to the list
	// of to-be-dispatched calls.
	//
	// All calls made it to the top-level context will be dispatched before
	// finishing the execution of the calling extrinsic.
	ext_dispatch_call(ctx, call_ptr: u32, call_len: u32) => {
		let call = {
			let call_buf = read_sandbox_memory(ctx, call_ptr, call_len)?;
			<<<E as Ext>::T as Trait>::Call>::decode(&mut &call_buf[..])
				.ok_or_else(|| sandbox::HostError)?
		};

		// Charge gas for dispatching this call.
		let fee = {
			let balance_fee = <<E as Ext>::T as Trait>::ComputeDispatchFee::compute_dispatch_fee(&call);
			approx_gas_for_balance(ctx.gas_meter.gas_price(), balance_fee)
		};
		charge_gas(&mut ctx.gas_meter, ctx.schedule, RuntimeToken::ComputedDispatchFee(fee))?;

		ctx.ext.note_dispatch_call(call);

		Ok(())
	},

	// Returns the size of the scratch buffer.
	//
	// For more details on the scratch buffer see `ext_scratch_copy`.
	ext_scratch_size(ctx) -> u32 => {
		Ok(ctx.scratch_buf.len() as u32)
	},

	// Copy data from the scratch buffer starting from `offset` with length `len` into the contract memory.
	// The region at which the data should be put is specified by `dest_ptr`.
	//
	// In order to get size of the scratch buffer use `ext_scratch_size`. At the start of contract
	// execution, the scratch buffer is filled with the input data. Whenever a contract calls
	// function that uses the scratch buffer the contents of the scratch buffer are overwritten.
	ext_scratch_copy(ctx, dest_ptr: u32, offset: u32, len: u32) => {
		let offset = offset as usize;
		if offset > ctx.scratch_buf.len() {
			// Offset can't be larger than scratch buffer length.
			return Err(sandbox::HostError);
		}

		// This can't panic since `offset <= ctx.scratch_buf.len()`.
		let src = &ctx.scratch_buf[offset..];
		if src.len() != len as usize {
			return Err(sandbox::HostError);
		}

		// Finally, perform the write.
		write_sandbox_memory(
			ctx.schedule,
			ctx.gas_meter,
			&ctx.memory,
			dest_ptr,
			src,
		)?;

		Ok(())
	},

	// Deposit a contract event with the data buffer and optional list of topics. There is a limit
	// on the maximum number of topics specified by `max_event_topics`.
	//
	// - topics_ptr - a pointer to the buffer of topics encoded as `Vec<T::Hash>`. The value of this
	//   is ignored if `topics_len` is set to 0. The topics list can't contain duplicates.
	// - topics_len - the length of the topics buffer. Pass 0 if you want to pass an empty vector.
	// - data_ptr - a pointer to a raw data buffer which will saved along the event.
	// - data_len - the length of the data buffer.
	ext_deposit_event(ctx, topics_ptr: u32, topics_len: u32, data_ptr: u32, data_len: u32) => {
		let mut topics = match topics_len {
			0 => Vec::new(),
			_ => {
				let topics_buf = read_sandbox_memory(ctx, topics_ptr, topics_len)?;
				Vec::<TopicOf<<E as Ext>::T>>::decode(&mut &topics_buf[..])
					.ok_or_else(|| sandbox::HostError)?
			}
		};

		// If there are more than `max_event_topics`, then trap.
		if topics.len() > ctx.schedule.max_event_topics as usize {
			return Err(sandbox::HostError);
		}

		// Check for duplicate topics. If there are any, then trap.
		if has_duplicates(&mut topics) {
			return Err(sandbox::HostError);
		}

		let event_data = read_sandbox_memory(ctx, data_ptr, data_len)?;

		match ctx
			.gas_meter
			.charge(
				ctx.schedule,
				RuntimeToken::DepositEvent(topics.len() as u32, data_len)
			)
		{
			GasMeterResult::Proceed => (),
			GasMeterResult::OutOfGas => return Err(sandbox::HostError),
		}
		ctx.ext.deposit_event(topics, event_data);

		Ok(())
	},

	// Set rent allowance of the contract
	//
	// - value_ptr: a pointer to the buffer with value, how much to allow for rent
	//   Should be decodable as a `T::Balance`. Traps otherwise.
	// - value_len: length of the value buffer.
	ext_set_rent_allowance(ctx, value_ptr: u32, value_len: u32) => {
		let value = {
			let value_buf = read_sandbox_memory(ctx, value_ptr, value_len)?;
			BalanceOf::<<E as Ext>::T>::decode(&mut &value_buf[..])
				.ok_or_else(|| sandbox::HostError)?
		};
		ctx.ext.set_rent_allowance(value);

		Ok(())
	},

	// Stores the rent allowance into the scratch buffer.
	//
	// The data is encoded as T::Balance. The current contents of the scratch buffer are overwritten.
	ext_rent_allowance(ctx) => {
		ctx.scratch_buf.clear();
		ctx.ext.rent_allowance().encode_to(&mut ctx.scratch_buf);

		Ok(())
	},

	// Prints utf8 encoded string from the data buffer.
	// Only available on `--dev` chains.
	// This function may be removed at any time, superseded by a more general contract debugging feature.
	ext_println(ctx, str_ptr: u32, str_len: u32) => {
		let data = read_sandbox_memory(ctx, str_ptr, str_len)?;
		if let Ok(utf8) = core::str::from_utf8(&data) {
			runtime_io::print(utf8);
		}
		Ok(())
	},
);

/// Finds duplicates in a given vector.
///
/// This function has complexity of O(n log n) and no additional memory is required, although
/// the order of items is not preserved.
fn has_duplicates<T: PartialEq + AsRef<[u8]>>(items: &mut Vec<T>) -> bool {
	// Sort the vector
	items.sort_unstable_by(|a, b| {
		Ord::cmp(a.as_ref(), b.as_ref())
	});
	// And then find any two consecutive equal elements.
	items.windows(2).any(|w| {
		match w {
			&[ref a, ref b] => a == b,
			_ => false,
		}
	})
}
