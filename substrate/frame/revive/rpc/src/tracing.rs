use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
};

#[rpc(server)]
#[async_trait]
pub trait ReviveTracingApi {
	#[method(name = "debug_traceTransaction")]
	async fn trace_transaction(&self) -> RpcResult<bool>;
}

#[derive(Debug, Clone, Default)]
pub struct ReviveTracing;

#[async_trait]
impl ReviveTracingApiServer for ReviveTracing {
	async fn trace_transaction(&self) -> RpcResult<bool> {
		Ok(true)
	}
}
