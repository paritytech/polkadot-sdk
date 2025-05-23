#![allow(unused)]
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

use super::{runtime_api::RuntimeApi, storage_api::StorageApi};
use crate::{Bytes, PrestateTraceInfo};
use futures::{stream, StreamExt};
use pallet_revive::evm::PrestateTracerConfig;
use sp_core::H160;
use sp_rpc::tracing::Event;
use std::collections::{BTreeMap, HashMap, HashSet};
use subxt::storage::DynamicAddress;

// An iterator that returns events by extrinsic index.
pub struct EventsByExtrinsicIndex {
	events: Vec<Event>,
}

/// The `:extrinsic_index` hex string, u to identify new extrinsics events.
const EXTRINSIC_INDEX: &'static str = "3a65787472696e7369635f696e646578";

/// System.Account's address prefix
const SYSTEM_ACCOUNT: &'static str =
	"26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9";

/// The hex prefix's len for System.Account's address.
/// The key is encoded as:
/// twox_128("System") ++ twox_128("Account") ++ blak2_128(<address>) ++ address
const SYSTEM_ACCOUNT_PREFIX_LEN: usize = SYSTEM_ACCOUNT.len() as usize + 32;

/// Revive.ContractInfoOf's address prefix
const REVIVE_CONTRACT_INFO_OF: &'static str =
	"735f040a5d490f1107ad9c56f5ca00d2060e99e5378e562537cf3bc983e17b91";

/// The hex prefix's len for Revive.ContractInfoOf's address.
/// The key is encoded as:
/// twox_128("Revive") ++ twox_128("ContractInfoOf") ++  address
const REVIVE_CONTRACT_INFO_PREFIX_LEN: usize = REVIVE_CONTRACT_INFO_OF.len() as usize;

#[test]
fn playground() {
	let key = "26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9d351f2b7018a686527db25eb657c9153f24ff3a9cf04c71dbc94d0b566f7a27b94566caceeeeeeeeeeeeeeeeeeeeeeee";
	let key = hex::decode(&key[SYSTEM_ACCOUNT_PREFIX_LEN..]).unwrap();
	println!("{:?}", hex::encode(key));
}

#[test]
fn test_static_str() {
	assert_eq!(EXTRINSIC_INDEX, hex::encode(":extrinsic_index"));
	assert_eq!(
		hex::decode(SYSTEM_ACCOUNT).unwrap().to_vec(),
		DynamicAddress::new("System", "Account", ()).to_root_bytes()
	);
	assert_eq!(
		hex::decode(REVIVE_CONTRACT_INFO_OF).unwrap().to_vec(),
		DynamicAddress::new("Revive", "ContractInfoOf", ()).to_root_bytes()
	);
}

impl EventsByExtrinsicIndex {
	pub fn new(events: Vec<Event>) -> Self {
		let mut events = EventsByExtrinsicIndex { events };
		// fast forward to the first extrinsic
		events.next();
		events
	}
}

impl Iterator for EventsByExtrinsicIndex {
	type Item = Vec<Event>;

	fn next(&mut self) -> Option<Self::Item> {
		if self.events.is_empty() {
			return None;
		}

		let last = self.events.iter().position(|evt| {
			let map = &evt.data.string_values;
			map.get("method").map_or(false, |e| e == "Put") &&
				map.get("key").map_or(false, |e| e == EXTRINSIC_INDEX)
		});

		let events = if let Some(last) = last {
			let mut events = self.events.drain(..=last).collect::<Vec<_>>();
			events.pop();
			events
		} else {
			std::mem::take(&mut self.events)
		};

		Some(events)
	}
}

pub struct Events {
	events: Vec<Event>,
	storage_api: StorageApi,
	runtime_api: RuntimeApi,
}

impl Events {
	pub fn new(events: Vec<Event>, storage_api: StorageApi, runtime_api: RuntimeApi) -> Self {
		Events { events, runtime_api, storage_api }
	}

	/// Extracts Accounts from the events.
	pub async fn get_accounts(&self, put_only: bool) -> Vec<H160> {
		let raw_addresses = self
			.events
			.iter()
			.filter_map(|evt| {
				let key = evt.data.string_values.get("key")?;
				if put_only {
					if evt.data.string_values.get("method")? != "Put" {
						return None;
					}
				}

				if key.starts_with(SYSTEM_ACCOUNT) {
					let bytes = hex::decode(&key[SYSTEM_ACCOUNT_PREFIX_LEN..]).ok()?;
					let bytes: [u8; 32] = bytes.try_into().ok()?;
					Some(bytes)
				} else {
					None
				}
			})
			.collect::<HashSet<_>>();

		stream::iter(raw_addresses)
			.filter_map(|account_id| {
				let runtime_api = self.runtime_api.clone();
				async move { runtime_api.to_address(account_id.into()).await.ok() }
			})
			.collect::<Vec<_>>()
			.await
	}

	/// Extracts storage changes from the events.
	pub async fn get_contracts_storage(
		&self,
		put_only: bool,
	) -> HashMap<H160, HashMap<Bytes, Option<Bytes>>> {
		let contract_accounts = self
			.events
			.iter()
			.filter_map(|evt| {
				let key = evt.data.string_values.get("key")?;
				if key.starts_with(&REVIVE_CONTRACT_INFO_OF) {
					let addr = hex::decode(&key[REVIVE_CONTRACT_INFO_PREFIX_LEN..]).ok()?;
					let addr = H160::from_slice(&addr);
					Some(addr)
				} else {
					None
				}
			})
			.collect::<HashSet<_>>();

		let contracts_by_trie_id = stream::iter(contract_accounts)
			.map(|addr| {
				let storage_api = self.storage_api.clone();
				async move {
					let trie_id = storage_api.get_contract_trie_id(&addr).await.unwrap_or_default();
					(hex::encode(&trie_id), addr)
				}
			})
			.buffer_unordered(10)
			.collect::<HashMap<_, _>>()
			.await;

		println!("contracts_by_trie_id: {:#?}", contracts_by_trie_id);
		self.events
			.iter()
			.filter_map(|evt| {
				let child_info = evt.data.string_values.get("child_info")?;

				if put_only && evt.data.string_values.get("method")? != "ChildPut" {
					return None;
				}

				if let Some(addr) = contracts_by_trie_id.get(child_info) {
					println!("child_info: {:#?} evt: {evt:#?}", child_info);
					let key = evt.data.string_values.get("key")?;
					let value = evt.data.string_values.get("value_encoded")?;
					let key: Bytes = hex::decode(key).unwrap().into();
					use codec::Decode;
					let value = <Option<Vec<u8>>>::decode(&mut &hex::decode(value).unwrap()[..])
						.unwrap()
						.map(Bytes);

					Some((*addr, (key.clone(), value)))
				} else {
					None
				}
			})
			.fold(HashMap::new(), |mut addrs, (addr, (key, val))| {
				addrs.entry(addr).or_default().insert(key, val);
				addrs
			})
	}
}
