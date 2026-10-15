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
use emulated_integration_tests_common::{
	collators,
	xcm_emulator::{self, pallet_aura},
};
use pallet_staking_async::OnEraStart;
use parachains_common::{AccountId, AuraId};
use sp_externalities::ExternalitiesExt;
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_staking::ValidatorServiceKind;
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
		type Runtime = <AssetHubWestend as Chain>::Runtime;
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
		// AND both chains record the arrival of the set to time the report before the era end
		assert!(pallet_validator_collators::SetReceivedAt::<Runtime>::get().is_some());
		pallet_session::Pallet::<Runtime>::current_index()
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
		type Runtime = <PeopleWestend as Chain>::Runtime;
		assert!(pallet_validator_collators::SetReceivedAt::<Runtime>::get().is_some());
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

#[test]
fn people_reports_authored_blocks_to_asset_hub() {
	type AssetHubRuntime = <AssetHubWestend as Chain>::Runtime;
	type PeopleRuntime = <PeopleWestend as Chain>::Runtime;
	type PeopleValidatorCollators = pallet_validator_collators::Pallet<PeopleRuntime>;
	let alice = Sr25519Keyring::Alice.to_account_id();
	let bob = Sr25519Keyring::Bob.to_account_id();
	let invulnerable = invulnerables()[0].0.clone();
	let people = PeopleWestend::para_id();

	// GIVEN era 1 is active on Asset Hub with Alice exposed, and People's blocks earn 5 points
	AssetHubWestend::execute_with(|| {
		pallet_staking_async::ActiveEra::<AssetHubRuntime>::put(
			pallet_staking_async::ActiveEraInfo { index: 1, start: Some(0) },
		);
		pallet_staking_async::session_rotation::Eras::<AssetHubRuntime>::upsert_exposure(
			1,
			&alice,
			pallet_staking_async::Exposure { total: 1, own: 1, others: vec![] },
		);
		assert_ok!(pallet_staking_async_points_converter::Pallet::<AssetHubRuntime>::set_points_per_service(
			<AssetHubWestend as Chain>::RuntimeOrigin::root(),
			people,
			ValidatorServiceKind::Collation,
			5,
		));
	});

	// AND People stores the set of Alice and Bob, Alice authored 3 blocks, Bob 1 and an
	// invulnerable 2, and the next block is a report block. People drops the invulnerable, which
	// is not in its set, and Asset Hub drops Bob, who is in People's set but not exposed.
	PeopleWestend::execute_with(|| {
		assert_ok!(PeopleValidatorCollators::set_validators(
			<PeopleWestend as Chain>::RuntimeOrigin::root(),
			1,
			[alice.clone(), bob.clone()]
				.into_iter()
				.collect::<std::collections::BTreeSet<_>>()
				.try_into()
				.unwrap(),
		));
		let note = |who: &AccountId, blocks: u32| {
			(0..blocks).for_each(|_| {
				<PeopleValidatorCollators as pallet_authorship::EventHandler<_, _>>::note_author(
					who.clone(),
				)
			})
		};
		note(&alice, 3);
		note(&bob, 1);
		note(&invulnerable, 2);
		let next = <PeopleWestend as Chain>::System::block_number() + 1;
		assert_ok!(PeopleValidatorCollators::set_report_period(
			<PeopleWestend as Chain>::RuntimeOrigin::root(),
			next,
		));
	});

	// WHEN People reaches the report block
	PeopleWestend::execute_with(|| {
		type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			PeopleWestend,
			vec![
				RuntimeEvent::ValidatorCollators(
					pallet_validator_collators::Event::BlocksReported { reported: 2, .. }
				) => {},
			]
		);
		assert_eq!(pallet_validator_collators::AuthoredBlocks::<PeopleRuntime>::count(), 0);
	});

	// THEN Asset Hub credits Alice's blocks with People's points per service, drops Bob's and
	// credits nobody else
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::StakingPointsConverter(
					pallet_staking_async_points_converter::Event::BlocksCredited { source, entries: 2 }
				) => { source: *source == people, },
				RuntimeEvent::Staking(
					pallet_staking_async::Event::ServicePointsCredited {
						service: ValidatorServiceKind::Collation,
						era: 1,
						credited: 1,
						dropped: 1,
					}
				) => {},
			]
		);
		let points = pallet_staking_async::ErasRewardPoints::<AssetHubRuntime>::get(1);
		assert_eq!(points.individual.into_iter().collect::<Vec<_>>(), vec![(alice.clone(), 15)]);
		assert_eq!(points.total, 15);
		// AND they are recorded as collation points only
		let by_service = |service| {
			let points =
				pallet_staking_async::ErasRewardPointsByService::<AssetHubRuntime>::get(1, service);
			(points.total, points.individual.into_iter().collect::<Vec<_>>())
		};
		assert_eq!(by_service(ValidatorServiceKind::Collation), (15, vec![(alice, 15)]));
		assert_eq!(by_service(ValidatorServiceKind::RelayConsensus), (0, vec![]));
	});
}

