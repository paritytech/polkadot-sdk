// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Test the migration.

#![cfg(test)]

use super::*;

#[cfg(feature = "try-runtime")]
use frame_support::assert_ok;
use frame_support::{
	parameter_types,
	traits::{Footprint, HandleMessage, OnRuntimeUpgrade},
	StorageNoopGuard,
};
use sp_core::bounded_vec::BoundedSlice;
use sp_io::TestExternalities as TestExt;

parameter_types! {
	static RecordedMessages: u32 = 0;
}

struct MockedDmpHandler;
impl HandleMessage for MockedDmpHandler {
	type MaxMessageLen = ConstU32<16>;

	fn handle_message(_: BoundedSlice<u8, Self::MaxMessageLen>) {
		RecordedMessages::mutate(|n| *n += 1);
	}

	fn handle_messages<'a>(_: impl Iterator<Item = BoundedSlice<'a, u8, Self::MaxMessageLen>>) {
		unimplemented!()
	}

	fn sweep_queue() {
		unimplemented!()
	}

	fn footprint() -> Footprint {
		unimplemented!()
	}
}

parameter_types! {
	const PalletName: &'static str = "DmpQueue";
}

struct Runtime;
impl MigrationConfig for Runtime {
	type PalletName = PalletName;
	type DmpHandler = MockedDmpHandler;
	type DbWeight = ();
}

#[test]
fn migration_works() {
	TestExt::default().execute_with(|| {
		// This test should leak no storage:
		let _g = StorageNoopGuard::default();

		// Setup the storage:
		PageIndex::<Runtime>::set(PageIndexData {
			begin_used: 10,
			end_used: 20,
			overweight_count: 5,
		});

		for p in 10..20 {
			let msgs = (0..16).map(|i| (p, vec![i as u8; 1])).collect::<Vec<_>>();
			Pages::<Runtime>::insert(p, msgs);
		}

		for i in 0..5 {
			Overweight::<Runtime>::insert(i, (0, vec![i as u8; 1]));
		}

		// Run the migration:
		#[cfg(feature = "try-runtime")]
		assert_ok!(UndeployDmpQueue::<Runtime>::pre_upgrade());
		let _weight = UndeployDmpQueue::<Runtime>::on_runtime_upgrade();
		#[cfg(feature = "try-runtime")]
		assert_ok!(UndeployDmpQueue::<Runtime>::post_upgrade(vec![]));

		assert_eq!(RecordedMessages::take(), 10 * 16 + 5);

		// Test the storage removal:
		assert!(PageIndex::<Runtime>::exists(), "Not gone yet");
		DeleteDmpQueue::<Runtime>::on_runtime_upgrade();
		assert!(!PageIndex::<Runtime>::exists());
		assert!(!Pages::<Runtime>::contains_key(10));
		assert!(!Overweight::<Runtime>::contains_key(0));
	});
}

#[test]
fn migration_too_long_ignored() {
	TestExt::default().execute_with(|| {
		// This test should leak no storage:
		//let _g = StorageNoopGuard::default();

		// Setup the storage:
		PageIndex::<Runtime>::set(PageIndexData {
			begin_used: 10,
			end_used: 11,
			overweight_count: 2,
		});

		let short = vec![1; 16];
		let long = vec![0; 17];
		Pages::<Runtime>::insert(10, vec![(10, short.clone()), (10, long.clone())]);
		// Insert one good and one bad overweight msg:
		Overweight::<Runtime>::insert(0, (0, short.clone()));
		Overweight::<Runtime>::insert(1, (0, long.clone()));

		// Run the migration:
		#[cfg(feature = "try-runtime")]
		assert_ok!(UndeployDmpQueue::<Runtime>::pre_upgrade());
		let _weight = UndeployDmpQueue::<Runtime>::on_runtime_upgrade();
		#[cfg(feature = "try-runtime")]
		assert_ok!(UndeployDmpQueue::<Runtime>::post_upgrade(vec![]));

		assert_eq!(RecordedMessages::take(), 2);

		// Test the storage removal:
		assert!(PageIndex::<Runtime>::exists(), "Not gone yet");
		DeleteDmpQueue::<Runtime>::on_runtime_upgrade();
		assert!(!PageIndex::<Runtime>::exists());
		assert!(!Pages::<Runtime>::contains_key(10));
		assert!(!Overweight::<Runtime>::contains_key(0));
	});
}
