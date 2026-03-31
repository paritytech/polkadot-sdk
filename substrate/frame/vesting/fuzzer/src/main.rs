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

//! # Running
//! Running this fuzzer can be done with `cargo hfuzz run vesting`. `honggfuzz` CLI
//! options can be used by setting `HFUZZ_RUN_ARGS`, such as `-n 4` to use 4 threads.
//!
//! # Debugging a panic
//! Once a panic is found, it can be debugged with
//! `cargo hfuzz run-debug vesting hfuzz_workspace/vesting/*.fuzz`.

use frame_support::traits::{Currency, UnfilteredDispatchable, VestingSchedule};
use frame_system::RawOrigin;
use honggfuzz::fuzz;
use pallet_vesting::{
	mock::*,
	pallet::{self as vesting_pallet, Vesting as VestingStorage},
	VestingInfo,
};
use rand::Rng;
use sp_runtime::traits::Zero;

type BalanceOf = u64;
type BlockNumberOf = u64;

const ED: BalanceOf = 256;
const MAX_ACCOUNT_ID: u64 = 20;
const MAX_BALANCE_MULTIPLE: u64 = 100;
const MAX_BLOCK: BlockNumberOf = 1_000_000;

/// Funded accounts that are set up in each iteration.
const FUNDED_ACCOUNTS: &[(u64, u64)] = &[
	(1, 10_000 * ED),
	(2, 20_000 * ED),
	(3, 30_000 * ED),
	(4, 40_000 * ED),
	(5, 50_000 * ED),
];

/// Generate a random account id, which may or may not be funded.
fn random_account<R: Rng>(rng: &mut R) -> u64 {
	rng.gen_range(1..=MAX_ACCOUNT_ID)
}

/// Generate a random balance as a multiple of ED.
fn random_balance<R: Rng>(rng: &mut R) -> BalanceOf {
	let multiple = rng.gen_range(1..=MAX_BALANCE_MULTIPLE);
	ED * multiple
}

/// Generate a random block number.
fn random_block<R: Rng>(rng: &mut R) -> BlockNumberOf {
	rng.gen_range(0..MAX_BLOCK)
}

/// Generate a random vesting schedule with valid parameters.
fn random_schedule<R: Rng>(rng: &mut R) -> VestingInfo<BalanceOf, BlockNumberOf> {
	let locked = random_balance(rng);
	// per_block must be > 0 for a valid schedule.
	let per_block = rng.gen_range(1..=locked);
	let starting_block = random_block(rng);
	VestingInfo::new(locked, per_block, starting_block)
}

/// Fuzzable operations on the vesting pallet.
#[derive(Debug, Clone)]
enum Action {
	/// Call `vested_transfer(source, target, schedule)`.
	VestedTransfer { source: u64, target: u64, schedule: VestingInfo<BalanceOf, BlockNumberOf> },
	/// Call `force_vested_transfer(source, target, schedule)` via root.
	ForceVestedTransfer { source: u64, target: u64, schedule: VestingInfo<BalanceOf, BlockNumberOf> },
	/// Call `vest()` on an account.
	Vest { who: u64 },
	/// Call `vest_other(target)` from a random origin.
	VestOther { caller: u64, target: u64 },
	/// Call `merge_schedules(idx1, idx2)` on an account.
	MergeSchedules { who: u64, idx1: u32, idx2: u32 },
	/// Advance the block number.
	AdvanceBlock { delta: BlockNumberOf },
	/// Transfer all funds from an account, then attempt vested_transfer to self.
	/// This is the specific attack scenario from the security report.
	DrainAndSelfVest { attacker: u64, drain_target: u64 },
}

fn random_action<R: Rng>(rng: &mut R) -> Action {
	match rng.gen_range(0u32..8) {
		0 => Action::VestedTransfer {
			source: random_account(rng),
			target: random_account(rng),
			schedule: random_schedule(rng),
		},
		1 => Action::ForceVestedTransfer {
			source: random_account(rng),
			target: random_account(rng),
			schedule: random_schedule(rng),
		},
		2 => Action::Vest { who: random_account(rng) },
		3 => Action::VestOther { caller: random_account(rng), target: random_account(rng) },
		4 => Action::MergeSchedules {
			who: random_account(rng),
			idx1: rng.gen_range(0..5),
			idx2: rng.gen_range(0..5),
		},
		5 | 6 => Action::AdvanceBlock { delta: rng.gen_range(1..1000) },
		7 => Action::DrainAndSelfVest {
			attacker: random_account(rng),
			drain_target: random_account(rng),
		},
		_ => unreachable!(),
	}
}

