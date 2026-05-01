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

use super::*;
use crate::testing::{allow_unsafe, deny_unsafe};
use assert_matches::assert_matches;
use sp_core::{offchain::storage::InMemOffchainStorage, Bytes};

#[test]
fn local_storage_should_work() {
	let storage = InMemOffchainStorage::default();
	let offchain = Offchain::new(storage);
	let key = Bytes(b"offchain_storage".to_vec());
	let value = Bytes(b"offchain_value".to_vec());

	let ext = allow_unsafe();

	assert_matches!(
		offchain.set_local_storage(&ext, StorageKind::PERSISTENT, key.clone(), value.clone()),
		Ok(())
	);
	assert_matches!(
		offchain.get_local_storage(&ext, StorageKind::PERSISTENT, key.clone()),
		Ok(Some(ref v)) if *v == value
	);
	assert_matches!(
		offchain.clear_local_storage(&ext, StorageKind::PERSISTENT, key.clone()),
		Ok(())
	);
	assert_matches!(offchain.get_local_storage(&ext, StorageKind::PERSISTENT, key), Ok(None));
}

#[test]
fn offchain_calls_considered_unsafe() {
	let storage = InMemOffchainStorage::default();
	let offchain = Offchain::new(storage);
	let key = Bytes(b"offchain_storage".to_vec());
	let value = Bytes(b"offchain_value".to_vec());

	let ext = deny_unsafe();

	assert_matches!(
		offchain.set_local_storage(&ext, StorageKind::PERSISTENT, key.clone(), value.clone()),
		Err(Error::UnsafeRpcCalled(e)) => {
			assert_eq!(e.to_string(), "RPC call is unsafe to be called externally")
		}
	);
	assert_matches!(
		offchain.clear_local_storage(&ext, StorageKind::PERSISTENT, key.clone()),
		Err(Error::UnsafeRpcCalled(e)) => {
			assert_eq!(e.to_string(), "RPC call is unsafe to be called externally")
		}
	);
	assert_matches!(
		offchain.get_local_storage(&ext, StorageKind::PERSISTENT, key),
		Err(Error::UnsafeRpcCalled(e)) => {
			assert_eq!(e.to_string(), "RPC call is unsafe to be called externally")
		}
	);
}
