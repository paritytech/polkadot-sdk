// This file is part of Substrate.

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

#![cfg(test)]

use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use crate::{
	mock::{Test as T, *},
	mock_helpers::{MockedMigrationKind::*, *},
	Cursor,
	Event,
	FailedMigrationHandling,
	MbmIsOngoing,
	MbmProgress,
	MigrationCursor,
	// MigrationSteps,
};

#[docify::export]
#[test]
fn simple_works() {
	use Event::*;
	test_closure(|| {
		// Add three migrations, each taking one block longer than the previous.
		MockedMigrations::set(vec![(SucceedAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Check that the executed migrations are recorded in `Historical`.
		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 2),]);

		// Check that we got all events.
		assert_events(vec![
			UpgradeStarted { migrations: 1 },
			MigrationAdvanced { index: 0, took: 1 },
			MigrationAdvanced { index: 0, took: 2 },
			MigrationCompleted { index: 0, took: 3 },
			UpgradeCompleted,
		]);
	});
}

#[test]
fn simple_multiple_works() {
	use Event::*;
	test_closure(|| {
		// Add three migrations, each taking one block longer than the previous.
		MockedMigrations::set(vec![(SucceedAfter, 0), (SucceedAfter, 1), (SucceedAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Check that the executed migrations are recorded in `Historical`.
		assert_eq!(
			historic(),
			vec![
				mocked_id(SucceedAfter, 0),
				mocked_id(SucceedAfter, 1),
				mocked_id(SucceedAfter, 2),
			]
		);

		// Check that we got all events.
		assert_events(vec![
			UpgradeStarted { migrations: 3 },
			MigrationCompleted { index: 0, took: 1 },
			MigrationAdvanced { index: 1, took: 0 },
			MigrationCompleted { index: 1, took: 1 },
			MigrationAdvanced { index: 2, took: 0 },
			MigrationAdvanced { index: 2, took: 1 },
			MigrationCompleted { index: 2, took: 2 },
			UpgradeCompleted,
		]);
	});
}

#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn failing_migration_sets_cursor_to_stuck() {
	test_closure(|| {
		FailedUpgradeResponse::set(FailedMigrationHandling::KeepStuck);
		MockedMigrations::set(vec![(FailAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Failed migrations are not recorded in `Historical`.
		assert!(historic().is_empty());
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took: 1 },
			Event::MigrationAdvanced { index: 0, took: 2 },
			Event::MigrationFailed { index: 0, took: 3 },
			Event::UpgradeFailed,
		]);

		// Check that the handler was called correctly.
		assert_eq!(UpgradesStarted::take(), 1);
		assert_eq!(UpgradesCompleted::take(), 0);
		assert_eq!(UpgradesFailed::take(), vec![Some(0)]);

		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck), "Must stuck the chain");
	});
}

#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn failing_migration_force_unstuck_works() {
	test_closure(|| {
		FailedUpgradeResponse::set(FailedMigrationHandling::ForceUnstuck);
		MockedMigrations::set(vec![(FailAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Failed migrations are not recorded in `Historical`.
		assert!(historic().is_empty());
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took: 1 },
			Event::MigrationAdvanced { index: 0, took: 2 },
			Event::MigrationFailed { index: 0, took: 3 },
			Event::UpgradeFailed,
		]);

		// Check that the handler was called correctly.
		assert_eq!(UpgradesStarted::take(), 1);
		assert_eq!(UpgradesCompleted::take(), 0);
		assert_eq!(UpgradesFailed::take(), vec![Some(0)]);

		assert!(Cursor::<T>::get().is_none(), "Must unstuck the chain");
	});
}

/// A migration that reports not getting enough weight errors if it is the first one to run in that
/// block.
#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn high_weight_migration_singular_fails() {
	test_closure(|| {
		MockedMigrations::set(vec![(HighWeightAfter(Weight::zero()), 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Failed migrations are not recorded in `Historical`.
		assert!(historic().is_empty());
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took: 1 },
			Event::MigrationAdvanced { index: 0, took: 2 },
			Event::MigrationFailed { index: 0, took: 3 },
			Event::UpgradeFailed,
		]);

		// Check that the handler was called correctly.
		assert_eq!(upgrades_started_completed_failed(), (1, 0, 1));
		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck));
	});
}