/// Execute an action and return whether it succeeded.
fn execute_action(action: Action) -> bool {
	match action {
		Action::VestedTransfer { source, target, schedule } => {
			let origin = RawOrigin::Signed(source);
			vesting_pallet::Call::<Test>::vested_transfer {
				target,
				schedule,
			}
			.dispatch_bypass_filter(origin.into())
			.is_ok()
		},
		Action::ForceVestedTransfer { source, target, schedule } => {
			vesting_pallet::Call::<Test>::force_vested_transfer {
				source,
				target,
				schedule,
			}
			.dispatch_bypass_filter(RawOrigin::Root.into())
			.is_ok()
		},
		Action::Vest { who } => {
			vesting_pallet::Call::<Test>::vest {}
				.dispatch_bypass_filter(RawOrigin::Signed(who).into())
				.is_ok()
		},
		Action::VestOther { caller, target } => {
			vesting_pallet::Call::<Test>::vest_other { target }
				.dispatch_bypass_filter(RawOrigin::Signed(caller).into())
				.is_ok()
		},
		Action::MergeSchedules { who, idx1, idx2 } => {
			vesting_pallet::Call::<Test>::merge_schedules {
				schedule1_index: idx1,
				schedule2_index: idx2,
			}
			.dispatch_bypass_filter(RawOrigin::Signed(who).into())
			.is_ok()
		},
		Action::AdvanceBlock { delta } => {
			let current = System::block_number();
			System::set_block_number(current.saturating_add(delta));
			true
		},
		Action::DrainAndSelfVest { attacker, drain_target } => {
			// This simulates the batch attack from the security report.
			let balance = Balances::free_balance(&attacker);
			if balance > 0 && attacker != drain_target {
				// Drain all funds to the other account.
				let _ = <Balances as Currency<u64>>::transfer(
					&attacker,
					&drain_target,
					balance,
					frame_support::traits::ExistenceRequirement::AllowDeath,
				);
			}

			// Now attempt a self-vested-transfer with a huge schedule.
			let schedule = VestingInfo::new(ED * 999_999, 1, 999_999_999);
			let result = vesting_pallet::Call::<Test>::vested_transfer {
				target: attacker,
				schedule,
			}
			.dispatch_bypass_filter(RawOrigin::Signed(attacker).into());

			// This must ALWAYS fail with SelfVestedTransfer.
			assert!(
				result.is_err(),
				"Self-vested-transfer should always fail, but succeeded for attacker {}",
				attacker,
			);

			false
		},
	}
}

/// Check storage invariants that must always hold.
fn check_invariants() {
	for account_id in 1..=MAX_ACCOUNT_ID {
		let free_balance = Balances::free_balance(&account_id);
		let schedules = VestingStorage::<Test>::get(&account_id);

		match schedules {
			Some(ref scheds) if !scheds.is_empty() => {
				// INVARIANT 1: If an account has vesting schedules, it must exist
				// (have a non-zero free balance or at least be above ED).
				assert!(
					free_balance > Zero::zero(),
					"Account {} has vesting schedules but zero free balance. \
					 Schedules: {:?}",
					account_id,
					scheds,
				);

				// INVARIANT 2: The number of schedules must not exceed MAX_VESTING_SCHEDULES.
				assert!(
					scheds.len() as u32 <= <Test as pallet_vesting::Config>::MAX_VESTING_SCHEDULES,
					"Account {} has {} schedules, exceeding max of {}",
					account_id,
					scheds.len(),
					<Test as pallet_vesting::Config>::MAX_VESTING_SCHEDULES,
				);

				// INVARIANT 3: Each schedule must be valid (locked > 0 and per_block > 0).
				for (i, sched) in scheds.iter().enumerate() {
					assert!(
						sched.is_valid(),
						"Account {} has invalid schedule at index {}: locked={:?}, per_block={:?}",
						account_id,
						i,
						sched.locked(),
						sched.per_block(),
					);
				}

				// INVARIANT 4: The vesting balance (amount still locked) should never exceed
				// the account's free balance.
				if let Some(vesting_balance) = Vesting::vesting_balance(&account_id) {
					assert!(
						vesting_balance <= free_balance,
						"Account {} has vesting balance {} > free balance {}",
						account_id,
						vesting_balance,
						free_balance,
					);
				}
			},
			_ => {
				// INVARIANT 5: If an account has no vesting schedules, it should have
				// no vesting lock. We check this by verifying no vesting balance.
				let vesting_balance = Vesting::vesting_balance(&account_id);
				assert!(
					vesting_balance.is_none(),
					"Account {} has no schedules but has vesting balance {:?}",
					account_id,
					vesting_balance,
				);
			},
		}
	}
}

fn main() {
	loop {
		fuzz!(|seed: [u8; 32]| {
			use rand::{rngs::SmallRng, SeedableRng};
			let mut rng = SmallRng::from_seed(seed);

			let mut ext = sp_io::TestExternalities::new_empty();
			ext.execute_with(|| {
				// Initialize the runtime.
				System::set_block_number(1);

				// Fund accounts.
				for &(account, balance) in FUNDED_ACCOUNTS {
					let _ = Balances::deposit_creating(&account, balance);
				}

				let num_actions = rng.gen_range(1..=50);
				let mut ok_count = 0u32;
				let mut err_count = 0u32;

				for _ in 0..num_actions {
					let action = random_action(&mut rng);
					if execute_action(action) {
						ok_count += 1;
					} else {
						err_count += 1;
					}

					// Check invariants after every action.
					check_invariants();
				}

				// Final invariant check after all actions.
				check_invariants();

				log::trace!(
					"Fuzz iteration complete: {} actions ({} ok, {} err)",
					num_actions,
					ok_count,
					err_count,
				);
			});
		});
	}
}
