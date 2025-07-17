use crate::{
	vm::{ExecResult, ExportedFunction},
	ExecReturnValue,
};
use pallet_revive_uapi::ReturnFlags;
use revm::{
	bytecode::Bytecode,
	context_interface::{
		context::{SStoreResult, SelfDestructResult, StateLoad},
		journaled_state::AccountLoad,
	},
	interpreter::{
		host::Host,
		instruction_table,
		interpreter::{EthInterpreter, ExtBytecode},
		interpreter_action::{
			CallInputs, CreateInputs, CreateOutcome, FrameInput, InterpreterAction,
		},
		interpreter_types::ReturnData,
		CallInput, InputsImpl, Interpreter, InterpreterResult, SharedMemory,
	},
	primitives::{hardfork::SpecId, Address, Bytes, Log, StorageKey, StorageValue, B256, U256},
};

/// TODO handle error case
pub fn call(bytecode: Bytecode, function: ExportedFunction, input_data: Vec<u8>) -> ExecResult {
	let inputs = InputsImpl {
		caller_address: Default::default(),
		target_address: Default::default(),
		call_value: Default::default(),
		bytecode_address: None,
		input: CallInput::Bytes(input_data.into()),
	};

	let mut interpreter = Interpreter::new(
		SharedMemory::new(),
		ExtBytecode::new(bytecode),
		inputs,
		false,
		SpecId::default(),
		1_000_000,
	);

	let table = instruction_table::<EthInterpreter, MockHost>();
	let result = run(&mut interpreter, &table, &mut MockHost::default());

	if result.is_ok() {
		return Ok(ExecReturnValue {
			flags: if result.is_revert() { ReturnFlags::REVERT } else { ReturnFlags::empty() },
			data: result.output.to_vec(),
		})
	}

	todo!()
}

fn run(
	interpreter: &mut Interpreter<EthInterpreter>,
	table: &revm::interpreter::InstructionTable<EthInterpreter, MockHost>,
	host: &mut MockHost,
) -> InterpreterResult {
	loop {
		let action = interpreter.run_plain(table, host);
		match action {
			InterpreterAction::NewFrame(frame_input) => match frame_input {
				FrameInput::Call(input) => {
					let result = host.call(&input);
					interpreter.return_data.set_buffer(result.output.clone());
					let _ = interpreter.stack.push(U256::from(result.result.is_ok() as u8));
				},
				FrameInput::Create(input) => {
					let outcome = host.create(&input);
					let address = outcome.address.unwrap_or_default();
					let _ = interpreter.stack.push(U256::from_be_slice(address.as_slice()));
				},
				FrameInput::Empty => {
					panic!("Unexpected empty frame input");
				},
			},
			InterpreterAction::Return(result) => return result,
		}
	}
}

/// Mock [`Host`] implementation
#[derive(Debug, Default)]
struct MockHost;

impl MockHost {
	/// Mock calling a child contract.
	pub fn call(&mut self, call_inputs: &CallInputs) -> InterpreterResult {
		let mock_result = Bytes::from(U256::from(42u64).to_be_bytes_vec());

		InterpreterResult::new(
			revm::interpreter::InstructionResult::Return,
			mock_result,
			revm::interpreter::Gas::new(call_inputs.gas_limit - 100), // Consume some gas
		)
	}

	/// Mock creating a new contract.
	pub fn create(&mut self, create_inputs: &CreateInputs) -> CreateOutcome {
		// Generate a mock contract address
		let contract_address = Address::from_slice(&[42u8; 20]);

		CreateOutcome::new(
			InterpreterResult::new(
				revm::interpreter::InstructionResult::Return,
				Bytes::default(),
				revm::interpreter::Gas::new(create_inputs.gas_limit - 200), // Consume some gas
			),
			Some(contract_address),
		)
	}
}

impl Host for MockHost {
	fn basefee(&self) -> U256 {
		U256::ZERO
	}
	fn blob_gasprice(&self) -> U256 {
		U256::ZERO
	}
	fn gas_limit(&self) -> U256 {
		U256::from(30_000_000u64)
	}
	fn difficulty(&self) -> U256 {
		U256::ZERO
	}
	fn prevrandao(&self) -> Option<U256> {
		None
	}
	fn block_number(&self) -> U256 {
		U256::from(1u64)
	}
	fn timestamp(&self) -> U256 {
		U256::from(1000u64)
	}
	fn beneficiary(&self) -> Address {
		Address::ZERO
	}
	fn chain_id(&self) -> U256 {
		U256::from(1u64)
	}
	fn effective_gas_price(&self) -> U256 {
		U256::ZERO
	}
	fn caller(&self) -> Address {
		Address::ZERO
	}
	fn blob_hash(&self, _number: usize) -> Option<U256> {
		None
	}
	fn max_initcode_size(&self) -> usize {
		0x40000
	}
	fn block_hash(&mut self, _number: u64) -> Option<B256> {
		None
	}
	fn selfdestruct(
		&mut self,
		_address: Address,
		_target: Address,
	) -> Option<StateLoad<SelfDestructResult>> {
		None
	}
	fn log(&mut self, _log: Log) {}
	fn sstore(
		&mut self,
		_address: Address,
		_key: StorageKey,
		_value: StorageValue,
	) -> Option<StateLoad<SStoreResult>> {
		None
	}
	fn sload(&mut self, _address: Address, _key: StorageKey) -> Option<StateLoad<StorageValue>> {
		None
	}
	fn tstore(&mut self, _address: Address, _key: StorageKey, _value: StorageValue) {}
	fn tload(&mut self, _address: Address, _key: StorageKey) -> StorageValue {
		StorageValue::ZERO
	}
	fn balance(&mut self, _address: Address) -> Option<StateLoad<U256>> {
		None
	}
	fn load_account_delegated(&mut self, _address: Address) -> Option<StateLoad<AccountLoad>> {
		Some(StateLoad::new(AccountLoad { is_delegate_account_cold: None, is_empty: true }, true))
	}
	fn load_account_code(&mut self, _address: Address) -> Option<StateLoad<Bytes>> {
		None
	}
	fn load_account_code_hash(&mut self, _address: Address) -> Option<StateLoad<B256>> {
		None
	}
}
