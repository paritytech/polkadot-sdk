// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Test utilities for the statement store

use sp_core::{sr25519, Encode, Pair};
use sp_statement_store::{
	statement_allowance_key, Channel, StatementAllowance, Topic,
};
use subxt::{
	config::{Config, DefaultExtrinsicParamsBuilder, TransactionExtensions},
	dynamic::Value,
	ext::scale_value::value,
	transactions::Signer,
	utils::H256,
	OnlineClient,
};

use super::subxt_client::CustomConfig;

/// Generate a deterministic keypair for a given client index
pub fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementClient//{idx}"), None)
		.expect("Derivation path is always valid; qed")
}

/// Create a test statement with the given parameters
pub fn create_test_statement(
	keypair: &sr25519::Pair,
	topics: &[Topic],
	channel: Option<Channel>,
	data: Vec<u8>,
	expiry_ts: u32,
	seq: u32,
) -> sp_statement_store::Statement {
	let mut statement = sp_statement_store::Statement::new();
	for (i, topic) in topics.iter().enumerate() {
		statement.set_topic(i, *topic);
	}
	if let Some(ch) = channel {
		statement.set_channel(ch);
	}
	statement.set_plain_data(data);
	statement.set_expiry_from_parts(expiry_ts, seq);
	statement.sign_sr25519_private(keypair);
	statement
}

/// Creates storage items for custom per-participant allowances
pub fn create_allowance_items(
	allowances: &[(u32, StatementAllowance)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let mut items = Vec::with_capacity(allowances.len());
	for (idx, allowance) in allowances {
		let keypair = get_keypair(*idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance.encode()));
	}
	items
}

/// Creates uniform allowance storage items for a range of participants
pub fn create_uniform_allowance_items(
	count: u32,
	allowance: StatementAllowance,
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let allowance_encoded = allowance.encode();
	let mut items = Vec::with_capacity(count as usize);
	for idx in 0..count {
		let keypair = get_keypair(idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance_encoded.clone()));
	}
	items
}

/// Creates a sudo -> frame_system::set_storage call to set statement allowances
pub fn create_set_storage_call(
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> subxt::transactions::DynamicPayload<Vec<Value>> {
	let items_value: Vec<Value> = items
		.into_iter()
		.map(|(key, value)| value!((Value::from_bytes(key), Value::from_bytes(value))))
		.collect();

	subxt::transactions::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items_value })
		}],
	)
}

/// Builds params for CustomConfig's transaction extensions (9 defaults + RestrictOrigins)
pub fn build_params(
	nonce: u64,
) -> <<CustomConfig as Config>::TransactionExtensions as TransactionExtensions<CustomConfig>>::Params
{
	let (a, b, c, d, e, f, g, h, i) = DefaultExtrinsicParamsBuilder::<CustomConfig>::new()
		.immortal()
		.nonce(nonce)
		.build();
	(a, b, c, d, e, f, g, h, i, ())
}

/// Submits an extrinsic with an explicit nonce and waits for it to be finalized
pub async fn submit_extrinsic<S: Signer<CustomConfig>>(
	client: &OnlineClient<CustomConfig>,
	call: &subxt::transactions::DynamicPayload<Vec<Value>>,
	signer: &S,
	nonce: u64,
) -> Result<H256, anyhow::Error> {
	let tx_in_block = client
		.tx()
		.await?
		.sign_and_submit_then_watch(call, signer, build_params(nonce))
		.await?
		.wait_for_finalized()
		.await?;

	tx_in_block.wait_for_success().await?;
	Ok(tx_in_block.block_hash())
}

/// Gets the current nonce for an account
pub async fn get_account_nonce(
	client: &OnlineClient<CustomConfig>,
	account_id: &<CustomConfig as Config>::AccountId,
) -> Result<u64, anyhow::Error> {
	let nonce = client.tx().await?.account_nonce(account_id).await?;
	Ok(nonce)
}

/// Sets statement allowances via sudo -> frame_system::set_storage extrinsic
pub async fn set_allowances_via_sudo(
	para_client: &OnlineClient<CustomConfig>,
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), anyhow::Error> {
	let alice = subxt_signer::sr25519::dev::alice();
	let alice_account_id =
		<subxt_signer::sr25519::Keypair as Signer<CustomConfig>>::account_id(&alice);

	let current_nonce = get_account_nonce(para_client, &alice_account_id).await?;
	let set_storage_call = create_set_storage_call(items);

	submit_extrinsic(para_client, &set_storage_call, &alice, current_nonce).await?;

	Ok(())
}
