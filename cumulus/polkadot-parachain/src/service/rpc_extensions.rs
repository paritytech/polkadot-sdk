use crate::service::{
	common_types::{AccountId, Balance, Block, Nonce},
	ParachainBackend, ParachainClient,
};
use sp_api::ConstructRuntimeApi;
use std::sync::Arc;

pub fn build_parachain_rpc_extensions<RuntimeApi>(
	deny_unsafe: sc_rpc::DenyUnsafe,
	client: Arc<ParachainClient<RuntimeApi>>,
	backend: Arc<ParachainBackend>,
	pool: Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
{
	let deps = crate::rpc::FullDeps { client, pool, deny_unsafe };

	crate::rpc::create_full(deps, backend).map_err(Into::into)
}

pub fn build_contracts_rpc_extensions(
	deny_unsafe: sc_rpc::DenyUnsafe,
	client: Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>,
	_backend: Arc<ParachainBackend>,
	pool: Arc<
		sc_transaction_pool::FullPool<
			Block,
			ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>,
		>,
	>,
) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error> {
	let deps = crate::rpc::FullDeps { client: client.clone(), pool: pool.clone(), deny_unsafe };

	crate::rpc::create_contracts_rococo(deps).map_err(Into::into)
}
