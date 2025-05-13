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

use super::*;

/// Backward mapping from https://github.com/paritytech/polkadot-sdk/blob/74a5e1a242274ddaadac1feb3990fc95c8612079/substrate/frame/balances/src/types.rs#L38
pub fn map_lock_reason(reasons: LockReasons) -> LockWithdrawReasons {
	match reasons {
		LockReasons::All => LockWithdrawReasons::TRANSACTION_PAYMENT | LockWithdrawReasons::RESERVE,
		LockReasons::Fee => LockWithdrawReasons::TRANSACTION_PAYMENT,
		LockReasons::Misc => LockWithdrawReasons::TIP,
	}
}

/// Relay Chain pallet list with indexes.
#[derive(Encode, Decode)]
pub enum RcPalletConfig {
	#[codec(index = 255)]
	RcmController(RcMigratorCall),
}

/// Call encoding for the calls needed from the rc-migrator pallet.
#[derive(Encode, Decode)]
pub enum RcMigratorCall {
	#[codec(index = 2)]
	StartDataMigration,
	#[codec(index = 3)]
	UpdateAhMsgProcessedCount(u32),
}

/// Trait to run some checks on the Asset Hub before and after a pallet migration.
///
/// This needs to be called by the test harness.
pub trait AhMigrationCheck {
	/// Relay Chain payload which is exported for migration checks.
	type RcPrePayload: Clone;
	/// Asset hub payload for data that needs to be preserved during migration.
	type AhPrePayload: Clone;

	/// Run some checks on asset hub before the migration and store intermediate payload.
	///
	/// The expected output should contain the data stored in asset hub before the migration.
	fn pre_check(rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload;

	/// Run some checks after the migration and use the intermediate payload.
	///
	/// The expected input should contain the data just transferred out of the relay chain, to allow
	/// the check that data has been correctly migrated to asset hub. It should also contain the
	/// data previously stored in asset hub, allowing for more complex logical checks on the
	/// migration outcome.
	fn post_check(rc_pre_payload: Self::RcPrePayload, ah_pre_payload: Self::AhPrePayload);
}

#[impl_trait_for_tuples::impl_for_tuples(24)]
impl AhMigrationCheck for Tuple {
	for_tuples! { type RcPrePayload = (#( Tuple::RcPrePayload ),* ); }
	for_tuples! { type AhPrePayload = (#( Tuple::AhPrePayload ),* ); }

	fn pre_check(rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload {
		(for_tuples! { #(
			// Copy&paste `frame_support::hypothetically` since we cannot use macros here
			frame_support::storage::transactional::with_transaction(|| -> sp_runtime::TransactionOutcome<Result<_, sp_runtime::DispatchError>> {
				sp_runtime::TransactionOutcome::Rollback(Ok(Tuple::pre_check(rc_pre_payload.Tuple)))
			}).expect("Always returning Ok")
		),* })
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, ah_pre_payload: Self::AhPrePayload) {
		(for_tuples! { #(
			// Copy&paste `frame_support::hypothetically` since we cannot use macros here
			frame_support::storage::transactional::with_transaction(|| -> sp_runtime::TransactionOutcome<Result<_, sp_runtime::DispatchError>> {
				sp_runtime::TransactionOutcome::Rollback(Ok(Tuple::post_check(rc_pre_payload.Tuple, ah_pre_payload.Tuple)))
			}).expect("Always returning Ok")
		),* });
	}
}
