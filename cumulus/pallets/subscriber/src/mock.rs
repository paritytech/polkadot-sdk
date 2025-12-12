// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use super::*;
use codec::Encode;
use cumulus_pallet_parachain_system::RelayChainStateProof;
use cumulus_primitives_core::ParaId;
use frame_support::{derive_impl, parameter_types};
use sp_runtime::{BuildStorage, StateVersion};
use sp_state_machine::{Backend, TrieBackendBuilder};
use sp_trie::{PrefixedMemoryDB, StorageProof};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Subscriber: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

// Test handler that records calls
parameter_types! {
	pub static ReceivedData: Vec<(ParaId, Vec<u8>, Vec<u8>)> = vec![];
	pub static TestSubscriptions: Vec<(ParaId, Vec<Vec<u8>>)> = vec![];
}

pub struct TestHandler;
impl SubscriptionHandler for TestHandler {
	fn subscriptions() -> Vec<(ParaId, Vec<Vec<u8>>)> {
		TestSubscriptions::get()
	}

	fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>) {
		ReceivedData::mutate(|d| d.push((publisher, key, value)));
	}
}

parameter_types! {
	pub const MaxPublishers: u32 = 100;
}

impl crate::Config for Test {
	type SubscriptionHandler = TestHandler;
	type WeightInfo = ();
	type MaxPublishers = MaxPublishers;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	t.into()
}

/// Minimal relay chain state proof builder for subscriber tests
pub fn build_sproof_with_child_data(
	publisher_para_id: ParaId,
	child_data: Vec<(Vec<u8>, Vec<u8>)>,
) -> RelayChainStateProof {
	use sp_runtime::traits::HashingFor;

	let (db, root) = PrefixedMemoryDB::<HashingFor<polkadot_primitives::Block>>::default_with_root();
	let state_version = StateVersion::default();
	let mut backend = TrieBackendBuilder::new(db, root).build();

	// Derive child info same way as pallet
	let child_info = sp_core::storage::ChildInfo::new_default(&(b"pubsub", publisher_para_id).encode());

	// Insert child trie data
	let child_kv: Vec<_> = child_data.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect();
	backend.insert(vec![(Some(child_info.clone()), child_kv)], state_version);

	// Get child trie root and insert it in main trie
	let child_root = backend.child_storage_root(&child_info, core::iter::empty(), state_version).0;
	let prefixed_key = child_info.prefixed_storage_key();
	backend.insert(
		vec![(None, vec![(prefixed_key.to_vec(), Some(child_root.encode()))])],
		state_version,
	);

	let root = *backend.root();

	// Prove child trie keys
	let child_keys: Vec<_> = child_data.iter().map(|(k, _)| k.clone()).collect();
	let child_proof = sp_state_machine::prove_child_read_on_trie_backend(&backend, &child_info, child_keys)
		.expect("prove child read");

	// Prove child root in main trie
	let main_proof = sp_state_machine::prove_read_on_trie_backend(&backend, vec![prefixed_key.to_vec()])
		.expect("prove read");

	// Merge proofs
	let proof = StorageProof::merge(vec![main_proof, child_proof]);

	RelayChainStateProof::new(ParaId::from(100), root, proof).expect("valid proof")
}
