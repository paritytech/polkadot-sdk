use crate::{
	address::AddressMapper,
	exec::PrecompileExt,
	vm::{ExecResult, Ext},
	Config, ExecReturnValue,
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
		interpreter::{ExtBytecode, ReturnDataImpl, RuntimeFlags},
		interpreter_action::{
			CallInputs, CreateInputs, CreateOutcome, FrameInput, InterpreterAction,
		},
		interpreter_types::{InputsTr, ReturnData, StackTr},
		CallInput, Gas, Interpreter, InterpreterResult, InterpreterTypes, SharedMemory, Stack,
	},
	primitives::{hardfork::SpecId, Address, Bytes, Log, StorageKey, StorageValue, B256, U256},
};
use sp_core::H256;

/// TODO handle error case
pub fn call<'a, E: Ext>(bytecode: Bytecode, inputs: EVMInputs<'a, E>) -> ExecResult {
	let mut interpreter: Interpreter<EVMInterpreter<'a, E>> = Interpreter {
		bytecode: ExtBytecode::new(bytecode),
		gas: Gas::new(30_000_000),
		stack: Stack::new(),
		return_data: Default::default(),
		memory: SharedMemory::new(),
		input: inputs,
		runtime_flag: RuntimeFlags { is_static: false, spec_id: SpecId::default() },
		extend: Default::default(),
	};

	let table = instruction_table::<EVMInterpreter<'a, E>, MockHost>();
	let result = run(&mut interpreter, &table, &mut MockHost::default());

	if result.is_ok() {
		return Ok(ExecReturnValue {
			flags: if result.is_revert() { ReturnFlags::REVERT } else { ReturnFlags::empty() },
			data: result.output.to_vec(),
		})
	}

	dbg!(result);
	todo!("Handle error case properly");
}

fn run<WIRE: InterpreterTypes>(
	interpreter: &mut Interpreter<WIRE>,
	table: &revm::interpreter::InstructionTable<WIRE, MockHost>,
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

	fn call_value(&self) -> U256 {
		U256::from_limbs(self.ext.value_transferred().0)
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

pub struct EVMRuntime<'a, E: Ext> {
	ext: &'a mut E,
}

use frame_support::traits::Get;
impl<'a, E: Ext> Host for EVMRuntime<'a, E> {
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
		U256::from_limbs(self.ext.block_number().0)
	}
	fn timestamp(&self) -> U256 {
		U256::from_limbs(self.ext.now().0)
	}
	fn beneficiary(&self) -> Address {
		self.ext.block_author().unwrap_or_default().0.into()
	}
	fn chain_id(&self) -> U256 {
		U256::from(<E::T as Config>::ChainId::get())
	}
	fn effective_gas_price(&self) -> U256 {
		U256::ZERO
	}
	fn caller(&self) -> Address {
		let caller = self.ext.caller();
		let Ok(id) = caller.account_id() else { return Address::default() };
		let addr = <E::T as Config>::AddressMapper::to_address(id);
		addr.0.into()
	}
	fn blob_hash(&self, _number: usize) -> Option<U256> {
		None
	}
	fn max_initcode_size(&self) -> usize {
		0x40000
	}
	fn block_hash(&mut self, number: u64) -> Option<B256> {
		self.ext.block_hash(number.into()).map(|h| B256::from(h.0))
	}
	fn selfdestruct(
		&mut self,
		_address: Address,
		_target: Address,
	) -> Option<StateLoad<SelfDestructResult>> {
		None
	}

	fn log(&mut self, log: Log) {
		let (topics, data) = log.data.split();
		let topics = topics.into_iter().map(|v| H256::from(v.0)).collect();
		self.ext.deposit_event(topics, data.into());
	}

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
