use core::cmp::max;
use revm::{
	interpreter::{
		gas as revm_gas,
		interpreter_types::{InterpreterTypes, MemoryTr, RuntimeFlag, StackTr},
		InstructionContext,
	},
	primitives::U256,
};

/// Implements the MLOAD instruction.
///
/// Loads a 32-byte word from memory.
pub fn mload<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
	gas!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([], top, context.interpreter);
	let offset = as_usize_or_fail!(context.interpreter, top);
	resize_memory!(context.interpreter, offset, 32);
	*top =
		U256::try_from_be_slice(context.interpreter.memory.slice_len(offset, 32).as_ref()).unwrap()
}

/// Implements the MSTORE instruction.
///
/// Stores a 32-byte word to memory.
pub fn mstore<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
	gas!(context.interpreter, revm_gas::VERYLOW);
	popn!([offset, value], context.interpreter);
	let offset = as_usize_or_fail!(context.interpreter, offset);
	resize_memory!(context.interpreter, offset, 32);
	context.interpreter.memory.set(offset, &value.to_be_bytes::<32>());
}

/// Implements the MSTORE8 instruction.
///
/// Stores a single byte to memory.
pub fn mstore8<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
	gas!(context.interpreter, revm_gas::VERYLOW);
	popn!([offset, value], context.interpreter);
	let offset = as_usize_or_fail!(context.interpreter, offset);
	resize_memory!(context.interpreter, offset, 1);
	context.interpreter.memory.set(offset, &[value.byte(0)]);
}

/// Implements the MSIZE instruction.
///
/// Gets the size of active memory in bytes.
pub fn msize<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.interpreter.memory.size()));
}

/// Implements the MCOPY instruction.
///
/// EIP-5656: Memory copying instruction that copies memory from one location to another.
pub fn mcopy<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
	check!(context.interpreter, CANCUN);
	popn!([dst, src, len], context.interpreter);

	// Into usize or fail
	let len = as_usize_or_fail!(context.interpreter, len);
	// Deduce gas
	gas_or_fail!(context.interpreter, revm_gas::copy_cost_verylow(len));
	if len == 0 {
		return;
	}

	let dst = as_usize_or_fail!(context.interpreter, dst);
	let src = as_usize_or_fail!(context.interpreter, src);
	// Resize memory
	resize_memory!(context.interpreter, max(dst, src), len);
	// Copy memory in place
	context.interpreter.memory.copy(dst, src, len);
}
