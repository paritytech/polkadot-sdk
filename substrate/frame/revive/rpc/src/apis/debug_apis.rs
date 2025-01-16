use crate::*;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

/// Debug Ethererum JSON-RPC apis.
#[rpc(server, client)]
pub trait DebugRpc {
	/// Returns the tracing of the execution of a specific block using its number.
	///
	/// ## References
	///
	/// - https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtraceblockbynumber
	/// - https://docs.alchemy.com/reference/what-is-trace_block
	/// - https://docs.chainstack.com/reference/ethereum-traceblockbynumber
	#[method(name = "debug_traceBlockByNumber")]
	async fn trace_block_by_number(
		&self,
		block: BlockNumberOrTag,
		tracer_config: TracerConfig,
	) -> RpcResult<Vec<TransactionTrace>>;

	/// Returns a transaction's traces by replaying it. This method provides a detailed
	/// breakdown of every step in the execution of a transaction
	///
	/// ## References
	///
	/// - https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracetransaction
	/// - https://docs.alchemy.com/reference/debug-tracetransaction
	/// - https://docs.chainstack.com/reference/ethereum-tracetransaction
	#[method(name = "debug_traceTransaction")]
	async fn trace_transaction(
		&self,
		transaction_hash: H256,
		tracer_config: TracerConfig,
	) -> RpcResult<EthTraces>;
}

pub struct DebugRpcServerImpl {
	client: client::Client,
}

impl DebugRpcServerImpl {
	pub fn new(client: client::Client) -> Self {
		Self { client }
	}
}

#[async_trait]
impl DebugRpcServer for DebugRpcServerImpl {
	async fn trace_block_by_number(
		&self,
		block: BlockNumberOrTag,
		tracer_config: TracerConfig,
	) -> RpcResult<Vec<TransactionTrace>> {
		log::debug!(target: crate::LOG_TARGET, "trace_block_by_number: {block:?} config: {tracer_config:?}");
		let traces = self.client.trace_block_by_number(block, tracer_config).await?;
		Ok(traces)
	}

	async fn trace_transaction(
		&self,
		transaction_hash: H256,
		tracer_config: TracerConfig,
	) -> RpcResult<EthTraces> {
		let traces = self.client.trace_transaction(transaction_hash, tracer_config).await?;
		Ok(traces)
	}
}
