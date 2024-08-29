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

//! State machine fuzzing implementation, behind `fuzzing` feature.

use super::{ext::Ext, *};
use crate::ext::StorageAppend;
use arbitrary::Arbitrary;
#[cfg(test)]
use codec::Encode;
use hash_db::Hasher;
use sp_core::{storage::StateVersion, traits::Externalities};
#[cfg(test)]
use sp_runtime::traits::BlakeTwo256;
use sp_trie::PrefixedMemoryDB;
use std::collections::BTreeMap;

#[derive(Arbitrary, Debug, Clone)]
enum DataLength {
	Zero = 0,
	Small = 1,
	Medium = 3,
	Big = 300, // 2 byte scale encode length
}

#[derive(Arbitrary, Debug, Clone)]
#[repr(u8)]
enum DataValue {
	A = b'a',
	B = b'b',
	C = b'c',
	D = b'd',       // This can be read as a multiple byte compact length.
	EasyBug = 20u8, // value compact len.
}

/// Action to fuzz
#[derive(Arbitrary, Debug, Clone)]
enum FuzzAppendItem {
	Append(DataValue, DataLength),
	Insert(DataValue, DataLength),
	StartTransaction,
	RollbackTransaction,
	CommitTransaction,
	Read,
	Remove,
	// To go over 256 items easily (different compact size then).
	Append50(DataValue, DataLength),
}

/// Arbitrary payload for fuzzing append.
#[derive(Arbitrary, Debug, Clone)]
pub struct FuzzAppendPayload(Vec<FuzzAppendItem>, Option<(DataValue, DataLength)>);

struct SimpleOverlay {
	data: Vec<BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
}

impl Default for SimpleOverlay {
	fn default() -> Self {
		Self { data: vec![BTreeMap::new()] }
	}
}

impl SimpleOverlay {
	fn insert(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) {
		self.data.last_mut().expect("always at least one item").insert(key, value);
	}

	fn append<H>(
		&mut self,
		key: Vec<u8>,
		value: Vec<u8>,
		backend: &mut TrieBackend<PrefixedMemoryDB<H>, H>,
	) where
		H: Hasher,
		H::Out: codec::Decode + codec::Encode + 'static,
	{
		let current_value = self
			.data
			.last_mut()
			.expect("always at least one item")
			.entry(key.clone())
			.or_insert_with(|| {
				Some(backend.storage(&key).expect("Ext not allowed to fail").unwrap_or_default())
			});
		if current_value.is_none() {
			*current_value = Some(vec![]);
		}
		StorageAppend::new(current_value.as_mut().expect("init above")).append(value);
	}

	fn get(&mut self, key: &[u8]) -> Option<&Vec<u8>> {
		self.data
			.last_mut()
			.expect("always at least one item")
			.get(key)
			.and_then(|o| o.as_ref())
	}

	fn commit_transaction(&mut self) {
		if let Some(to_commit) = self.data.pop() {
			let dest = self.data.last_mut().expect("always at least one item");
			for (k, v) in to_commit.into_iter() {
				dest.insert(k, v);
			}
		}
	}

	fn rollback_transaction(&mut self) {
		let _ = self.data.pop();
	}

	fn start_transaction(&mut self) {
		let cloned = self.data.last().expect("always at least one item").clone();
		self.data.push(cloned);
	}
}

struct FuzzAppendState<H: Hasher> {
	key: Vec<u8>,

	// reference simple implementation
	reference: SimpleOverlay,

	// trie backend
	backend: TrieBackend<PrefixedMemoryDB<H>, H>,
	// Standard Overlay
	overlay: OverlayedChanges<H>,

	// block dropping/commiting too many transaction
	transaction_depth: usize,
}

