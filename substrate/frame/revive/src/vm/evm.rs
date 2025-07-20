mod instructions;

use crate::{
	AccountIdOf, BalanceOf, CodeInfo, CodeVec, Config, ContractBlob, DispatchError, Error,
	ExecReturnValue, H256, LOG_TARGET, U256,
	address::AddressMapper,
	exec::PrecompileExt,
	vm::{ExecResult, Ext},
};
use instructions::instruction_table;
use pallet_revive_uapi::ReturnFlags;
use revm::{
	bytecode::Bytecode,
	interpreter::{
		CallInput, Gas, Interpreter, InterpreterResult, InterpreterTypes, SharedMemory, Stack,
		host::DummyHost,
		interpreter::{ExtBytecode, ReturnDataImpl, RuntimeFlags},
		interpreter_action::InterpreterAction,
		interpreter_types::InputsTr,
	},
	primitives::{self, Address, hardfork::SpecId},
};

impl<T: Config> ContractBlob<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
{
	/// Create a new contract from EVM code.
	pub fn from_evm_code(code: Vec<u8>, owner: AccountIdOf<T>) -> Result<Self, DispatchError> {
		use revm::{bytecode::Bytecode, primitives::Bytes};

		let code: CodeVec = code.try_into().map_err(|_| <Error<T>>::BlobTooLarge)?;
		Bytecode::new_raw_checked(Bytes::from(code.to_vec())).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to create evm bytecode from code: {err:?}" );
			<Error<T>>::CodeRejected
		})?;

		let code_len = code.len() as u32;
		let code_info = CodeInfo {
			owner,
			deposit: Default::default(),
			refcount: 0,
			code_len,
			behaviour_version: Default::default(),
		};
		let code_hash = H256(sp_io::hashing::keccak_256(&code));
		Ok(ContractBlob { code, code_info, code_hash })
	}
}

/// TODO handle error case
pub fn call<'a, E: Ext>(bytecode: Bytecode, inputs: EVMInputs<'a, E>) -> ExecResult {
	let mut interpreter: Interpreter<EVMInterpreter<'a, E>> = Interpreter {
		gas: Gas::new(30_000_000), // TODO clean up
		bytecode: ExtBytecode::new(bytecode),
		stack: Stack::new(),
		return_data: Default::default(),
		memory: SharedMemory::new(),
		input: inputs,
		runtime_flag: RuntimeFlags { is_static: false, spec_id: SpecId::default() },
		extend: Default::default(),
	};

	let table = instruction_table::<EVMInterpreter<'a, E>, DummyHost>();
	let result = run(&mut interpreter, &table);

	if result.is_error() {
		Err(Error::<E::T>::ContractTrapped.into())
	} else {
		Ok(ExecReturnValue {
			flags: if result.is_revert() { ReturnFlags::REVERT } else { ReturnFlags::empty() },
			data: result.output.to_vec(),
		})
	}
}

fn run<WIRE: InterpreterTypes>(
	interpreter: &mut Interpreter<WIRE>,
	table: &revm::interpreter::InstructionTable<WIRE, DummyHost>,
) -> InterpreterResult {
	let host = &mut DummyHost {};
	loop {
		let action = interpreter.run_plain(table, host);
		match action {
			InterpreterAction::Return(result) => return result,
			_ => panic!("Unexpected action: {:?}", action),
		}
	}
}

pub struct EVMInterpreter<'a, E: Ext> {
	_phantom: core::marker::PhantomData<&'a E>,
}

impl<'a, E: Ext> InterpreterTypes for EVMInterpreter<'a, E> {
	type Stack = Stack;
	type Memory = SharedMemory;
	type Bytecode = ExtBytecode;
	type ReturnData = ReturnDataImpl;
	type Input = EVMInputs<'a, E>;
	type RuntimeFlag = RuntimeFlags;
	type Extend = ();
	type Output = InterpreterAction;
}

pub struct EVMInputs<'a, E: Ext> {
	ext: &'a mut E,
	input: CallInput,
}

impl<'a, E: Ext> EVMInputs<'a, E> {
	pub fn new(ext: &'a mut E, input: Vec<u8>) -> Self {
		Self { ext, input: CallInput::Bytes(input.into()) }
	}
}

impl<'a, E: Ext> InputsTr for EVMInputs<'a, E> {
	fn target_address(&self) -> Address {
		let address = self.ext.address();
		address.0.into()
	}

	fn caller_address(&self) -> Address {
		let caller = self.ext.caller();
		let Ok(caller) = caller.account_id() else { return Address::ZERO };

		let addr = <<E as PrecompileExt>::T as Config>::AddressMapper::to_address(caller);
		addr.0.into()
	}

	fn bytecode_address(&self) -> Option<&Address> {
		todo!()
	}

	fn input(&self) -> &CallInput {
		&self.input
	}

	fn call_value(&self) -> primitives::U256 {
		primitives::U256::from_limbs(self.ext.value_transferred().0)
	}
}
