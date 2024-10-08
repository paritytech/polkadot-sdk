//! The [`EthRpcServer`] RPC server implementation
#![cfg_attr(docsrs, feature(doc_cfg))]

use client::{ClientError, GAS_PRICE};
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	types::{ErrorCode, ErrorObjectOwned},
};
pub use pallet_revive::{evm::*, EthContractResult};
pub use sp_core::{H160, H256, U256};
use thiserror::Error;
pub mod client;
pub mod example;
pub mod subxt_client;

mod rpc_methods_gen;
pub use rpc_methods_gen::*;

pub const LOG_TARGET: &str = "eth-rpc";

/// Additional RPC methods, exposed on the RPC server on top of all the eth_xxx methods.
#[rpc(server, client)]
pub trait MiscRpc {
	/// Returns the health status of the server.
	#[method(name = "healthcheck")]
	async fn healthcheck(&self) -> RpcResult<()>;
}

/// An EVM RPC server implementation.
pub struct EthRpcServerImpl {
	client: client::Client,
}

impl EthRpcServerImpl {
	/// Creates a new [`EthRpcServerImpl`].
	pub fn new(client: client::Client) -> Self {
		Self { client }
	}
}

/// The error type for the EVM RPC server.
#[derive(Error, Debug)]
pub enum EthRpcError {
	/// A [`ClientError`] wrapper error.
	#[error("Client error: {0}")]
	ClientError(#[from] ClientError),
	/// A [`rlp::DecoderError`] wrapper error.
	#[error("Decoding error: {0}")]
	RlpError(#[from] rlp::DecoderError),
	/// A Decimals conversion error.
	#[error("Conversion error")]
	ConversionError,
	/// An invalid signature error.
	#[error("Invalid signature")]
	InvalidSignature,
	/// The account was not found at the given address
	#[error("Account not found for address {0:?}")]
	AccountNotFound(H160),
}

impl From<EthRpcError> for ErrorObjectOwned {
	fn from(value: EthRpcError) -> Self {
		let code = match value {
			EthRpcError::ClientError(_) => ErrorCode::InternalError,
			_ => ErrorCode::InvalidRequest,
		};
		Self::owned::<String>(code.code(), value.to_string(), None)
	}
}

#[async_trait]
impl EthRpcServer for EthRpcServerImpl {
	async fn net_version(&self) -> RpcResult<String> {
		Ok(self.client.chain_id().to_string())
	}

	async fn block_number(&self) -> RpcResult<U256> {
		let number = self.client.block_number().await?;
		Ok(number.into())
	}

	async fn get_transaction_receipt(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<ReceiptInfo>> {
		let receipt = self.client.receipt(&transaction_hash).await;
		Ok(receipt)
	}

	async fn estimate_gas(
		&self,
		transaction: GenericTransaction,
		_block: Option<BlockNumberOrTag>,
	) -> RpcResult<U256> {
		let result = self.client.estimate_gas(&transaction, BlockTag::Latest.into()).await?;
		Ok(result)
	}

	async fn send_raw_transaction(&self, transaction: Bytes) -> RpcResult<H256> {
		let tx = rlp::decode::<TransactionLegacySigned>(&transaction.0).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to decode transaction: {err:?}");
			EthRpcError::from(err)
		})?;

		let eth_addr = tx.recover_eth_address().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to recover eth address: {err:?}");
			EthRpcError::InvalidSignature
		})?;

		// Dry run the transaction to get the weight limit and storage deposit limit
		let TransactionLegacyUnsigned { to, input, value, .. } = tx.transaction_legacy_unsigned;
		let dry_run = self
			.client
			.dry_run(
				&GenericTransaction {
					from: Some(eth_addr),
					input: Some(input.clone()),
					to,
					value: Some(value),
					..Default::default()
				},
				BlockTag::Latest.into(),
			)
			.await?;

		let EthContractResult { transact_kind, gas_limit, storage_deposit, .. } = dry_run;
		let call = subxt_client::tx().revive().eth_transact(
			transaction.0,
			gas_limit.into(),
			storage_deposit,
			transact_kind.into(),
		);
		let ext = self.client.tx().create_unsigned(&call).map_err(|err| ClientError::from(err))?;
		let hash = ext.submit().await.map_err(|err| EthRpcError::ClientError(err.into()))?;