impl<H> FuzzAppendState<H>
where
	H: Hasher,
	H::Out: codec::Decode + codec::Encode + 'static,
{
	fn process_item(&mut self, item: FuzzAppendItem) {
		let mut ext = Ext::new(&mut self.overlay, &mut self.backend, None);
		match item {
			FuzzAppendItem::Append(value, length) => {
				let value = vec![value as u8; length as usize];
				ext.storage_append(self.key.clone(), value.clone());
				self.reference.append(self.key.clone(), value, &mut self.backend);
			},
			FuzzAppendItem::Append50(value, length) => {
				let value = vec![value as u8; length as usize];
				for _ in 0..50 {
					let mut ext = Ext::new(&mut self.overlay, &mut self.backend, None);
					ext.storage_append(self.key.clone(), value.clone());
					self.reference.append(self.key.clone(), value.clone(), &mut self.backend);
				}
			},
			FuzzAppendItem::Insert(value, length) => {
				let value = vec![value as u8; length as usize];
				ext.set_storage(self.key.clone(), value.clone());
				self.reference.insert(self.key.clone(), Some(value));
			},
			FuzzAppendItem::Remove => {
				ext.clear_storage(&self.key);
				self.reference.insert(self.key.clone(), None);
			},
			FuzzAppendItem::Read => {
				let left = ext.storage(self.key.as_slice());
				let right = self.reference.get(self.key.as_slice());
				assert_eq!(left.as_ref(), right);
			},
			FuzzAppendItem::StartTransaction => {
				self.transaction_depth += 1;
				self.reference.start_transaction();
				ext.storage_start_transaction();
			},
			FuzzAppendItem::RollbackTransaction => {
				if self.transaction_depth == 0 {
					return
				}
				self.transaction_depth -= 1;
				self.reference.rollback_transaction();
				ext.storage_rollback_transaction().unwrap();
			},
			FuzzAppendItem::CommitTransaction => {
				if self.transaction_depth == 0 {
					return
				}
				self.transaction_depth -= 1;
				self.reference.commit_transaction();
				ext.storage_commit_transaction().unwrap();
			},
		}
	}

	fn check_final_state(&mut self) {
		let mut ext = Ext::new(&mut self.overlay, &mut self.backend, None);
		let left = ext.storage(self.key.as_slice());
		let right = self.reference.get(self.key.as_slice());
		assert_eq!(left.as_ref(), right);
	}
}

#[test]
fn fuzz_scenarii() {
	assert_eq!(codec::Compact(5u16).encode()[0], DataValue::EasyBug as u8);
	let scenarii = vec![
		(
			vec![
				FuzzAppendItem::Append(DataValue::A, DataLength::Small),
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append50(DataValue::D, DataLength::Small),
				FuzzAppendItem::Read,
				FuzzAppendItem::RollbackTransaction,
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append(DataValue::D, DataLength::Small),
				FuzzAppendItem::Read,
				FuzzAppendItem::RollbackTransaction,
			],
			Some((DataValue::D, DataLength::Small)),
		),
		(
			vec![
				FuzzAppendItem::Append(DataValue::B, DataLength::Small),
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append(DataValue::A, DataLength::Small),
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Remove,
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append(DataValue::A, DataLength::Zero),
				FuzzAppendItem::CommitTransaction,
				FuzzAppendItem::CommitTransaction,
				FuzzAppendItem::Remove,
			],
			Some((DataValue::EasyBug, DataLength::Small)),
		),
		(
			vec![
				FuzzAppendItem::Append(DataValue::A, DataLength::Small),
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append(DataValue::A, DataLength::Medium),
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Remove,
				FuzzAppendItem::CommitTransaction,
				FuzzAppendItem::RollbackTransaction,
			],
			Some((DataValue::B, DataLength::Big)),
		),
		(
			vec![
				FuzzAppendItem::Append(DataValue::A, DataLength::Big),
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append(DataValue::A, DataLength::Medium),
				FuzzAppendItem::Remove,
				FuzzAppendItem::RollbackTransaction,
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::Append(DataValue::A, DataLength::Zero),
			],
			None,
		),
		(
			vec![
				FuzzAppendItem::StartTransaction,
				FuzzAppendItem::RollbackTransaction,
				FuzzAppendItem::RollbackTransaction,
				FuzzAppendItem::Append(DataValue::A, DataLength::Zero),
			],
			None,
		),
		(vec![FuzzAppendItem::StartTransaction], Some((DataValue::EasyBug, DataLength::Zero))),
	];

	for (scenario, init) in scenarii.into_iter() {
		fuzz_append::<BlakeTwo256>(FuzzAppendPayload(scenario, init));
	}
}

/// Test append operation for a given fuzzing payload.
pub fn fuzz_append<H>(payload: FuzzAppendPayload)
where
	H: Hasher,
	H::Out: codec::Decode + codec::Encode + 'static,
{
	let FuzzAppendPayload(to_fuzz, initial) = payload;
	let key = b"k".to_vec();
	let mut reference = SimpleOverlay::default();
	let initial: BTreeMap<_, _> = initial
		.into_iter()
		.map(|(v, l)| (key.clone(), vec![v as u8; l as usize]))
		.collect();
	for (k, v) in initial.iter() {
		reference.data[0].insert(k.clone(), Some(v.clone()));
	}
	reference.start_transaction(); // level 0 is backend, keep it untouched.
	let overlay = OverlayedChanges::default();

	let mut state = FuzzAppendState::<H> {
		key,
		reference,
		overlay,
		backend: (initial, StateVersion::default()).into(),
		transaction_depth: 0,
	};
	for item in to_fuzz {
		state.process_item(item);
	}
	state.check_final_state();
}
