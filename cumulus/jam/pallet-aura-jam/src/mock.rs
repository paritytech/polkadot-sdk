// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

use crate as pallet_aura_jam;
use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use sp_core::{sr25519, H256};
use sp_runtime::{traits::ConvertInto, BuildStorage, Digest};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AuraId = sp_consensus_aura::sr25519::AuthorityId;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
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

pub const PERIOD: u32 = 10;

impl pallet_aura_jam::Config for Test {
	type AuthorityId = AuraId;
	type MaxAuthorities = ConstU32<16>;
	type SlotDuration = frame_support::traits::ConstU64<6000>;
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
	t.into()
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

/// Execute block `number` anchored at `anchor_slot`.
pub fn run_block(number: u64, anchor_slot: u32) {
	use frame_support::traits::{OnFinalize, OnInitialize};
	System::reset_events();
	System::initialize(&number, &H256::repeat_byte(number as u8), &jam_digest(anchor_slot));
	// Timestamp is left out: it insists on an inherent these blocks do not need.
	<(System, Session, AuraJam)>::on_initialize(number);
	<(System, Session, AuraJam)>::on_finalize(number);
}