		Ok(hash)
	}

	async fn get_block_by_hash(
		&self,
		block_hash: H256,
		_hydrated_transactions: bool,
	) -> RpcResult<Option<Block>> {
		let Some(block) = self.client.block_by_hash(&block_hash).await? else {
			return Ok(None);
		};
		let block = self.client.evm_block(block).await?;
		Ok(Some(block))
	}

	async fn get_balance(
		&self,
		address: Address,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<U256> {
		let balance = self.client.balance(address, &block).await?;
		log::debug!(target: LOG_TARGET, "balance({address}): {balance:?}");
		Ok(balance)
	}

	async fn chain_id(&self) -> RpcResult<U256> {
		Ok(self.client.chain_id().into())
	}

	async fn gas_price(&self) -> RpcResult<U256> {
		Ok(U256::from(GAS_PRICE))
	}

	async fn get_code(&self, address: Address, block: BlockNumberOrTagOrHash) -> RpcResult<Bytes> {
		let code = self.client.get_contract_code(&address, block).await?;
		Ok(code.into())
	}

	async fn accounts(&self) -> RpcResult<Vec<Address>> {
		Ok(vec![])
	}

	async fn call(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<Bytes> {
		let dry_run = self
			.client
			.dry_run(&transaction, block.unwrap_or_else(|| BlockTag::Latest.into()))
			.await?;
		let output = dry_run.result.map_err(|err| {
			log::debug!(target: LOG_TARGET, "Dry run failed: {err:?}");
			ClientError::DryRunFailed
		})?;

		Ok(output.into())
	}

	async fn get_block_by_number(
		&self,
		block: BlockNumberOrTag,
		_hydrated_transactions: bool,
	) -> RpcResult<Option<Block>> {
		let Some(block) = self.client.block_by_number_or_tag(&block).await? else {
			return Ok(None);
		};
		let block = self.client.evm_block(block).await?;
		Ok(Some(block))
	}

	async fn get_block_transaction_count_by_hash(
		&self,
		block_hash: Option<H256>,
	) -> RpcResult<Option<U256>> {
		let block_hash = if let Some(block_hash) = block_hash {
			block_hash
		} else {
			self.client.latest_block().await.ok_or(ClientError::BlockNotFound)?.hash()
		};
		Ok(self.client.receipts_count_per_block(&block_hash).await.map(U256::from))
	}

	async fn get_block_transaction_count_by_number(
		&self,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<Option<U256>> {
		let Some(block) = self
			.get_block_by_number(block.unwrap_or_else(|| BlockTag::Latest.into()), false)
			.await?
		else {
			return Ok(None);
		};

		Ok(self.client.receipts_count_per_block(&block.hash).await.map(U256::from))
	}

	async fn get_storage_at(
		&self,
		address: Address,
		storage_slot: U256,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<Bytes> {
		let bytes = self.client.get_contract_storage(address, storage_slot, block).await?;
		Ok(bytes.into())
	}

	async fn get_transaction_by_block_hash_and_index(
		&self,
		block_hash: H256,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(receipt) =
			self.client.receipt_by_hash_and_index(&block_hash, &transaction_index).await
		else {
			return Ok(None);
		};

		let Some(signed_tx) = self.client.signed_tx_by_hash(&receipt.transaction_hash).await else {
			return Ok(None);
		};

		Ok(Some(TransactionInfo::new(receipt, signed_tx)))
	}

	async fn get_transaction_by_block_number_and_index(
		&self,
		block: BlockNumberOrTag,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(block) = self.client.block_by_number_or_tag(&block).await? else {
			return Ok(None);
		};
		self.get_transaction_by_block_hash_and_index(block.hash(), transaction_index)
			.await
	}

	async fn get_transaction_by_hash(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<TransactionInfo>> {
		let receipt = self.client.receipt(&transaction_hash).await;
		let signed_tx = self.client.signed_tx_by_hash(&transaction_hash).await;
		if let (Some(receipt), Some(signed_tx)) = (receipt, signed_tx) {
			return Ok(Some(TransactionInfo::new(receipt, signed_tx)));
		}

		Ok(None)
	}

	async fn get_transaction_count(
		&self,
		address: Address,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<U256> {
		let nonce = self.client.nonce(address, block).await?;
		Ok(nonce.into())
	}
}

/// A [`MiscRpcServer`] RPC server implementation.
pub struct MiscRpcServerImpl;

#[async_trait]
impl MiscRpcServer for MiscRpcServerImpl {
	async fn healthcheck(&self) -> RpcResult<()> {
		Ok(())
	}
}