#[test]
fn people_report_call_decodes_on_asset_hub() {
	// GIVEN the largest call People sends to report blocks
	type MaxEntries =
		<people_westend_runtime::Runtime as pallet_validator_collators::Config>::MaxEntriesPerReport;
	let max_entries = <MaxEntries as frame_support::traits::Get<u32>>::get();
	let blocks = (0..max_entries)
		.map(|i| {
			let mut account = [0u8; 32];
			account[..4].copy_from_slice(&i.to_le_bytes());
			(AccountId::from(account), i + 1)
		})
		.collect::<Vec<_>>();
	let encoded = people_westend_runtime::ReportBlocksToAssetHub::encoded_call(&blocks);
	// WHEN Asset Hub decodes it
	let call = <asset_hub_westend_runtime::RuntimeCall as codec::Decode>::decode(&mut &encoded[..]);
	// THEN it is `report_authored_blocks` of `pallet-staking-async-points-converter` with the
	// same entries
	match call {
		Ok(asset_hub_westend_runtime::RuntimeCall::StakingPointsConverter(
			pallet_staking_async_points_converter::Call::report_authored_blocks { blocks: decoded },
		)) => assert_eq!(decoded, blocks),
		other => panic!("People's report decodes on Asset Hub as {other:?}"),
	}
}

/// The account Aura assigns People slot `slot` to.
fn author_on_people(slot: u64) -> AccountId {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	let validators = pallet_session::Pallet::<Runtime>::validators();
	validators[(slot % validators.len() as u64) as usize].clone()
}

/// Put the next People block in slot `slot`, through the relay chain block number the emulator
/// derives the slot from.
fn next_people_block_in_slot(slot: u64) {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	let relay_blocks_per_slot = pallet_aura::Pallet::<Runtime>::slot_duration() /
		xcm_emulator::RELAY_CHAIN_SLOT_DURATION_MILLIS;
	<<PeopleWestend as Chain>::Network as xcm_emulator::Network>::set_relay_block_number(
		(slot * relay_blocks_per_slot - 1) as u32,
	);
}

/// The slots counted up to the current People block.
fn counted_slots_on_people() -> u64 {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	pallet_validator_collators::CountedSlots::<Runtime>::get().map_or(0, |counted| counted.count)
}

