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
use emulated_integration_tests_common::collators;
use pallet_staking_async::OnEraStart;
use parachains_common::AccountId;
use sp_externalities::ExternalitiesExt;
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use westend_system_emulated_network::asset_hub_westend_emulated_chain::asset_hub_westend_runtime;

fn use_memory_keystore() {
	sp_externalities::with_externalities(|mut ext| {
		let _ = ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
	});
}

fn register_keys_on_asset_hub(who: &AccountId) {
	type Runtime = <AssetHubWestend as Chain>::Runtime;
	use_memory_keystore();
	let keys = asset_hub_westend_runtime::SessionKeys::generate(&who.encode(), None);
	assert_ok!(pallet_session::Pallet::<Runtime>::set_keys(
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(who.clone()),
		keys.keys,
		keys.proof.encode(),
	));
}

fn register_keys_on_people(who: &AccountId) {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	use_memory_keystore();
	let keys = people_westend_runtime::SessionKeys::generate(&who.encode(), None);
	assert_ok!(pallet_session::Pallet::<Runtime>::set_keys(
		<PeopleWestend as Chain>::RuntimeOrigin::signed(who.clone()),
		keys.keys,
		keys.proof.encode(),
	));
}

fn invulnerables() -> Vec<AccountId> {
	let mut invulnerables =
		collators::invulnerables().into_iter().map(|(who, _)| who).collect::<Vec<_>>();
	invulnerables.sort();
	invulnerables
}

#[test]
fn era_start_on_asset_hub_makes_validators_with_keys_collators_on_both_chains() {
	let alice = Sr25519Keyring::Alice.to_account_id();
	let bob = Sr25519Keyring::Bob.to_account_id();
	let charlie = Sr25519Keyring::Charlie.to_account_id();
	let validators = vec![alice.clone(), bob.clone(), charlie];
	let expected = invulnerables()
		.into_iter()
		.chain([alice.clone(), bob.clone()])
		.collect::<Vec<_>>();

	// GIVEN Alice and Bob registered collator keys on both chains and Charlie did not
	AssetHubWestend::execute_with(|| {
		register_keys_on_asset_hub(&alice);
		register_keys_on_asset_hub(&bob);
	});
	PeopleWestend::execute_with(|| {
		register_keys_on_people(&alice);
		register_keys_on_people(&bob);
	});
	let people_session_before = PeopleWestend::execute_with(|| {
		pallet_session::Pallet::<<PeopleWestend as Chain>::Runtime>::current_index()
	});

	// WHEN Asset Hub starts era 1 with Alice, Bob and Charlie
	let asset_hub_session_before = AssetHubWestend::execute_with(|| {
		asset_hub_westend_runtime::staking::AnnounceValidatorSet::on_era_start(1, &validators);
		pallet_session::Pallet::<<AssetHubWestend as Chain>::Runtime>::current_index()
	});
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::ValidatorSetAnnouncer(
					pallet_validator_set_announcer::Event::AnnouncementSent { era: 1, .. }
				) => {},
			]
		);
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
	AssetHubWestend::execute_with(|| {
		type Session = pallet_session::Pallet<<AssetHubWestend as Chain>::Runtime>;
		assert_eq!(Session::current_index(), asset_hub_session_before + 2);
		assert_eq!(Session::validators(), expected);
	});
	// Every `execute_with` runs one block. People received the set in the block above, so it
	// needs one more block than Asset Hub to reach the second rotation.
	PeopleWestend::execute_with(|| {});
	PeopleWestend::execute_with(|| {
		type Session = pallet_session::Pallet<<PeopleWestend as Chain>::Runtime>;
		assert_eq!(Session::current_index(), people_session_before + 2);
		assert_eq!(Session::validators(), expected);
	});
}
