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
}

impl RevmTracer {
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
		let mut insp = TracingInspector::new(TracingInspectorConfig::from_geth_config(
			&GethDefaultTracingOptions::default().enable_memory(),
		));

		let evm = Context::mainnet().with_db(self.db.clone()).build_mainnet();
		let mut evm = evm.clone().build_mainnet_with_inspector(&mut insp);
		let tx = TxEnv { nonce: self.get_nonce(tx.caller), ..tx };
		let res = evm.inspect_tx(tx).unwrap();
		assert!(res.result.is_success());
		self.db = evm.db().clone();

		let trace = insp
			.with_transaction_gas_used(res.result.gas_used())
			.geth_builder()
			.geth_traces(
				res.result.gas_used(),
				res.result.output().unwrap_or_default().clone(),
				GethDefaultTracingOptions::default().enable_memory(),
			);

		trace
	}
}
