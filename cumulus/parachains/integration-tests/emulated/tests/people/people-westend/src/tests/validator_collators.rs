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

use crate::imports::*;
use codec::Encode;
use emulated_integration_tests_common::{collators, xcm_emulator::pallet_aura};
use pallet_staking_async::OnEraStart;
use parachains_common::{AccountId, AuraId};
use sp_externalities::ExternalitiesExt;
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use westend_system_emulated_network::asset_hub_westend_emulated_chain::asset_hub_westend_runtime;

fn use_memory_keystore() {
	sp_externalities::with_externalities(|mut ext| {
		let _ = ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
	});
}

fn register_keys_on_asset_hub(who: &AccountId) -> AuraId {
	type Runtime = <AssetHubWestend as Chain>::Runtime;
	use_memory_keystore();
	let keys = asset_hub_westend_runtime::SessionKeys::generate(&who.encode(), None);
	let aura = keys.keys.aura.clone();
	assert_ok!(pallet_session::Pallet::<Runtime>::set_keys(
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(who.clone()),
		keys.keys,
		keys.proof.encode(),
	));
	aura
}

fn register_keys_on_people(who: &AccountId) -> AuraId {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	use_memory_keystore();
	let keys = people_westend_runtime::SessionKeys::generate(&who.encode(), None);
	let aura = keys.keys.aura.clone();
	assert_ok!(pallet_session::Pallet::<Runtime>::set_keys(
		<PeopleWestend as Chain>::RuntimeOrigin::signed(who.clone()),
		keys.keys,
		keys.proof.encode(),
	));
	aura
}

fn invulnerables() -> Vec<(AccountId, AuraId)> {
	let mut invulnerables = collators::invulnerables();
	invulnerables.sort_by(|(a, _), (b, _)| a.cmp(b));
	invulnerables
}

#[test]
fn era_start_on_asset_hub_makes_validators_with_keys_collators_on_both_chains() {
	let alice = Sr25519Keyring::Alice.to_account_id();
	let bob = Sr25519Keyring::Bob.to_account_id();
	let charlie = Sr25519Keyring::Charlie.to_account_id();
	let validators = vec![alice.clone(), bob.clone(), charlie];
	// The validators follow the invulnerables in account order.
	let mut with_keys = vec![alice.clone(), bob.clone()];
	with_keys.sort();
	let expected = invulnerables()
		.into_iter()
		.map(|(who, _)| who)
		.chain(with_keys.clone())
		.collect::<Vec<_>>();
	let expected_authorities = |alice_aura: AuraId, bob_aura: AuraId| {
		let aura_of =
			|who: &AccountId| if *who == alice { alice_aura.clone() } else { bob_aura.clone() };
		invulnerables()
			.into_iter()
			.map(|(_, aura)| aura)
			.chain(with_keys.iter().map(aura_of))
			.collect::<Vec<_>>()
	};

	// GIVEN Alice and Bob registered collator keys on both chains and Charlie did not
	let asset_hub_authorities = AssetHubWestend::execute_with(|| {
		expected_authorities(register_keys_on_asset_hub(&alice), register_keys_on_asset_hub(&bob))
	});
	let people_authorities = PeopleWestend::execute_with(|| {
		expected_authorities(register_keys_on_people(&alice), register_keys_on_people(&bob))
	});
	let people_session_before = PeopleWestend::execute_with(|| {
		pallet_session::Pallet::<<PeopleWestend as Chain>::Runtime>::current_index()
	});

	// WHEN Asset Hub starts era 1 with Alice, Bob and Charlie
	let asset_hub_session_before = AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		asset_hub_westend_runtime::staking::AnnounceValidatorSet::on_era_start(1, &validators);
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::ValidatorCollators(
					pallet_validator_collators::Event::ValidatorSetReceived { era: 1, count: 3 }
				) => {},
				RuntimeEvent::ValidatorSetAnnouncer(
					pallet_validator_set_announcer::Event::AnnouncementSent { era: 1, .. }
				) => {},
			]
		);
		assert!(!<AssetHubWestend as Chain>::events().iter().any(|event| matches!(
			event,
			RuntimeEvent::ValidatorSetAnnouncer(
				pallet_validator_set_announcer::Event::AnnouncementFailed { .. }
			)
		)));
		pallet_session::Pallet::<<AssetHubWestend as Chain>::Runtime>::current_index()
	});
	PeopleWestend::execute_with(|| {
		type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			PeopleWestend,
			vec![
				RuntimeEvent::ValidatorCollators(
					pallet_validator_collators::Event::ValidatorSetReceived { era: 1, count: 3 }
				) => {},
			]
		);
	});

	// THEN after two forced rotations both chains collate with the invulnerables, Alice and Bob
	// Every `execute_with` runs one block, and the rotations run in the two blocks after the one
	// that received the set.
	AssetHubWestend::execute_with(|| {});
	AssetHubWestend::execute_with(|| {
		type Session = pallet_session::Pallet<<AssetHubWestend as Chain>::Runtime>;
		assert_eq!(Session::current_index(), asset_hub_session_before + 2);
		assert_eq!(Session::validators(), expected);
		assert_eq!(
			pallet_aura::Authorities::<<AssetHubWestend as Chain>::Runtime>::get().into_inner(),
			asset_hub_authorities
		);
	});
	PeopleWestend::execute_with(|| {});
	PeopleWestend::execute_with(|| {
		type Session = pallet_session::Pallet<<PeopleWestend as Chain>::Runtime>;
		assert_eq!(Session::current_index(), people_session_before + 2);
		assert_eq!(Session::validators(), expected);
		assert_eq!(
			pallet_aura::Authorities::<<PeopleWestend as Chain>::Runtime>::get().into_inner(),
			people_authorities
		);
	});
}

