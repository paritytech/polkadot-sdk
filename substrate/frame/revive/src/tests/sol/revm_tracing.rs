use crate::{
	evm::{get_opcode_byte, Bytes, OpcodeStep, OpcodeTrace, OpcodeTracerConfig},
	U256,
};
use alloy_core::hex;
use alloy_rpc_types_trace::geth::{DefaultFrame, GethDefaultTracingOptions};
use revm::{
	context::{ContextTr, TxEnv},
	context_interface::TransactTo,
	database::CacheDB,
	database_interface::{DatabaseRef, EmptyDB},
	primitives::Address,
	Context, ExecuteCommitEvm, InspectEvm, MainBuilder, MainContext,
};

use revm_inspectors::tracing::{TracingInspector, TracingInspectorConfig};

#[derive(Debug, Default, Clone)]
pub struct RevmTracer {
	db: CacheDB<EmptyDB>,
	// inspector: TracingInspector,
	config: OpcodeTracerConfig,
}

impl<Gas: Default> From<DefaultFrame> for OpcodeTrace<Gas> {
	fn from(frame: DefaultFrame) -> Self {
		let mut struct_logs = Vec::with_capacity(frame.struct_logs.len());
		for log in frame.struct_logs {
			struct_logs.push(OpcodeStep {
				pc: log.pc,
				op: get_opcode_byte(&log.op).unwrap_or_default(),
				depth: log.depth as u32,
				error: log.error,
				stack: log.stack.unwrap_or_default().iter().map(|s| U256(s.into_limbs())).collect(),
				return_data: log.return_data.unwrap_or_default().0.to_vec().into(),
				memory: log
					.memory
					.unwrap_or_default()
					.iter()
					.map(|m| Bytes(hex::decode(m).unwrap_or_default()))
					.collect(),
				storage: log
					.storage
					.unwrap_or_default()
					.iter()
					.map(|(k, v)| (Bytes(k.0.to_vec()), Bytes(v.0.to_vec())))
					.collect(),
				..Default::default()
			});
		}
		Self {
			struct_logs,
			failed: frame.failed,
			return_value: frame.return_value.to_vec().into(),
			..Default::default()
		}
	}
}

impl RevmTracer {
	pub fn new(config: OpcodeTracerConfig) -> Self {
		Self { db: Default::default(), config }
	}

	fn get_nonce(&self, address: Address) -> u64 {
		match self.db.basic_ref(address) {
			Ok(Some(account_info)) => account_info.nonce,
			_ => 0,
		}
	}

	pub fn deploy(&mut self, tx: TxEnv) -> Address {
		let mut evm = Context::mainnet().with_db(self.db.clone()).build_mainnet();
		let tx = TxEnv {
			gas_limit: 1000000,
			kind: TransactTo::Create,
			nonce: self.get_nonce(tx.caller),
			..tx
		};
		let out = evm.transact_commit(tx).unwrap();
		assert!(out.is_success(), "Contract deployment failed");
		self.db = evm.db().clone();
		out.created_address().unwrap()
	}

	pub fn call(&mut self, tx: TxEnv) -> DefaultFrame {
		let tx = TxEnv { nonce: self.get_nonce(tx.caller), ..tx };
		let mut inspector = TracingInspector::new(TracingInspectorConfig::from_geth_config(
			&self.config.clone().into(),
		));

		let evm = Context::mainnet().with_db(self.db.clone()).build_mainnet();
		let mut evm = evm.clone().build_mainnet_with_inspector(&mut inspector);
		let res = evm.inspect_tx(tx).unwrap();
		assert!(res.result.is_success());
		self.db = evm.db().clone();

		let trace = inspector
			.clone()
			.with_transaction_gas_used(res.result.gas_used())
			.geth_builder()
			.geth_traces(
				res.result.gas_used(),
				res.result.output().unwrap_or_default().clone(),
				self.config.clone().into(),
			);

		trace
	}
}

impl From<OpcodeTracerConfig> for GethDefaultTracingOptions {
	fn from(config: OpcodeTracerConfig) -> Self {
		GethDefaultTracingOptions::default()
			.with_enable_memory(config.enable_memory)
			.with_disable_stack(config.disable_stack)
			.with_disable_stack(config.disable_stack)
			.with_enable_return_data(config.enable_return_data)
	}
}
