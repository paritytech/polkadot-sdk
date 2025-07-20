use revm::interpreter::interpreter_action::InterpreterAction;
use revm::interpreter::{
    gas as revm_gas,
    Interpreter,
    interpreter_types::{InterpreterTypes, Jumps, LoopControl, MemoryTr, RuntimeFlag, StackTr},
    InstructionContext,
    InstructionResult,
};
use revm::primitives::{Bytes, U256};

/// Implements the JUMP instruction.
///
/// Unconditional jump to a valid destination.
pub fn jump<ITy: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, ITy>) {
    gas!(context.interpreter, revm_gas::MID);
    popn!([target], context.interpreter);
    jump_inner(context.interpreter, target);
}

/// Implements the JUMPI instruction.
///
/// Conditional jump to a valid destination if condition is true.
pub fn jumpi<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    gas!(context.interpreter, revm_gas::HIGH);
    popn!([target, cond], context.interpreter);

    if !cond.is_zero() {
        jump_inner(context.interpreter, target);
    }
}

#[inline(always)]
/// Internal helper function for jump operations.
///
/// Validates jump target and performs the actual jump.
fn jump_inner<WIRE: InterpreterTypes>(interpreter: &mut Interpreter<WIRE>, target: U256) {
    let target = as_usize_or_fail!(interpreter, target, InstructionResult::InvalidJump);
    if !interpreter.bytecode.is_valid_legacy_jump(target) {
        interpreter.halt(InstructionResult::InvalidJump);
        return;
    }
    // SAFETY: `is_valid_jump` ensures that `dest` is in bounds.
    interpreter.bytecode.absolute_jump(target);
}

/// Implements the JUMPDEST instruction.
///
/// Marks a valid destination for jump operations.
pub fn jumpdest<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    gas!(context.interpreter, revm_gas::JUMPDEST);
}

/// Implements the PC instruction.
///
/// Pushes the current program counter onto the stack.
pub fn pc<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    gas!(context.interpreter, revm_gas::BASE);
    // - 1 because we have already advanced the instruction pointer in `Interpreter::step`
    push!(
        context.interpreter,
        U256::from(context.interpreter.bytecode.pc() - 1)
    );
}

#[inline]
/// Internal helper function for return operations.
///
/// Handles memory data retrieval and sets the return action.
fn return_inner(
    interpreter: &mut Interpreter<impl InterpreterTypes>,
    instruction_result: InstructionResult,
) {
    // Zero gas cost
    // gas!(interpreter, revm_gas::ZERO)
    popn!([offset, len], interpreter);
    let len = as_usize_or_fail!(interpreter, len);
    // Important: Offset must be ignored if len is zeros
    let mut output = Bytes::default();
    if len != 0 {
        let offset = as_usize_or_fail!(interpreter, offset);
        resize_memory!(interpreter, offset, len);
        output = interpreter.memory.slice_len(offset, len).to_vec().into()
    }

    interpreter
        .bytecode
        .set_action(InterpreterAction::new_return(
            instruction_result,
            output,
            interpreter.gas,
        ));
}

/// Implements the RETURN instruction.
///
/// Halts execution and returns data from memory.
pub fn ret<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    return_inner(context.interpreter, InstructionResult::Return);
}

/// EIP-140: REVERT instruction
pub fn revert<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    check!(context.interpreter, BYZANTIUM);
    return_inner(context.interpreter, InstructionResult::Revert);
}

/// Stop opcode. This opcode halts the execution.
pub fn stop<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    context.interpreter.halt(InstructionResult::Stop);
}

/// Invalid opcode. This opcode halts the execution.
pub fn invalid<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    context.interpreter.halt(InstructionResult::InvalidFEOpcode);
}

/// Unknown opcode. This opcode halts the execution.
pub fn unknown<WIRE: InterpreterTypes, H: ?Sized>(context: InstructionContext<'_, H, WIRE>) {
    context.interpreter.halt(InstructionResult::OpcodeNotFound);
}
