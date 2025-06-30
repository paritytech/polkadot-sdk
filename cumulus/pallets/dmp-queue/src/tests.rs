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

//! Test the lazy migration.

#![cfg(test)]

use super::{migration::*, mock::*};
use crate::*;

use frame_support::{pallet_prelude::*, traits::OnIdle, StorageNoopGuard};

#[test]
fn migration_works() {
	let mut ext = new_test_ext();
	ext.execute_with(|| {
		sp_tracing::try_init_simple();
		// Insert some storage:
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
		testing_only::Configuration::<Runtime>::put(123);
	});
	// We need to commit, otherwise the keys are removed from the overlay; not the backend.
	ext.commit_all().unwrap();
	ext.execute_with(|| {
		// Run one step of the migration:
		pre_upgrade_checks::<Runtime>();
		run_to_block(1);
		// First we expect a StartedExport event:
		assert_only_event(Event::StartedExport);

		// Then we expect 10 Exported events:
		for page in 0..10 {
			run_to_block(2 + page);
			assert_only_event(Event::Exported { page: page as u32 + 10 });
			assert!(!Pages::<Runtime>::contains_key(page as u32), "Page is gone");
			assert_eq!(
				MigrationStatus::<Runtime>::get(),
				MigrationState::StartedExport { next_begin_used: page as u32 + 11 }
			);
		}

		// Then we expect a CompletedExport event:
		run_to_block(12);
		assert_only_event(Event::CompletedExport);
		assert_eq!(MigrationStatus::<Runtime>::get(), MigrationState::CompletedExport);

		// Then we expect a StartedOverweightExport event:
		run_to_block(13);
		assert_only_event(Event::StartedOverweightExport);
		assert_eq!(
			MigrationStatus::<Runtime>::get(),
			MigrationState::StartedOverweightExport { next_overweight_index: 0 }
		);

		// Then we expect 5 ExportedOverweight events:
		for index in 0..5 {
			run_to_block(14 + index);
			assert_only_event(Event::ExportedOverweight { index });
			assert!(!Overweight::<Runtime>::contains_key(index), "Overweight msg is gone");
			assert_eq!(
				MigrationStatus::<Runtime>::get(),
				MigrationState::StartedOverweightExport { next_overweight_index: index + 1 }
			);
		}

		// Then we expect a CompletedOverweightExport event:
		run_to_block(19);
		assert_only_event(Event::CompletedOverweightExport);
		assert_eq!(MigrationStatus::<Runtime>::get(), MigrationState::CompletedOverweightExport);

		// Then we expect a StartedCleanup event:
		run_to_block(20);
		assert_only_event(Event::StartedCleanup);
		assert_eq!(
			MigrationStatus::<Runtime>::get(),
			MigrationState::StartedCleanup { cursor: None }
		);
	});
	ext.commit_all().unwrap();
	// Then it cleans up the remaining storage items:
	ext.execute_with(|| {
		run_to_block(21);
		assert_only_event(Event::CleanedSome { keys_removed: 2 });
	});
	ext.commit_all().unwrap();
	ext.execute_with(|| {
		run_to_block(22);
		assert_only_event(Event::CleanedSome { keys_removed: 2 });
	});
	ext.commit_all().unwrap();
	ext.execute_with(|| {
		run_to_block(24);
		assert_eq!(
			System::events().into_iter().map(|e| e.event).collect::<Vec<_>>(),
			vec![
				Event::CleanedSome { keys_removed: 2 }.into(),
				Event::Completed { error: false }.into()
			]
		);
		System::reset_events();
		assert_eq!(MigrationStatus::<Runtime>::get(), MigrationState::Completed);

		post_upgrade_checks::<Runtime>();
		assert_eq!(RecordedMessages::take().len(), 10 * 16 + 5);

		// Test the storage removal:
		assert!(!PageIndex::<Runtime>::exists());
		assert!(!testing_only::Configuration::<Runtime>::exists());
		assert_eq!(Pages::<Runtime>::iter_keys().count(), 0);
		assert_eq!(Overweight::<Runtime>::iter_keys().count(), 0);

		// The `MigrationStatus` never disappears and there are no more storage changes:
		{
			let _g = StorageNoopGuard::default();

			run_to_block(100);
			assert_eq!(MigrationStatus::<Runtime>::get(), MigrationState::Completed);
			assert!(System::events().is_empty());
			// ... besides the block number
			System::set_block_number(24);
		}
	});
}

/// Too long messages are dropped by the migration.
#[test]
fn migration_too_long_ignored() {
	new_test_ext().execute_with(|| {
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
		pre_upgrade_checks::<Runtime>();
		run_to_block(100);
		post_upgrade_checks::<Runtime>();

		assert_eq!(RecordedMessages::take(), vec![short.clone(), short]);

		// Test the storage removal:
		assert!(!PageIndex::<Runtime>::exists());
		assert_eq!(Pages::<Runtime>::iter_keys().count(), 0);
		assert_eq!(Overweight::<Runtime>::iter_keys().count(), 0);
	});
}

fn run_to_block(n: u64) {
	System::run_to_block_with::<AllPalletsWithSystem>(
		n,
		frame_system::RunToBlockHooks::default().after_initialize(|bn| {
			AllPalletsWithSystem::on_idle(bn, Weight::MAX);
		}),
	);
}

fn assert_only_event(e: Event<Runtime>) {
	assert_eq!(System::events().pop().expect("Event expected").event, e.clone().into());
	assert_eq!(System::events().len(), 1, "Got events: {:?} but wanted {:?}", System::events(), e);
	System::reset_events();
}

/// TESTING ONLY
fn pre_upgrade_checks<T: crate::Config>() {
	let index = PageIndex::<T>::get();

	// Check that all pages are present.
	assert!(index.begin_used <= index.end_used, "Invalid page index");
	for p in index.begin_used..index.end_used {
		assert!(Pages::<T>::contains_key(p), "Missing page");
		assert!(Pages::<T>::get(p).len() > 0, "Empty page");
	}

	// Check that all overweight messages are present.
	for i in 0..index.overweight_count {
		assert!(Overweight::<T>::contains_key(i), "Missing overweight message");
	}
}

/// TESTING ONLY
fn post_upgrade_checks<T: crate::Config>() {
	let index = PageIndex::<T>::get();

	// Check that all pages are removed.
	for p in index.begin_used..index.end_used {
		assert!(!Pages::<T>::contains_key(p), "Page should be gone");
	}
	assert!(Pages::<T>::iter_keys().next().is_none(), "Un-indexed pages");

	// Check that all overweight messages are removed.
	for i in 0..index.overweight_count {
		assert!(!Overweight::<T>::contains_key(i), "Overweight message should be gone");
	}
	assert!(Overweight::<T>::iter_keys().next().is_none(), "Un-indexed overweight messages");
}
