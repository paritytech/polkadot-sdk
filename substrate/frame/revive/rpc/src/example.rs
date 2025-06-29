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
use pallet_revive::evm::*;
use std::sync::Arc;

/// Transaction builder.
pub struct TransactionBuilder<Client: EthRpcClient + Sync + Send> {
	client: Arc<Client>,
	signer: Account,
	value: U256,
	input: Bytes,
	to: Option<H160>,
	mutate: Box<dyn FnOnce(&mut TransactionLegacyUnsigned)>,
}

#[derive(Debug)]
pub struct SubmittedTransaction<Client: EthRpcClient + Sync + Send> {
	tx: GenericTransaction,
	hash: H256,
	client: Arc<Client>,
}

impl<Client: EthRpcClient + Sync + Send> SubmittedTransaction<Client> {
	/// Get the hash of the transaction.
	pub fn hash(&self) -> H256 {
		self.hash
	}

	/// The gas sent with the transaction.
	pub fn gas(&self) -> U256 {
		self.tx.gas.unwrap()
	}

	/// Wait for the receipt of the transaction.
	pub async fn wait_for_receipt(&self) -> anyhow::Result<ReceiptInfo> {
		let hash = self.hash();
		for _ in 0..30 {
			tokio::time::sleep(std::time::Duration::from_secs(2)).await;
			let receipt = self.client.get_transaction_receipt(hash).await?;
			if let Some(receipt) = receipt {
				if receipt.is_success() {
					assert!(
						self.gas() > receipt.gas_used,
						"Gas used should be less than gas estimated."
					);
					return Ok(receipt)
				} else {
					anyhow::bail!("Transaction failed")
				}
			}
		}

		anyhow::bail!("Timeout, failed to get receipt")
	}
}

impl<Client: EthRpcClient + Send + Sync> TransactionBuilder<Client> {
	pub fn new(client: &Arc<Client>) -> Self {
		Self {
			client: Arc::clone(client),
			signer: Account::default(),
			value: U256::zero(),
			input: Bytes::default(),
			to: None,
			mutate: Box::new(|_| {}),
		}
	}
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
	pub async fn eth_call(self) -> anyhow::Result<Vec<u8>> {
		let TransactionBuilder { client, signer, value, input, to, .. } = self;

		let from = signer.address();
		let result = client
			.call(
				GenericTransaction {
					from: Some(from),
					input: input.into(),
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
	pub async fn send(self) -> anyhow::Result<SubmittedTransaction<Client>> {
		let TransactionBuilder { client, signer, value, input, to, mutate } = self;

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
					input: input.clone().into(),
					value: Some(value),
					gas_price: Some(gas_price),
					to,
					..Default::default()
				},
				None,
			)
			.await
			.with_context(|| "Failed to fetch gas estimate")?;

		println!("Gas estimate: {gas:?}");
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

		let signed_tx = signer.sign_transaction(unsigned_tx.into());
		let bytes = signed_tx.signed_payload();

		let hash = client
			.send_raw_transaction(bytes.into())
			.await
			.with_context(|| "transaction failed")?;

		Ok(SubmittedTransaction {
			tx: GenericTransaction::from_signed(signed_tx, gas_price, Some(from)),
			hash,
			client,
		})
	}
}

#[test]
fn test_dummy_payload_has_correct_len() {
	let signer = Account::from(subxt_signer::eth::dev::ethan());
	let unsigned_tx: TransactionUnsigned =
		TransactionLegacyUnsigned { input: vec![42u8; 100].into(), ..Default::default() }.into();

	let signed_tx = signer.sign_transaction(unsigned_tx.clone());
	let signed_payload = signed_tx.signed_payload();
	let unsigned_tx = signed_tx.unsigned();

	let dummy_payload = unsigned_tx.dummy_signed_payload();
	assert_eq!(dummy_payload.len(), signed_payload.len());
}
