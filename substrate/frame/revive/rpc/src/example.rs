//! Example utilities
#![cfg(feature = "example")]

use crate::{EthRpcClient, ReceiptInfo};
use anyhow::Context;
use jsonrpsee::http_client::HttpClient;
use pallet_revive::evm::{
	rlp::*, BlockTag, Bytes, GenericTransaction, TransactionLegacySigned,
	TransactionLegacyUnsigned, H160, H256, U256,
};

/// A simple account that can sign transactions
pub struct Account(subxt_signer::eth::Keypair);

impl Default for Account {
	fn default() -> Self {
		Self(subxt_signer::eth::dev::alith())
	}
}

/// Wait for a transaction receipt.
pub async fn wait_for_receipt(client: &HttpClient, hash: H256) -> anyhow::Result<ReceiptInfo> {
	for _ in 0..6 {
		tokio::time::sleep(std::time::Duration::from_secs(2)).await;
		let receipt = client.get_transaction_receipt(hash).await?;
		if let Some(receipt) = receipt {
			return Ok(receipt)
		}
	}

	anyhow::bail!("Failed to get receipt")
}

impl Account {
	/// Get the [`H160`] address of the account.
	pub fn address(&self) -> H160 {
		H160::from_slice(&self.0.account_id().as_ref())
	}

	/// Sign a transaction.
	pub fn sign_transaction(&self, tx: TransactionLegacyUnsigned) -> TransactionLegacySigned {
		let rlp_encoded = tx.rlp_bytes();
		let signature = self.0.sign(&rlp_encoded);
		TransactionLegacySigned::from(tx, signature.as_ref())
	}

	/// Send a transaction.
	pub async fn send_transaction(
		&self,
		client: &HttpClient,
		value: U256,
		input: Bytes,
		to: Option<H160>,
	) -> anyhow::Result<H256> {
		let from = self.address();

		let chain_id = Some(client.chain_id().await?);

		let gas_price = client.gas_price().await?;
		let nonce = client
			.get_transaction_count(from, BlockTag::Latest.into())
			.await
			.with_context(|| "Failed to fetch account nonce")?;

		let gas = client
			.estimate_gas(
				GenericTransaction {
					from: Some(from),
					input: Some(input.clone()),
					value: Some(value),
					gas_price: Some(gas_price),
					to,
					..Default::default()
				},
				None,
			)
			.await
			.with_context(|| "Failed to fetch gas estimate")?;

		println!("Estimated Gas: {gas:?}");

		let unsigned_tx = TransactionLegacyUnsigned {
			gas,
			nonce,
			to,
			value,
			input,
			gas_price,
			chain_id,
			..Default::default()
		};

		let tx = self.sign_transaction(unsigned_tx.clone());
		let bytes = tx.rlp_bytes().to_vec();

		let hash = client
			.send_raw_transaction(bytes.clone().into())
			.await
			.with_context(|| "transaction failed")?;

		Ok(hash)
	}
}
