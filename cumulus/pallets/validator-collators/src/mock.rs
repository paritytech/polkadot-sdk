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

use super::*;
use crate as pallet_validator_collators;
use cumulus_pallet_parachain_system::OnSystemEvent;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types, traits::ConstU32, PalletId,
};
use frame_system::EnsureSignedBy;
use pallet_collator_selection::IdentityCollator;
use sp_runtime::{testing::UintAuthorityId, traits::OpaqueKeys, BuildStorage, RuntimeAppPublic};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Session: pallet_session,
		Balances: pallet_balances,
		CollatorSelection: pallet_collator_selection,
		ValidatorCollators: pallet_validator_collators,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 5;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

sp_runtime::impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub aura: UintAuthorityId,
	}
}

impl From<UintAuthorityId> for MockSessionKeys {
	fn from(aura: UintAuthorityId) -> Self {
		Self { aura }
	}
}

pub struct TestSessionHandler;
impl pallet_session::SessionHandler<u64> for TestSessionHandler {
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[UintAuthorityId::ID];
	fn on_genesis_session<Ks: OpaqueKeys>(_: &[(u64, Ks)]) {}
	fn on_new_session<Ks: OpaqueKeys>(_: bool, _: &[(u64, Ks)], _: &[(u64, Ks)]) {}
	fn on_before_session_ending() {}
	fn on_disabled(_: u32) {}
}

parameter_types! {
	pub const Offset: u64 = 0;
	pub const Period: u64 = 10;
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = IdentityCollator;
	type ShouldEndSession = ValidatorCollators;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager =
		pallet_session::UnionSessionManager<CollatorSelection, ValidatorCollators>;
	type SessionHandler = TestSessionHandler;
	type Keys = MockSessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = ();
	type Currency = Balances;
	type KeyDeposit = ();
}

ord_parameter_types! {
	pub const RootAccount: u64 = 777;
	pub const SetAccount: u64 = 999;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"PotStake");
}

impl pallet_collator_selection::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type UpdateOrigin = EnsureSignedBy<RootAccount, u64>;
	type PotId = PotId;
	type MaxCandidates = ConstU32<20>;
	type MinEligibleCollators = ConstU32<1>;
	type MaxInvulnerables = ConstU32<20>;
	type KickThreshold = Period;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = IdentityCollator;
	type ValidatorRegistration = Session;
	type WeightInfo = ();
}

impl Config for Test {
	type SetOrigin = EnsureSignedBy<SetAccount, u64>;
	type UpdateOrigin = EnsureSignedBy<RootAccount, u64>;
	type ValidatorRegistration = Session;
	type MaxValidators = ConstU32<50>;
	type PeriodicSession = pallet_session::PeriodicSessions<Period, Offset>;
	type WeightInfo = ();
}

/// Standalone collators, funded and with session keys at genesis.
pub const COLLATORS: [u64; 5] = [1, 2, 3, 4, 5];
/// Validators, funded but without session keys at genesis.
pub const VALIDATORS: [u64; 6] = [10, 11, 12, 13, 14, 15];

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_tracing::try_init_simple();
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let balances = COLLATORS.iter().chain(VALIDATORS.iter()).map(|&a| (a, 100)).collect();
	let keys = COLLATORS
		.iter()
		.map(|&a| (a, a, MockSessionKeys { aura: UintAuthorityId(a) }))
		.collect();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	pallet_collator_selection::GenesisConfig::<Test> {
		desired_candidates: 2,
		candidacy_bond: 10,
		invulnerables: vec![1, 2],
	}
	.assimilate_storage(&mut t)
	.unwrap();
	pallet_session::GenesisConfig::<Test> { keys, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	t.into()
}

parameter_types! {
	/// The relay chain epoch randomness in the state proof of every block, `None` for no proof.
	pub static RelayEpochRandomness: Option<[u8; 32]> = Some([7; 32]);
}

/// Produce blocks up to `n`, each with a relay chain state proof carrying
/// [`RelayEpochRandomness`].
pub fn initialize_to_block(n: u64) {
	for i in System::block_number() + 1..=n {
		System::set_block_number(i);
		<AllPalletsWithSystem as frame_support::traits::OnInitialize<u64>>::on_initialize(i);
		if let Some(randomness) = RelayEpochRandomness::get() {
			<ValidatorCollators as OnSystemEvent>::on_relay_state_proof(
				&crate::relay_proof::with_epoch_randomness(randomness),
			);
		}
	}
}
