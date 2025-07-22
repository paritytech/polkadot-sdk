use super::Context;
use crate::{vm::Ext, RuntimeCosts};
use revm::{
	interpreter::{gas as revm_gas, host::Host, interpreter_types::RuntimeFlag},
	primitives::{hardfork::SpecId::*, U256},
};

/// EIP-1344: ChainID opcode
pub fn chainid<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, ISTANBUL);
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.chain_id());
}

/// Implements the COINBASE instruction.
///
/// Pushes the current block's beneficiary address onto the stack.
pub fn coinbase<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.beneficiary().into_word().into());
}

/// Implements the TIMESTAMP instruction.
///
/// Pushes the current block's timestamp onto the stack.
pub fn timestamp<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.timestamp());
}

/// Implements the NUMBER instruction.
///
/// Pushes the current block number onto the stack.
pub fn block_number<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_new!(context.interpreter, RuntimeCosts::BlockNumber);
	let block_number = context.interpreter.extend.block_number();
	push!(context.interpreter, U256::from_limbs(block_number.0));
}

/// Implements the DIFFICULTY/PREVRANDAO instruction.
///
/// Pushes the block difficulty (pre-merge) or prevrandao (post-merge) onto the stack.
pub fn difficulty<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, revm_gas::BASE);
	if context.interpreter.runtime_flag.spec_id().is_enabled_in(MERGE) {
		// Unwrap is safe as this fields is checked in validation handler.
		push!(context.interpreter, context.host.prevrandao().unwrap());
	} else {
		push!(context.interpreter, context.host.difficulty());
	}
}

/// Implements the GASLIMIT instruction.
///
/// Pushes the current block's gas limit onto the stack.
pub fn gaslimit<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.gas_limit());
}

/// EIP-3198: BASEFEE opcode
pub fn basefee<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, LONDON);
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.basefee());
}

/// EIP-7516: BLOBBASEFEE opcode
pub fn blob_basefee<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CANCUN);
	gas!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, context.host.blob_gasprice());
}
