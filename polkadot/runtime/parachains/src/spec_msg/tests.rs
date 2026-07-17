// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use crate::mock::{new_test_ext, MockGenesisConfig, Test};
use polkadot_primitives::Hash;

fn root(byte: u8) -> StreamsRoot {
	StreamsRoot(Hash::repeat_byte(byte))
}

/// A root derived from an index, unique for `i < u32::MAX`.
fn root_at(i: u32) -> StreamsRoot {
	StreamsRoot(Hash::from_low_u64_be(i as u64 + 1))
}

fn requires(entries: impl IntoIterator<Item = (u32, StreamsRoot)>) -> RequiresSet {
	RequiresSet::try_from_iter(entries.into_iter().map(|(id, root)| (ParaId::from(id), root)))
		.expect("test entries are unique and within bounds; qed")
}

fn assert_matches(source: u32, root: StreamsRoot) {
	assert!(Pallet::<Test>::check_requires(&requires([(source, root)])).is_ok());
}

fn assert_no_match(source: u32, root: StreamsRoot) {
	assert!(Pallet::<Test>::check_requires(&requires([(source, root)])).is_err());
}

#[test]
fn ring_rotation_evicts_oldest() {
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let sender = ParaId::from(1000);
		let k = 5;

		frame_system::Pallet::<Test>::set_block_number(1);
		for i in 0..RECENT_PROVIDES_WINDOW + k {
			Pallet::<Test>::note_provides(sender, root_at(i));
		}

		let ring = RecentProvides::<Test>::get(sender);
		assert_eq!(ring.entries().len(), RECENT_PROVIDES_WINDOW as usize);

		// The oldest `k` roots were evicted and no longer match.
		for i in 0..k {
			assert_no_match(1000, root_at(i));
		}
		// Any root still in the ring matches.
		for i in k..RECENT_PROVIDES_WINDOW + k {
			assert_matches(1000, root_at(i));
		}
	});
}

#[test]
fn entries_are_ordered_and_note_block_numbers() {
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let sender = ParaId::from(1000);

		frame_system::Pallet::<Test>::set_block_number(3);
		Pallet::<Test>::note_provides(sender, root(1));
		frame_system::Pallet::<Test>::set_block_number(7);
		Pallet::<Test>::note_provides(sender, root(2));
		Pallet::<Test>::note_provides(sender, root(3));

		let ring = RecentProvides::<Test>::get(sender);
		assert_eq!(ring.entries(), &[(root(1), 3), (root(2), 7), (root(3), 7)]);
	});
}

#[test]
fn absent_source_never_matches() {
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		// No provides were ever pushed for para 42 (e.g. unregistered): the entry just
		// fails to match, there is no dedicated error path and no storage is created.
		assert_no_match(42, root(1));
		assert!(!RecentProvides::<Test>::contains_key(ParaId::from(42)));
	});
}

#[test]
fn multiple_sources_all_must_match() {
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
		Pallet::<Test>::note_provides(ParaId::from(1000), root(1));
		Pallet::<Test>::note_provides(ParaId::from(2000), root(2));

		// All entries present: the set matches.
		assert!(
			Pallet::<Test>::check_requires(&requires([(1000, root(1)), (2000, root(2))])).is_ok()
		);

		// One entry missing (wrong root or absent source): the whole set fails, and the
		// offending entry is reported.
		let err = Pallet::<Test>::check_requires(&requires([(1000, root(1)), (2000, root(9))]))
			.unwrap_err();
		assert_eq!(err.source, ParaId::from(2000));
		assert_eq!(err.root, root(9));

		let err = Pallet::<Test>::check_requires(&requires([(1000, root(1)), (3000, root(3))]))
			.unwrap_err();
		assert_eq!(err.source, ParaId::from(3000));

		// The empty set trivially matches (candidates without dependencies).
		assert!(Pallet::<Test>::check_requires(&Default::default()).is_ok());
	});
}

#[test]
fn evict_after_revert_drops_entries_of_reverted_blocks() {
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let sender_a = ParaId::from(1000);
		let sender_b = ParaId::from(2000);

		frame_system::Pallet::<Test>::set_block_number(5);
		Pallet::<Test>::note_provides(sender_a, root(1));
		frame_system::Pallet::<Test>::set_block_number(6);
		Pallet::<Test>::note_provides(sender_a, root(2));
		Pallet::<Test>::note_provides(sender_b, root(3));
		frame_system::Pallet::<Test>::set_block_number(7);
		Pallet::<Test>::note_provides(sender_a, root(4));

		// Revert back to block 5: everything pushed at blocks 6 and 7 must stop matching,
		// across all senders.
		Pallet::<Test>::evict_after_revert(5);

		assert_eq!(RecentProvides::<Test>::get(sender_a).entries(), &[(root(1), 5)]);
		assert_matches(1000, root(1));
		assert_no_match(1000, root(2));
		assert_no_match(1000, root(4));
		assert_no_match(2000, root(3));
		// A ring emptied by the eviction is removed entirely.
		assert!(!RecentProvides::<Test>::contains_key(sender_b));
	});
}

#[test]
fn offboarding_clears_the_ring() {
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let outgoing = ParaId::from(1000);
		let staying = ParaId::from(2000);

		frame_system::Pallet::<Test>::set_block_number(1);
		Pallet::<Test>::note_provides(outgoing, root(1));
		Pallet::<Test>::note_provides(staying, root(2));

		Pallet::<Test>::initializer_on_new_session(&Default::default(), &[outgoing]);

		assert!(!RecentProvides::<Test>::contains_key(outgoing));
		assert_no_match(1000, root(1));
		assert_matches(2000, root(2));
	});
}

#[test]
fn ring_is_readable_under_the_well_known_key_as_a_plain_vec() {
	// The receiver-side node monitor (`cumulus-client-spec-msg`) reads the ring from relay
	// chain state under `well_known_keys::spec_msg_recent_provides` and decodes it as a
	// plain `Vec<(StreamsRoot, BlockNumber)>` — pin both halves of that contract here,
	// where the pallet placement (`SpecMsg`) and the `RecentRoots` layout are in scope.
	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let sender = ParaId::from(2000);

		frame_system::Pallet::<Test>::set_block_number(7);
		Pallet::<Test>::note_provides(sender, root(1));
		Pallet::<Test>::note_provides(sender, root(2));

		let raw = sp_io::storage::get(
			&polkadot_primitives::well_known_keys::spec_msg_recent_provides(sender),
		)
		.expect("the well-known key is where the pallet writes the ring");
		let decoded: Vec<(StreamsRoot, u32)> =
			Decode::decode(&mut &raw[..]).expect("a ring decodes as a plain vec of entries");
		assert_eq!(decoded, vec![(root(1), 7), (root(2), 7)]);
	});
}