#[test]
fn idle_validator_is_dropped_on_people_at_the_regular_rotation() {
	type Runtime = <PeopleWestend as Chain>::Runtime;
	type RuntimeOrigin = <PeopleWestend as Chain>::RuntimeOrigin;
	type ValidatorCollators = pallet_validator_collators::Pallet<Runtime>;
	type Session = pallet_session::Pallet<Runtime>;
	type System = <PeopleWestend as Chain>::System;
	let period = people_westend_runtime::PERIOD;
	let alice = Sr25519Keyring::Alice.to_account_id();
	let bob = Sr25519Keyring::Bob.to_account_id();
	let collators_with = |validators: Vec<AccountId>| {
		invulnerables()
			.into_iter()
			.map(|(who, _)| who)
			.chain(validators)
			.collect::<Vec<_>>()
	};
	let invulnerable =
		|who: &AccountId| invulnerables().into_iter().any(|(invulnerable, _)| invulnerable == *who);
	let mut both = vec![alice.clone(), bob.clone()];
	both.sort();
	let rule =
		pallet_validator_collators::DropRule { idle_rounds: 2, drop_slots: 2, min_collators: 0 };

	// GIVEN a rule of two rounds and drops of 2 slots without a floor, and a set of Alice and
	// Bob with keys, both set by root
	PeopleWestend::execute_with(|| {
		register_keys_on_people(&alice);
		register_keys_on_people(&bob);
		assert_ok!(ValidatorCollators::set_drop_rule(RuntimeOrigin::root(), Some(rule)));
		assert_ok!(ValidatorCollators::set_validators(
			RuntimeOrigin::root(),
			1,
			both.iter()
				.cloned()
				.collect::<std::collections::BTreeSet<_>>()
				.try_into()
				.unwrap(),
		));
	});
	PeopleWestend::execute_with(|| {});
	// AND the second forced rotation enacts both, so both are judged from the count of that block
	let (window_end, slots) = PeopleWestend::execute_with(|| {
		assert_eq!(Session::validators(), collators_with(both.clone()));
		let enacted = counted_slots_on_people();
		for v in &both {
			assert_eq!(
				pallet_validator_collators::LastActivity::<Runtime>::get(v),
				Some(pallet_validator_collators::Activity::Active {
					since: enacted,
					authorities: 0
				})
			);
		}
		let authorities = pallet_aura::Pallet::<Runtime>::authorities_len() as u64;
		// The drop end is reached in one gap of the list without Bob, which counts in full.
		assert!(rule.drop_slots < authorities);
		// AND the next blocks are in every slot of an invulnerable, up to the first slot of
		// Alice at least a window after the enactment, so each gap is shorter than the list and
		// counts in full, and the block in that slot is the regular rotation
		let enacted_slot = u64::from(pallet_aura::CurrentSlot::<Runtime>::get());
		let window = rule.idle_rounds as u64 * authorities;
		let rotation_slot =
			(enacted_slot + window..).find(|slot| author_on_people(*slot) == alice).unwrap();
		let slots = (enacted_slot + 1..rotation_slot)
			.filter(|slot| invulnerable(&author_on_people(*slot)))
			.chain([rotation_slot])
			.collect::<Vec<_>>();
		next_people_block_in_slot(slots[0]);
		System::set_block_number(period - slots.len() as u32);
		(enacted + window, slots)
	});
	for slot in &slots[1..] {
		PeopleWestend::execute_with(|| next_people_block_in_slot(*slot));
	}

	// WHEN People runs the regular rotation
	PeopleWestend::execute_with(|| {
		type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;
		assert_eq!(System::block_number(), period);
		assert_eq!(pallet_authorship::Pallet::<Runtime>::author(), Some(alice.clone()));
		assert!(counted_slots_on_people() >= window_end);
		// THEN only Bob, who authored nothing for a window, is dropped
		assert_expected_events!(
			PeopleWestend,
			vec![
				RuntimeEvent::ValidatorCollators(
					pallet_validator_collators::Event::CollatorsDropped { validators, kept_by_floor: 0 }
				) => { validators: *validators == vec![bob.clone()], },
			]
		);
		// AND he keeps his slots until the next regular rotation
		assert_eq!(Session::validators(), collators_with(both.clone()));
		System::set_block_number(2 * period - 1);
	});
	// WHEN the next regular rotation enacts the list without him
	let until = PeopleWestend::execute_with(|| {
		assert_eq!(System::block_number(), 2 * period);
		assert_eq!(Session::validators(), collators_with(vec![alice.clone()]));
		// THEN his drop of `drop_slots` counted slots starts there
		let until = counted_slots_on_people() + rule.drop_slots;
		let dropped = pallet_validator_collators::Dropped::<Runtime>::get();
		assert_eq!(dropped.get(&bob), Some(&Some(until)));
		// AND the next block, the next regular rotation, comes `drop_slots` slots later
		next_people_block_in_slot(
			u64::from(pallet_aura::CurrentSlot::<Runtime>::get()) + rule.drop_slots,
		);
		System::set_block_number(3 * period - 1);
		until
	});
	// AND that rotation, at the end of his drop, returns him again
	PeopleWestend::execute_with(|| {
		assert_eq!(System::block_number(), 3 * period);
		assert_eq!(counted_slots_on_people(), until);
		assert!(!pallet_validator_collators::Dropped::<Runtime>::get().contains_key(&bob));
		assert_eq!(
			pallet_validator_collators::LastActivity::<Runtime>::get(&bob),
			Some(pallet_validator_collators::Activity::Queued)
		);
	});
}
