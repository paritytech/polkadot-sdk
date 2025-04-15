//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use parachain_template_runtime::{opaque::Block, AccountId, Balance, Nonce};

use polkadot_sdk::*;

use sc_transaction_pool_api::TransactionPool;
use sp_api::CallApiAt;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};

/// A type representing all RPC extensions.
pub type RpcExtension = jsonrpsee::RpcModule<()>;

/// Full client dependencies
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
}

/// Instantiate all RPC extensions.
pub fn create_full<C, P>(
	deps: FullDeps<C, P>,
) -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>>
where
	C: HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = BlockChainError>
		+ CallApiAt<Block>
		+ Send
		+ Sync
		+ 'static,
	P: TransactionPool + Sync + Send + 'static,
{
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	let mut module = RpcExtension::new(());
	let FullDeps { client, pool } = deps;

	module.merge(
		System::<_, _, _, Nonce, AccountId>::new(client.clone(), pool).into_rpc(),
	)?;
	module.merge(TransactionPayment::<_, _, Balance>::new(client).into_rpc())?;
	Ok(module)
}
