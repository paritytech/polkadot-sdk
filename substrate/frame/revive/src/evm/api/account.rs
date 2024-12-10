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
//! Utilities for working with Ethereum accounts.
use crate::{
	evm::{TransactionSigned, TransactionUnsigned},
	H160,
};
use sp_runtime::AccountId32;

/// A simple account that can sign transactions
pub struct Account(subxt_signer::eth::Keypair);

impl Default for Account {
	fn default() -> Self {
		Self(subxt_signer::eth::dev::alith())
	}
}

impl From<subxt_signer::eth::Keypair> for Account {
	fn from(kp: subxt_signer::eth::Keypair) -> Self {
		Self(kp)
	}
}

impl Account {
	/// Create a new account from a secret
	pub fn from_secret_key(secret_key: [u8; 32]) -> Self {
		subxt_signer::eth::Keypair::from_secret_key(secret_key).unwrap().into()
	}

	/// Get the [`H160`] address of the account.
	pub fn address(&self) -> H160 {
		H160::from_slice(&self.0.public_key().to_account_id().as_ref())
	}

	/// Get the substrate [`AccountId32`] of the account.
	pub fn substrate_account(&self) -> AccountId32 {
		let mut account_id = AccountId32::new([0xEE; 32]);
		<AccountId32 as AsMut<[u8; 32]>>::as_mut(&mut account_id)[..20]
			.copy_from_slice(self.address().as_ref());
		account_id
	}

	/// Sign a transaction.
	pub fn sign_transaction(&self, tx: TransactionUnsigned) -> TransactionSigned {
		let payload = tx.unsigned_payload();
		let signature = self.0.sign(&payload).0;
		tx.with_signature(signature)
	}
}

#[test]
fn from_secret_key_works() {
	let account = Account::from_secret_key(hex_literal::hex!(
		"a872f6cbd25a0e04a08b1e21098017a9e6194d101d75e13111f71410c59cd57f"
	));

	assert_eq!(
		account.address(),
		H160::from(hex_literal::hex!("75e480db528101a381ce68544611c169ad7eb342"))
	)
}
