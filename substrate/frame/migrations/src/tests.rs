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

use crate::{
	mock::{Test as T, *},
	mock_helpers::{MockedMigrationKind::*, *},
	Config, Cursor, Event, FailedMigrationHandling, MigrationCursor,
};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

#[docify::export]
#[test]
fn simple_works() {
	use Event::*;
	test_closure(|| {
		sp_tracing::try_init_simple();
		// Add three migrations, each taking one step longer than the previous.
		MockedMigrations::set(vec![
			(SucceedAfter, 10, 1),
			(SucceedAfter, 10, 2),
			(SucceedAfter, 10, 3),
		]);

		run_to_block(1);
		Migrations::on_runtime_upgrade();
		// Running to block two will execute all migrations.
		run_to_block(2);

		// Check that the executed migrations are recorded in `Historical`.
		assert_eq!(
			historic(),
			vec![
				mocked_id(SucceedAfter, 10, 1),
				mocked_id(SucceedAfter, 10, 2),
				mocked_id(SucceedAfter, 10, 3),
			]
		);

		// Check that we got all events.
		assert_events(vec![
			UpgradeStarted { migrations: 3 },
			MigrationCompleted { index: 0, took_blocks: 1, took_steps: 1 },
			MigrationCompleted { index: 1, took_blocks: 0, took_steps: 2 },
			MigrationCompleted { index: 2, took_blocks: 0, took_steps: 3 },
			UpgradeCompleted,
		]);
	});
}

/// Check that migrations reaching their `max_blocks` before the `max_steps`.
#[test]
fn simple_works_limited_by_blocks() {
	use Event::*;
	test_closure(|| {
		sp_tracing::try_init_simple();
		// Add two migrations, both with 10 max steps but different max blocks.
		MockedMigrations::set(vec![(SucceedAfter, 2, 8), (SucceedAfter, 1, 8)]);
		// These limits (minus overhead) give us enough weight for 4 steps per block:
		let limit = <Test as Config>::MaxServiceWeight::get().div(5);
		MockedMigrations::set_step_weight(limit);

		run_to_block(1);
		Migrations::on_runtime_upgrade();
		// Running to block two will execute all migrations.
		run_to_block(10);

		// Only the first migration executed correctly and was recorded in `Historical`.
		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 2, 8),]);

		// Check that we got all events.
		assert_events(vec![
			UpgradeStarted { migrations: 2 },
			MigrationAdvanced { index: 0, took_blocks: 1, took_steps: 5 },
			MigrationCompleted { index: 0, took_blocks: 2, took_steps: 8 },
			MigrationAdvanced { index: 1, took_blocks: 0, took_steps: 1 },
			MigrationAdvanced { index: 1, took_blocks: 1, took_steps: 5 },
			MigrationFailed { index: 1, took_blocks: 2, took_steps: 5 },
			UpgradeFailed,
		]);
	});
}