#[test]
fn cap_on_people_draws_from_the_set_at_the_first_forced_rotation() {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	type RuntimeOrigin = <PeopleWestend as Chain>::RuntimeOrigin;
	type ValidatorCollators = pallet_validator_collators::Pallet<Runtime>;
	type Session = pallet_session::Pallet<Runtime>;
	let alice = Sr25519Keyring::Alice.to_account_id();
	let bob = Sr25519Keyring::Bob.to_account_id();

	// GIVEN Alice and Bob registered collator keys and a cap of 1 set by root
	PeopleWestend::execute_with(|| {
		register_keys_on_people(&alice);
		register_keys_on_people(&bob);
		assert_ok!(ValidatorCollators::set_max_collators(RuntimeOrigin::root(), Some(1)));
	});

	// WHEN a set with both arrives
	let session_before = PeopleWestend::execute_with(|| {
		assert_ok!(ValidatorCollators::set_validators(
			RuntimeOrigin::root(),
			1,
			[alice.clone(), bob.clone()]
				.into_iter()
				.collect::<std::collections::BTreeSet<_>>()
				.try_into()
				.unwrap(),
		));
		Session::current_index()
	});

	// THEN the first forced rotation draws one of them with the randomness the pallet cached as
	// `OnSystemEvent`, which is a zero value in the emulator's relay state proof
	let drawn = PeopleWestend::execute_with(|| {
		assert_eq!(pallet_validator_collators::EpochRandomness::<Runtime>::get(), Some([0; 32]));
		let drawn = pallet_validator_collators::Collators::<Runtime>::get()
			.map(|drawn| drawn.validators.into_inner())
			.unwrap_or_default();
		assert_eq!(drawn.len(), 1);
		assert!(drawn[0] == alice || drawn[0] == bob);
		type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			PeopleWestend,
			vec![
				RuntimeEvent::ValidatorCollators(
					pallet_validator_collators::Event::CollatorsDrawn { era: 1, validators }
				) => { validators: *validators == drawn, },
			]
		);
		drawn
	});

	// AND after the second forced rotation only the drawn validator collates next to the
	// invulnerables
	PeopleWestend::execute_with(|| {
		let expected =
			invulnerables().into_iter().map(|(who, _)| who).chain(drawn).collect::<Vec<_>>();
		assert_eq!(Session::current_index(), session_before + 2);
		assert_eq!(Session::validators(), expected);
	});
}

#[test]
fn asset_hub_caches_the_relay_epoch_randomness_from_the_next_block() {
	type EpochRandomness =
		pallet_validator_collators::EpochRandomness<<AssetHubWestend as Chain>::Runtime>;

	// GIVEN Asset Hub has no cached relay chain epoch randomness
	AssetHubWestend::execute_with(|| EpochRandomness::kill());

	// WHEN the next block processes its relay chain state proof
	// THEN the pallet, wired as `OnSystemEvent`, caches the randomness, which is a zero value in
	// the emulator's relay state proof
	AssetHubWestend::execute_with(|| assert_eq!(EpochRandomness::get(), Some([0; 32])));
}
