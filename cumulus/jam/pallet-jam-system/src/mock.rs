// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! A runtime of `frame_system` and this pallet, with the JAM state a test configures served to
//! `jam_state_read`.

use crate as jam_system;
use alloc::collections::BTreeMap;
use core::{cell::RefCell, num::NonZeroU32};
use cumulus_jam_state_reader::{JamStateExt, JamStateReader};
use cumulus_pallet_parachain_system::block_weight;
use cumulus_primitives_core::{CumulusDigestItem, JamParent, ParaId};
use frame_support::{derive_impl, parameter_types, traits::Hooks, weights::Weight};
use parachain_service_core::StateKey;
use sp_core::H256;
use sp_runtime::{BuildStorage, Digest};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		JamSystem: jam_system,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type OnSetCode = jam_system::JamSetCode<Test>;
}

/// The para id of the mock runtime.
pub const PARA_ID: u32 = 200;

parameter_types! {
	pub const SelfParaId: ParaId = ParaId::new(PARA_ID);
	/// The capacity [`TestConsensusHook`] returns.
	pub static SegmentCapacity: u32 = 3;
	/// The `JamParent` digests [`TestConsensusHook`] was called with, in order.
	pub static HookCalls: Vec<JamParent> = Vec::new();
	/// How often [`TestOnCodeApplied`] ran.
	pub static CodeApplied: u32 = 0;
}

/// Records the anchors it is called with and returns [`SegmentCapacity`].
pub struct TestConsensusHook;

impl jam_system::ConsensusHook for TestConsensusHook {
	fn on_block(parent: &JamParent) -> (Weight, NonZeroU32) {
		HookCalls::mutate(|calls| calls.push(*parent));
		let capacity =
			NonZeroU32::new(SegmentCapacity::get()).expect("tests set a non-zero capacity; qed");
		(Weight::zero(), capacity)
	}
}

/// Counts the applied code upgrades in [`CodeApplied`].
pub struct TestOnCodeApplied;

impl jam_system::OnCodeApplied for TestOnCodeApplied {
	fn on_code_applied() {
		CodeApplied::mutate(|count| *count += 1);
	}
}

impl jam_system::Config for Test {
	type SelfParaId = SelfParaId;
	type ConsensusHook = TestConsensusHook;
	type OnCodeApplied = TestOnCodeApplied;
}

/// The dynamic block weight logic's view of the pallet, as a JAM runtime implements it.
impl block_weight::Config for Test {
	type WeightInfo = ();

	fn mode() -> Option<block_weight::BlockWeightMode<Self>> {
		JamSystem::block_weight_mode()
	}

	fn set_mode(mode: Option<block_weight::BlockWeightMode<Self>>) {
		JamSystem::set_block_weight_mode(mode)
	}

	fn previous_core_count() -> Option<u16> {
		JamSystem::previous_core_count()
	}
}

thread_local! {
	/// The JAM state the current test serves to `jam_state_read`, keyed by the 31-byte state key.
	static MOCK_JAM_READS: RefCell<Option<BTreeMap<Vec<u8>, Vec<u8>>>> =
		const { RefCell::new(None) };
}

/// Configure the JAM state served to `jam_state_read` in the current test.
pub fn set_mock_jam_reads(map: BTreeMap<Vec<u8>, Vec<u8>>) {
	MOCK_JAM_READS.with(|c| *c.borrow_mut() = Some(map));
}

/// Serves `jam_state_read` from [`MOCK_JAM_READS`], the way the riscv runtime serves it from the
/// carried JAM state proof.
struct MockJamReads;

impl JamStateReader for MockJamReads {
	fn read(&self, key: &StateKey) -> Option<Vec<u8>> {
		MOCK_JAM_READS.with(|c| {
			c.borrow()
				.as_ref()
				.expect("jam reads configured for the current test")
				.get(&key[..])
				.cloned()
		})
	}

	fn proof_size(&self) -> usize {
		0
	}
}

/// Externalities with an empty genesis and [`MockJamReads`] registered.
///
/// Clears the per-test state first: with `--test-threads=1` every test runs on the same thread.
pub fn new_test_ext() -> sp_io::TestExternalities {
	MOCK_JAM_READS.with(|c| *c.borrow_mut() = None);
	SegmentCapacity::set(3);
	HookCalls::take();
	CodeApplied::take();

	let mut ext: sp_io::TestExternalities = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("the mock genesis builds; qed")
		.into();
	ext.register_extension(JamStateExt(Box::new(MockJamReads)));
	ext
}

/// The state key of the mock para's `ParaInfo` in the parachain service.
pub fn para_info_state_key() -> StateKey {
	parachain_service_core::service_value_state_key(
		parachain_service_core::PARACHAIN_SERVICE_ID,
		&parachain_service_core::para_info_key(parachain_service_core::types::ParaId::from(
			PARA_ID,
		)),
	)
}

/// The anchors every mock block names in its `JamParent` digest.
pub fn jam_parent() -> JamParent {
	JamParent {
		anchor: [1u8; 32].into(),
		anchor_slot: 11,
		lookup_anchor: [2u8; 32].into(),
		lookup_anchor_slot: 10,
	}
}

/// The digest of a mock block: [`jam_parent`] as a `JamParent` pre-runtime digest.
pub fn jam_digest() -> Digest {
	let JamParent { anchor, anchor_slot, lookup_anchor, lookup_anchor_slot } = jam_parent();
	Digest {
		logs: vec![CumulusDigestItem::JamParent {
			anchor,
			anchor_slot,
			lookup_anchor,
			lookup_anchor_slot,
		}
		.to_digest_item()],
	}
}

/// Initialize block `number` on `parent` with [`jam_digest`] and run the pallet's
/// `on_initialize`.
pub fn initialize_block(number: u64, parent: H256) {
	System::initialize(&number, &parent, &jam_digest());
	JamSystem::on_initialize(number);
}