#[test]
fn failing_migration_sets_cursor_to_stuck() {
	test_closure(|| {
		FailedUpgradeResponse::set(FailedMigrationHandling::KeepStuck);
		MockedMigrations::set(vec![(FailAfter, 2, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Failed migrations are not recorded in `Historical`.
		assert!(historic().is_empty());
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationFailed { index: 0, took_blocks: 1, took_steps: 2 },
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
fn failing_migration_force_unstuck_works() {
	test_closure(|| {
		FailedUpgradeResponse::set(FailedMigrationHandling::ForceUnstuck);
		MockedMigrations::set(vec![(FailAfter, 2, 2)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Failed migrations are not recorded in `Historical`.
		assert!(historic().is_empty());
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationFailed { index: 0, took_blocks: 1, took_steps: 2 },
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
fn high_weight_migration_singular_fails() {
	test_closure(|| {
		MockedMigrations::set(vec![(HighWeightAfter(Weight::zero()), 0, 4)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Failed migrations are not recorded in `Historical`.
		assert!(historic().is_empty());
		// Check that we got all events.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationFailed { index: 0, took_blocks: 1, took_steps: 1 },
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
fn high_weight_migration_retries_once() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 0, 1), (HighWeightAfter(Weight::zero()), 0, 1)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 0, 1)]);
		// Check that we got all events.
		assert_events::<Event<T>>(vec![
			Event::UpgradeStarted { migrations: 2 },
			Event::MigrationCompleted { index: 0, took_blocks: 1, took_steps: 1 },
			// `took_blocks=1`, took_steps: 0 means that it was retried once. FAIL-CI comment
			Event::MigrationAdvanced { index: 1, took_blocks: 0, took_steps: 1 },
			Event::MigrationFailed { index: 1, took_blocks: 1, took_steps: 1 },
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
fn high_weight_migration_permanently_overweight_fails() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 0, 1), (HighWeightAfter(Weight::MAX), 0, 1)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		assert_eq!(historic(), vec![mocked_id(SucceedAfter, 0, 1)]);
		// Check that we got all events.
		assert_events::<Event<T>>(vec![
			Event::UpgradeStarted { migrations: 2 },
			Event::MigrationCompleted { index: 0, took_blocks: 1, took_steps: 1 },
			// `blocks=0` means that it was not retried.
			Event::MigrationFailed { index: 1, took_blocks: 0, took_steps: 1 },
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
			(SucceedAfter, 0, 1),
			(SucceedAfter, 0, 1), // duplicate
			(SucceedAfter, 1, 2),
			(SucceedAfter, 2, 3),
			(SucceedAfter, 1, 2), // duplicate
		]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(10);

		// Just three historical ones, since two were added twice.
		assert_eq!(
			historic(),
			vec![
				mocked_id(SucceedAfter, 0, 1),
				mocked_id(SucceedAfter, 1, 2),
				mocked_id(SucceedAfter, 2, 3),
			]
		);
		// Events received.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 5 },
			Event::MigrationCompleted { index: 0, took_blocks: 1, took_steps: 1 },
			Event::MigrationSkipped { index: 1 },
			Event::MigrationCompleted { index: 2, took_blocks: 0, took_steps: 2 },
			Event::MigrationCompleted { index: 3, took_blocks: 0, took_steps: 3 },
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
				mocked_id(SucceedAfter, 0, 1),
				mocked_id(SucceedAfter, 1, 2),
				mocked_id(SucceedAfter, 2, 3),
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
fn runtime_upgrade_fails_when_mbm_in_progress() {
	test_closure(|| {
		MockedMigrations::set(vec![(SucceedAfter, 2, 10)]);
		// Set up the weight so that it will not finish in a single block:
		MockedMigrations::set_step_weight(<Test as crate::Config>::MaxServiceWeight::get() / 2);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(3);

		// Events received.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took_blocks: 1, took_steps: 2 },
			Event::MigrationAdvanced { index: 0, took_blocks: 2, took_steps: 3 },
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

/// Migration takes more than `max_steps` steps.
#[test]
fn migration_timeout_steps_errors() {
	test_closure(|| {
		MockedMigrations::set(vec![(TimeoutAfter, 3, 3)]);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(5);

		// Times out after taking more than 3 steps.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationFailed { index: 0, took_blocks: 1, took_steps: 4 },
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

/// Migration takes more than `max_blocks` blocks.
#[test]
fn migration_timeout_blocks_errors() {
	test_closure(|| {
		MockedMigrations::set(vec![(TimeoutAfter, 1, 2)]);
		// Set up the weight so that it will not finish in a single block:
		MockedMigrations::set_step_weight(<Test as crate::Config>::MaxServiceWeight::get() / 2);

		System::set_block_number(1);
		Migrations::on_runtime_upgrade();
		run_to_block(5);

		// Times out after taking more than 3 steps.
		assert_events(vec![
			Event::UpgradeStarted { migrations: 1 },
			Event::MigrationAdvanced { index: 0, took_blocks: 1, took_steps: 2 },
			Event::MigrationFailed { index: 0, took_blocks: 2, took_steps: 2 },
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
