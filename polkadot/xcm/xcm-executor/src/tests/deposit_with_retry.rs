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

//! Unit tests for `deposit_assets_with_retry` failure handling.
//!
//! Behaviour under test:
//!
//! - A per-asset deposit failure on the retry pass propagates as `Err`. The surrounding
//!   `transactional_process` rolls back the whole instruction (so `self.holding` is restored to its
//!   pre-instruction state), and the leftover holding is then trapped by `post_process` via
//!   `Config::AssetTrap`. Funds are never silently lost.
//! - Successful deposits do not trigger a trap.

use xcm::prelude::*;

use super::mock::*;

const SENDER: [u8; 32] = [0; 32];
const RECIPIENT: [u8; 32] = [1; 32];

/// Helper: amount of fungible at `asset_id` held by `who`, or `0` if none.
fn fungible_balance(who: impl Into<Location>, asset_id: AssetId) -> u128 {
	let holding = assets(who);
	let mut total = 0u128;
	for a in holding.fungible_assets_iter() {
		if a.id == asset_id {
			if let Fungibility::Fungible(n) = a.fun {
				total = total.saturating_add(n);
			}
		}
	}
	total
}

/// Helper: amount of `(Here, _)` held by `who`.
fn here_balance(who: impl Into<Location>) -> u128 {
	fungible_balance(who, AssetId(Location::here()))
}

/// A single sub-ED deposit fails, the instruction is aborted, and the leftover holding is
/// trapped by `post_process` — funds are not lost.
#[test]
fn failed_deposit_aborts_instruction_and_post_process_traps_holding() {
	add_asset(SENDER, (Here, 1u128)); // 1 < ExistentialDeposit (=2 in mock)

	let xcm = Xcm::<TestCall>::builder_unsafe()
		.withdraw_asset((Here, 1u128))
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, weight) = instantiate_executor(SENDER, xcm.clone());

	// `bench_process` returns `Err` because the retry-pass deposit failure now bubbles up.
	let result = vm.bench_process(xcm);
	let err = result.expect_err("retry-pass deposit failure must bubble up");

	// Mirror what `XcmExecutor::execute` does between `process` and `post_process`: register
	// the instruction error so `post_process` produces `Outcome::Incomplete`.
	vm.set_error(Some((err.index, err.xcm_error)));

	let outcome = vm.bench_post_process(weight);
	assert!(
		matches!(outcome, Outcome::Incomplete { .. }),
		"expected Outcome::Incomplete, got {outcome:?}"
	);

	// Recipient never received anything.
	assert_eq!(here_balance(RECIPIENT), 0);

	// `post_process` trapped the holding (which `transactional_process` had restored after
	// the failed `DepositAsset`). The mock `TestAssetTrap` accumulates everything under
	// `TRAPPED_ASSETS`.
	assert_eq!(
		here_balance(TRAPPED_ASSETS),
		1,
		"undeposited assets must be trapped, not silently lost"
	);
}

/// A successful deposit doesn't generate a trap entry.
#[test]
fn successful_deposit_does_not_trigger_trap() {
	add_asset(SENDER, (Here, 5u128)); // ≥ ED

	let xcm = Xcm::<TestCall>::builder_unsafe()
		.withdraw_asset((Here, 5u128))
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, weight) = instantiate_executor(SENDER, xcm.clone());
	assert!(vm.bench_process(xcm).is_ok());
	let outcome = vm.bench_post_process(weight);
	assert!(matches!(outcome, Outcome::Complete { .. }));

	assert_eq!(here_balance(RECIPIENT), 5);
	assert!(
		assets(TRAPPED_ASSETS).is_empty(),
		"successful deposits must not generate trap entries"
	);
}

/// Within a single `DepositAsset` containing multiple assets, a single per-asset failure
/// aborts the whole instruction. The holding-level rollback restores the full
/// pre-instruction holding, and `post_process` then traps it — including assets that
/// would have deposited fine on their own.
///
/// (Note: storage-level effects of the sibling deposits that succeeded in the first pass
/// would be rolled back in production by `Config::TransactionalProcessor`. The mock here
/// uses a no-op `TestTransactionalProcessor`, so we only assert the executor-level
/// invariants — the holding restoration and the trap — not the storage state of the
/// recipient account.)
#[test]
fn partial_deposit_failure_aborts_instruction_and_traps_full_holding() {
	add_asset(SENDER, (Here, 5u128)); // ≥ ED on its own
	add_asset(SENDER, (Parent, 1u128)); // < ED — will fail on retry

	let xcm = Xcm::<TestCall>(vec![
		WithdrawAsset(vec![(Here, 5u128).into(), (Parent, 1u128).into()].into()),
		DepositAsset {
			assets: AssetFilter::Wild(WildAsset::All),
			beneficiary: Location::from(AccountId32 { id: RECIPIENT, network: None }),
		},
	]);

	let (mut vm, weight) = instantiate_executor(SENDER, xcm.clone());

	let err = vm.bench_process(xcm).expect_err(
		"any per-asset deposit failure on the retry pass must abort the whole DepositAsset",
	);
	vm.set_error(Some((err.index, err.xcm_error)));

	let outcome = vm.bench_post_process(weight);
	assert!(
		matches!(outcome, Outcome::Incomplete { .. }),
		"expected Outcome::Incomplete, got {outcome:?}"
	);

	// `post_process` trapped the holding that `transactional_process` restored from the
	// pre-instruction backup — both assets are present.
	assert_eq!(here_balance(TRAPPED_ASSETS), 5);
	assert_eq!(fungible_balance(TRAPPED_ASSETS, AssetId(Location::parent())), 1);
}