/// A migration that reports of not getting enough weight is retried once, if it is not the first
/// one to run in a block.
#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn high_weight_migration_retries_once() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 0), (HighWeightAfter(Weight::zero()), 0)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 0)]);
		// Check that we got all events.
		assert_events::<Event<T>>(vec![
			Event::UpgradeStarted { migrations: 2 },
			Event::MigrationCompleted { index: 0, took: 1 },
			// `took=1` means that it was retried once.
			Event::MigrationFailed { index: 1, took: 1 },
			Event::UpgradeFailed,
		]);

		// Check that the handler was called correctly.
		assert_eq!(upgrades_started_completed_failed(), (1, 0, 1));
		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck));
	});
}

/// If a migration uses more weight than the limit, then it will not retry but fail even when it is
/// not the first one in the block.
// Note: Same as `high_weight_migration_retries_once` but with different required weight for the
// migration.
#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn high_weight_migration_permanently_overweight_fails() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 0), (HighWeightAfter(Weight::MAX), 0)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 0)]);
		// Check that we got all events.
		assert_events::<Event<T>>(vec![
			Event::UpgradeStarted { migrations: 2 },
			Event::MigrationCompleted { index: 0, took: 1 },
			// `blocks=0` means that it was not retried.
			Event::MigrationFailed { index: 1, took: 0 },
			Event::UpgradeFailed,
		]);

		// Check that the handler was called correctly.
		assert_eq!(upgrades_started_completed_failed(), (1, 0, 1));
		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck));
	});
}

#[test]
fn historic_skipping_works() {
	test_closure(|| {
		MockedMigrations::set(vec![
			(SucceedAfter, 0),
			(SucceedAfter, 0), // duplicate
			(SucceedAfter, 1),
			(SucceedAfter, 2),
			(SucceedAfter, 1), // duplicate
		]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Just three historical ones, since two were added twice.
		assert_eq!(
			historic(),
			vec![
				mocked_id(SucceedAfter, 0),
				mocked_id(SucceedAfter, 1),
				mocked_id(SucceedAfter, 2),
			]
		);
		// Events received.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 5 },
			Event::MigrationCompleted { index: 0, took: 1 },
			Event::MigrationSkipped { index: 1 },
			Event::MigrationAdvanced { index: 2, took: 0 },
			Event::MigrationCompleted { index: 2, took: 1 },
			Event::MigrationAdvanced { index: 3, took: 0 },
			Event::MigrationAdvanced { index: 3, took: 1 },
			Event::MigrationCompleted { index: 3, took: 2 },
			Event::MigrationSkipped { index: 4 },
			Event::UpgradeCompleted,
		]);
		assert_eq!(upgrades_started_completed_failed(), (1, 1, 0));

		// Now go for another upgrade; just to make sure that it wont execute again.
		System::reset_events();
		Migrations::on_runtime_upgrade();
		run_to_block(20);

		// Same historical ones as before.
		assert_eq!(
			historic(),
			vec![
				mocked_id(SucceedAfter, 0),
				mocked_id(SucceedAfter, 1),
				mocked_id(SucceedAfter, 2),
			]
		);

		// Everything got skipped.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 5 },
			Event::MigrationSkipped { index: 0 },
			Event::MigrationSkipped { index: 1 },
			Event::MigrationSkipped { index: 2 },
			Event::MigrationSkipped { index: 3 },
			Event::MigrationSkipped { index: 4 },
			Event::UpgradeCompleted,
		]);
		assert_eq!(upgrades_started_completed_failed(), (1, 1, 0));
	});
}

