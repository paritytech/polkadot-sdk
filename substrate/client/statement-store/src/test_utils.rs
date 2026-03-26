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
use sp_statement_store::{statement_allowance_key, StatementAllowance};

/// Generate a deterministic keypair for a given client index
pub fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementClient//{idx}"), None)
		.expect("Derivation path is always valid; qed")
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
