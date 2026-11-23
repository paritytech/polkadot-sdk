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

use crate::nodes::jam::{
	authorizer::{AuraPublic, AuthoritySource, Authorizers, RuntimeAuthorities},
	message::AuthoringHold,
};
use cumulus_client_jam_authorizer::testing::authorizer_of;
use jam_interface::Slot as JamSlot;
use sp_core::H256;
use std::sync::Arc;

/// The round-robin is still read off the anchor slot: the set's own size and the para's slot
/// duration decide whose turn it is, and only that turn may author.
#[test]
fn the_round_robin_names_one_collator_per_para_slot() {
	let bob = authorizer_of("alice,bob", "Bob", 3);

	// Para slot = 3 timeslots; bob is collator 1, so timeslots 3..=5 and 9..=11 are his.
	assert!(bob.names_us(3) && bob.names_us(5) && bob.names_us(11));
	assert!(!bob.names_us(2) && !bob.names_us(6));
	assert_eq!(bob.round_robin_window(), 6, "one full turn of two collators");
}

/// A runtime that schedules one set change at `switch`, as `pallet-jam-aura` announces it.
struct ScheduledSet {
	switch: JamSlot,
	current: Vec<AuraPublic<sp_consensus_aura::sr25519::AuthorityId>>,
	next: Vec<AuraPublic<sp_consensus_aura::sr25519::AuthorityId>>,
}

impl RuntimeAuthorities<cumulus_test_runtime::Block, sp_consensus_aura::sr25519::AuthorityId>
	for ScheduledSet
{
	fn authorities_at(
		&self,
		_: H256,
		anchor_slot: JamSlot,
	) -> Result<Vec<AuraPublic<sp_consensus_aura::sr25519::AuthorityId>>, String> {
		Ok(if anchor_slot < self.switch { self.current.clone() } else { self.next.clone() })
	}
}

/// The authorizer the collator picks for an anchor slot is the set JAM authorizes there: the
/// old set before the scheduled switch slot, the new one from the switch slot on. This is what
/// makes a collator sign with the set the core's pool holds at the package's anchor.
#[test]
fn the_authorizer_is_the_sets_at_the_anchor_slot() {
	use cumulus_client_jam_authorizer::testing::{authorities, keystore_of};
	use sp_consensus_aura::sr25519::AuthorityId as Sr25519AuraId;

	let switch = 100;
	let source = Arc::new(ScheduledSet {
		switch,
		current: authorities::<Sr25519AuraId>("alice,bob"),
		next: authorities::<Sr25519AuraId>("charlie,bob"),
	});
	let authorizers = Authorizers::<cumulus_test_runtime::Block, Sr25519AuraId>::new(
		source,
		std::path::PathBuf::from(cumulus_client_jam_authorizer::testing::SOME_BLOB),
		cumulus_client_jam_authorizer::testing::PARA_ID,
		cumulus_client_jam_authorizer::testing::SERVICE_ID,
		1,
		keystore_of::<Sr25519AuraId>("Bob"),
	);

	let before = authorizers
		.for_anchor(H256::zero(), switch - 1)
		.expect("the old set derives")
		.expect("Bob is in the old set");
	let after = authorizers
		.for_anchor(H256::zero(), switch)
		.expect("the new set derives")
		.expect("Bob is in the new set");

	assert_eq!(before.own_index(), 1, "Bob is second in the old set");
	assert_eq!(after.own_index(), 1, "Bob is second in the new set too");
	assert_ne!(
		before.hash(),
		after.hash(),
		"the switch changes the authorizer the core's pool must hold",
	);
}

/// A node outside the set at an anchor slot gets `None`, not an error: under slot-bound sets
/// that is the ordinary state of waiting for a set that names it.
#[test]
fn an_anchor_slot_that_does_not_name_us_is_none() {
	use cumulus_client_jam_authorizer::testing::{authorities, keystore_of};
	use sp_consensus_aura::sr25519::AuthorityId as Sr25519AuraId;

	let source = Arc::new(ScheduledSet {
		switch: 10,
		current: authorities::<Sr25519AuraId>("alice,bob"),
		next: authorities::<Sr25519AuraId>("charlie,dave"),
	});
	let authorizers = Authorizers::<cumulus_test_runtime::Block, Sr25519AuraId>::new(
		source,
		std::path::PathBuf::from(cumulus_client_jam_authorizer::testing::SOME_BLOB),
		cumulus_client_jam_authorizer::testing::PARA_ID,
		cumulus_client_jam_authorizer::testing::SERVICE_ID,
		1,
		keystore_of::<Sr25519AuraId>("Bob"),
	);

	assert!(authorizers.for_anchor(H256::zero(), 9).expect("derives").is_some());
	assert!(authorizers.for_anchor(H256::zero(), 10).expect("derives").is_none());
}

/// The collation task is the sole writer and the builder reads the latest value at tick time;
/// clones must share the one counter.
#[test]
fn the_authoring_hold_tracks_the_pending_count() {
	let hold = AuthoringHold::new();
	assert_eq!(hold.pending(), 0);

	hold.set_pending(3);
	assert_eq!(hold.pending(), 3);
	assert_eq!(hold.clone().pending(), 3, "clones share the counter");

	hold.set_pending(0);
	assert_eq!(hold.pending(), 0);
}