/// When another upgrade happens while a migration is still running, it should set the cursor to
/// stuck.
#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn upgrade_fails_when_migration_active() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 10)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(3);

		// Events received.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took: 1 },
			Event::MigrationAdvanced { index: 0, took: 2 },
		]);
		assert_eq!(upgrades_started_completed_failed(), (1, 0, 0));

		// Upgrade again.
		Migrations::on_runtime_upgrade();
		// -- Defensive path --
		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck));
		assert_events(vec![Event::UpgradeFailed]);
		assert_eq!(upgrades_started_completed_failed(), (0, 0, 1));
	});
}

#[test]
#[cfg_attr(feature = "try-runtime", should_panic)]
fn migration_timeout_errors() {
	test_closure(|| {
		MockedMigrations::set(vec![(TimeoutAfter, 3)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(5);

		// Times out after taking more than 3 steps.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took: 1 },
			Event::MigrationAdvanced { index: 0, took: 2 },
			Event::MigrationAdvanced { index: 0, took: 3 },
			Event::MigrationAdvanced { index: 0, took: 4 },
			Event::MigrationFailed { index: 0, took: 4 },
			Event::UpgradeFailed,
		]);
		assert_eq!(upgrades_started_completed_failed(), (1, 0, 1));

		// Failed migrations are not black-listed.
		assert!(historic().is_empty());
		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck));

		Migrations::on_runtime_upgrade();
		run_to_block(6);

		assert_events(vec![Event::UpgradeFailed]);
		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck));
		assert_eq!(upgrades_started_completed_failed(), (0, 0, 1));
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_runtime_success_case() {
	use Event::*;
	test_closure(|| {
		// Add three migrations, each taking one block longer than the previous.
		MockedMigrations::set(vec![(SucceedAfter, 0), (SucceedAfter, 1), (SucceedAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Check that we got all events.
		assert_events(vec![
			UpgradeStarted { migrations: 3 },
			MigrationCompleted { index: 0, took: 1 },
			MigrationAdvanced { index: 1, took: 0 },
			MigrationCompleted { index: 1, took: 1 },
			MigrationAdvanced { index: 2, took: 0 },
			MigrationAdvanced { index: 2, took: 1 },
			MigrationCompleted { index: 2, took: 2 },
			UpgradeCompleted,
		]);
	});
}

#[test]
#[cfg(feature = "try-runtime")]
#[should_panic]
fn try_runtime_pre_upgrade_failure() {
	test_closure(|| {
		// Add three migrations, it should fail after the second one.
		MockedMigrations::set(vec![(SucceedAfter, 0), (PreUpgradeFail, 1), (SucceedAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();

		// should panic
		run_to_block(10);
	});
}

#[test]
#[cfg(feature = "try-runtime")]
#[should_panic]
fn try_runtime_post_upgrade_failure() {
	test_closure(|| {
		// Add three migrations, it should fail after the second one.
		MockedMigrations::set(vec![(SucceedAfter, 0), (PostUpgradeFail, 1), (SucceedAfter, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();

		// should panic
		run_to_block(10);
	});
}

#[test]
#[cfg(feature = "try-runtime")]
#[should_panic]
fn try_runtime_migration_failure() {
	test_closure(|| {
		// Add three migrations, it should fail after the second one.
		MockedMigrations::set(vec![(SucceedAfter, 0), (FailAfter, 5), (SucceedAfter, 10)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();

		// should panic
		run_to_block(10);
	});
}

#[test]
fn try_runtime_no_migrations() {
	test_closure(|| {
		MockedMigrations::set(vec![]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();

		run_to_block(10);

		assert_eq!(System::events().len(), 0);
	});
}

#[test]
fn view_function_ongoing_status_works() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 2), (SucceedAfter, 3), (SucceedAfter, 5)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(2);

		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 0,
				total_migrations: 3,
				current_migration_steps: 1,
				current_migration_max_steps: 2,
			})
		);

		run_to_block(3);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 0,
				total_migrations: 3,
				current_migration_steps: 2,
				current_migration_max_steps: 2,
			})
		);

		run_to_block(4);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 1,
				total_migrations: 3,
				current_migration_steps: 0,
				current_migration_max_steps: 3,
			})
		);

		run_to_block(5);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 1,
				total_migrations: 3,
				current_migration_steps: 1,
				current_migration_max_steps: 3,
			})
		);

		run_to_block(6);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 1,
				total_migrations: 3,
				current_migration_steps: 2,
				current_migration_max_steps: 3,
			})
		);

		run_to_block(7);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 2,
				total_migrations: 3,
				current_migration_steps: 0,
				current_migration_max_steps: 5,
			})
		);
	});
}

