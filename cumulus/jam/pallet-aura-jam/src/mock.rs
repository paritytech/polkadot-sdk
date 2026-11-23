// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

use crate as pallet_aura_jam;
use cumulus_jam_state_reader::{JamStateExt, JamStateReader, StateKey};
use cumulus_primitives_core::ParaId;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, OnFinalize, OnInitialize},
};
use sp_consensus_aura::digests::CompatibleDigestItem;
use sp_core::{sr25519, H256};
use sp_runtime::{traits::ConvertInto, BuildStorage, Digest};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AuraId = sp_consensus_aura::sr25519::AuthorityId;

// `JamSystem` comes before `AuraJam`, as in a JAM runtime: its `on_initialize` runs the consensus
// hook before `AuraJam` stores the block's slot.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		JamSystem: cumulus_pallet_jam_system,
		Balances: pallet_balances,
		Timestamp: pallet_timestamp,
		Session: pallet_session,
		AuraJam: pallet_aura_jam,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = AuraJam;
	type MinimumPeriod = frame_support::traits::ConstU64<0>;
	type WeightInfo = ();
}

sp_runtime::impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub aura: AuraJam,
	}
}

parameter_types! {
	pub static NextValidators: Vec<u64> = vec![1, 2];
	pub const SelfParaId: ParaId = ParaId::new(200);
}

pub struct TestSessionManager;
impl pallet_session::SessionManager<u64> for TestSessionManager {
	fn new_session(index: u32) -> Option<Vec<u64>> {
		Some(if index == 0 { vec![1, 2] } else { NextValidators::get() })
	}
	fn end_session(_: u32) {}
	fn start_session(_: u32) {}
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = u64;
	type ValidatorIdOf = ConvertInto;
	type ShouldEndSession = pallet_aura_jam::JamSlotSessions<Test>;
	type NextSessionRotation = pallet_aura_jam::JamSlotSessions<Test>;
	type SessionManager = TestSessionManager;
	type SessionHandler = <MockSessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = MockSessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = ();
	type Currency = Balances;
	type KeyDeposit = ();
}

/// Blocks allowed per anchor slot by [`Hook`].
pub const VELOCITY: u32 = 1;
/// The unincluded segment capacity of [`Hook`]. No block is ever included in the mock (it serves
/// no `ParaInfo`), so it exceeds the number of blocks any test runs before it checks the capacity.
pub const CAPACITY: u32 = 16;

/// The consensus hook of the mock runtime.
pub type Hook = pallet_aura_jam::JamFixedVelocityConsensusHook<Test, VELOCITY, CAPACITY>;

impl cumulus_pallet_jam_system::Config for Test {
	type SelfParaId = SelfParaId;
	type ConsensusHook = Hook;
	type OnCodeApplied = ();
}

pub const PERIOD: u32 = 10;
pub const SLOT_DURATION: u64 = 6000;

impl pallet_aura_jam::Config for Test {
	type AuthorityId = AuraId;
	type MaxAuthorities = ConstU32<16>;
	type SlotDuration = frame_support::traits::ConstU64<SLOT_DURATION>;
	type RotationPeriod = ConstU32<PERIOD>;
}

pub fn aura_pair(i: u64) -> sr25519::Pair {
	<sr25519::Pair as sp_core::Pair>::from_seed(&[i as u8; 32])
}

pub fn aura_key(i: u64) -> AuraId {
	sp_core::Pair::public(&aura_pair(i)).into()
}

pub fn set(ids: &[u64]) -> Vec<AuraId> {
	ids.iter().map(|i| aura_key(*i)).collect()
}

/// The JAM state the mock serves: none, as before the para's registration with the service.
struct NoJamState;

impl JamStateReader for NoJamState {
	fn read(&self, _: &StateKey) -> Option<Vec<u8>> {
		None
	}

	fn proof_size(&self) -> usize {
		0
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: (1..=4).map(|i| (i, 100)).collect(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	pallet_session::GenesisConfig::<Test> {
		keys: (1..=4).map(|i| (i, i, MockSessionKeys { aura: aura_key(i) })).collect(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.register_extension(JamStateExt(Box::new(NoJamState)));
	ext
}

/// The Aura slot of a block built on `anchor_slot`.
pub fn aura_slot(anchor_slot: u32) -> u64 {
	cumulus_jam_aura_primitives::aura_slot_for_anchor(anchor_slot, SLOT_DURATION)
}

/// The digest of a block anchored at `anchor_slot`.
pub fn jam_digest(anchor_slot: u32) -> Digest {
	Digest {
		logs: vec![cumulus_primitives_core::CumulusDigestItem::JamParent {
			anchor: [1u8; 32].into(),
			anchor_slot,
			lookup_anchor: [2u8; 32].into(),
			lookup_anchor_slot: anchor_slot,
		}
		.to_digest_item()],
	}
}

/// The digest of a block anchored at `anchor_slot` with an Aura pre-digest for `aura_slot`.
pub fn block_digest(anchor_slot: u32, aura_slot: u64) -> Digest {
	let mut digest = jam_digest(anchor_slot);
	digest.push(CompatibleDigestItem::<()>::aura_pre_digest(aura_slot.into()));
	digest
}

/// Execute block `number` anchored at `anchor_slot`, in the anchor's Aura slot, the way
/// `Executive` does: every pallet's `on_initialize` in runtime order, the timestamp inherent with
/// the anchor's timestamp, every pallet's `on_finalize`.
pub fn run_block(number: u64, anchor_slot: u32) {
	System::reset_events();
	System::initialize(
		&number,
		&H256::repeat_byte(number as u8),
		&block_digest(anchor_slot, aura_slot(anchor_slot)),
	);
	AllPalletsWithSystem::on_initialize(number);
	Timestamp::set_timestamp(cumulus_jam_aura_primitives::timestamp_for_anchor(anchor_slot));
	AllPalletsWithSystem::on_finalize(number);
}
