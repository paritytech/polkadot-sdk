// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! Example utilities
use crate::{EthRpcClient, ReceiptInfo};
use anyhow::Context;
use pallet_revive::evm::{
	Account, BlockTag, Bytes, GenericTransaction, TransactionLegacyUnsigned, H160, H256, U256,
};

/// Wait for a transaction receipt.
pub async fn wait_for_receipt(
	client: &(impl EthRpcClient + Send + Sync),
	hash: H256,
) -> anyhow::Result<ReceiptInfo> {
	for _ in 0..30 {
		tokio::time::sleep(std::time::Duration::from_secs(2)).await;
		let receipt = client.get_transaction_receipt(hash).await?;
		if let Some(receipt) = receipt {
			return Ok(receipt)
		}
	}

	anyhow::bail!("Failed to get receipt")
}

/// Wait for a successful transaction receipt.
pub async fn wait_for_successful_receipt(
	client: &(impl EthRpcClient + Send + Sync),
	hash: H256,
) -> anyhow::Result<ReceiptInfo> {
	let receipt = wait_for_receipt(client, hash).await?;
	if receipt.is_success() {
		Ok(receipt)
	} else {
		anyhow::bail!("Transaction failed")
	}
}

/// Transaction builder.
pub struct TransactionBuilder {
	signer: Account,
	value: U256,
	input: Bytes,
	to: Option<H160>,
	mutate: Box<dyn FnOnce(&mut TransactionLegacyUnsigned)>,
}

impl Default for TransactionBuilder {
	fn default() -> Self {
		Self {
			signer: Account::default(),
			value: U256::zero(),
			input: Bytes::default(),
			to: None,
			mutate: Box::new(|_| {}),
		}
	}
}

impl TransactionBuilder {
	/// Set the signer.
	pub fn signer(mut self, signer: Account) -> Self {
		self.signer = signer;
		self
	}

	/// Set the value.
	pub fn value(mut self, value: U256) -> Self {
		self.value = value;
		self
	}

	/// Set the input.
	pub fn input(mut self, input: Vec<u8>) -> Self {
		self.input = Bytes(input);
		self
	}

	/// Set the destination.
	pub fn to(mut self, to: H160) -> Self {
		self.to = Some(to);
		self
	}

	/// Set a mutation function, that mutates the transaction before sending.
	pub fn mutate(mut self, mutate: impl FnOnce(&mut TransactionLegacyUnsigned) + 'static) -> Self {
		self.mutate = Box::new(mutate);
		self
	}

	/// Call eth_call to get the result of a view function
	pub async fn eth_call(
		self,
		client: &(impl EthRpcClient + Send + Sync),
	) -> anyhow::Result<Vec<u8>> {
		let TransactionBuilder { signer, value, input, to, .. } = self;

		let from = signer.address();
		let result = client
			.call(
				GenericTransaction {
					from: Some(from),
					input: Some(input.clone()),
					value: Some(value),
					to,
					..Default::default()
				},
				None,
			)
			.await
			.with_context(|| "eth_call failed")?;
		Ok(result.0)
	}

	/// Send the transaction.
	pub async fn send(self, client: &(impl EthRpcClient + Send + Sync)) -> anyhow::Result<H256> {
		let TransactionBuilder { signer, value, input, to, mutate } = self;

		let from = signer.address();
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

		let mut unsigned_tx = TransactionLegacyUnsigned {
			gas,
			nonce,
			to,
			value,
			input,
			gas_price,
			chain_id,
			..Default::default()
		};

		mutate(&mut unsigned_tx);

		let tx = signer.sign_transaction(unsigned_tx.into());
		let bytes = tx.signed_payload();

		let hash = client
			.send_raw_transaction(bytes.into())
			.await
			.with_context(|| "transaction failed")?;

		Ok(hash)
	}

	/// Send the transaction and wait for the receipt.
	pub async fn send_and_wait_for_receipt(
		self,
		client: &(impl EthRpcClient + Send + Sync),
	) -> anyhow::Result<ReceiptInfo> {
		let hash = self.send(client).await?;
		wait_for_successful_receipt(client, hash).await
	}
}