#[test]
#[cfg(feature = "try-runtime")]
#[should_panic]
fn migration_steps_on_failure_works() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 1), (FailAfter, 3), (SucceedAfter, 10)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 0,
				total_migrations: 3,
				current_migration_steps: 0,
				current_migration_max_steps: 1,
			})
		);
		run_to_block(2);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Yes);
		assert_eq!(
			Migrations::progress(),
			Some(MbmProgress {
				current_migration: 0,
				total_migrations: 3,
				current_migration_steps: 1,
				current_migration_max_steps: 1,
			})
		);

		run_to_block(6);
		assert_eq!(Migrations::ongoing_status(), MbmIsOngoing::Stuck);
		assert_eq!(Migrations::progress(), None);

		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 1),]);
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 3 },
			Event::MigrationAdvanced { index: 0, took: 1 },
			Event::MigrationCompleted { index: 0, took: 2 },
			Event::MigrationAdvanced { index: 1, took: 0 },
			Event::MigrationAdvanced { index: 1, took: 1 },
			Event::MigrationAdvanced { index: 1, took: 2 },
			Event::MigrationFailed { index: 1, took: 3 },
			Event::UpgradeFailed,
		]);

		assert_eq!(upgrades_started_completed_failed(), (1, 0, 1));

		assert_eq!(Cursor::<T>::get(), Some(MigrationCursor::Stuck), "Must stuck the chain");
	});
}

#[test]
fn view_function_status_detailed() {
	test_closure(|| {
		// Set up multiple migrations
		MockedMigrations::set(vec![(SucceedAfter, 2), (SucceedAfter, 1)]);

		System::set_block_number(1);
		// Check status before upgrade
		let prev_status = Migrations::status();
		assert_eq!(prev_status.ongoing, MbmIsOngoing::No);
		assert!(prev_status.progress.is_none());
		assert_eq!(prev_status.prefixes, Vec::<Vec<u8>>::new());

		Migrations::on_runtime_upgrade();

		let status = Migrations::status();
		let progress = status.progress.unwrap();
		assert_eq!(progress.current_migration, 0);
		assert_eq!(progress.total_migrations, 2);
		assert_eq!(progress.current_migration_steps, 0);
		assert_eq!(progress.current_migration_max_steps, 2);
		assert_eq!(status.prefixes, vec![mocked_id(SucceedAfter, 2).into_inner()]);

		// After first step
		run_to_block(2);
		let status = Migrations::status();
		let progress = status.progress.unwrap();
		assert_eq!(progress.current_migration_steps, 1);
		assert_eq!(status.prefixes, vec![mocked_id(SucceedAfter, 2).into_inner()]);

		// After second step (migration 0 completes)
		run_to_block(3);
		let status = Migrations::status();
		let progress = status.progress.unwrap();
		assert_eq!(progress.current_migration, 0);
		assert_eq!(progress.current_migration_steps, 2);
		assert_eq!(progress.current_migration_max_steps, 2);
		assert_eq!(status.prefixes, vec![mocked_id(SucceedAfter, 2).into_inner()]);

		run_to_block(4);
		let status = Migrations::status();
		let progress = status.progress.unwrap();
		assert_eq!(progress.current_migration, 1);
		assert_eq!(progress.current_migration_steps, 0);
		assert_eq!(progress.current_migration_max_steps, 1);
		assert_eq!(status.prefixes, vec![mocked_id(SucceedAfter, 1).into_inner()]);

		// After third step (migration 1 completes)
		run_to_block(5);
		let status = Migrations::status();
		assert_eq!(status.ongoing, MbmIsOngoing::No);
		assert!(status.progress.is_none());
	});
}
