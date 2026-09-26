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

//! Treasury pallet migrations.

use super::*;
use alloc::{collections::BTreeSet, vec::Vec};
use core::marker::PhantomData;
use frame_support::{
	defensive,
	traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
};

/// The log target for this pallet.
const LOG_TARGET: &str = "runtime::treasury";

mod cleanup_proposals {
	use super::*;

	/// Migration to cleanup unapproved proposals to return the bonds back to the proposers.
	/// Proposals can no longer be created and the `Proposal` storage item will be removed in the
	/// future.
	///
	/// `UnreserveWeight` returns `Weight` of `unreserve_balance` operation which is perfomed during
	/// this migration.
	pub struct Migration<T, I, UnreserveWeight>(PhantomData<(T, I, UnreserveWeight)>);

	impl<T: Config<I>, I: 'static, UnreserveWeight: Get<Weight>> OnRuntimeUpgrade
		for Migration<T, I, UnreserveWeight>
	{
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut approval_index = BTreeSet::new();
			#[allow(deprecated)]
			for approval in Approvals::<T, I>::get().iter() {
				approval_index.insert(*approval);
			}

			let mut proposals_processed = 0;
			#[allow(deprecated)]
			for (proposal_index, p) in Proposals::<T, I>::iter() {
				if !approval_index.contains(&proposal_index) {
					let err_amount = T::Currency::unreserve(&p.proposer, p.bond);
					if err_amount.is_zero() {
						Proposals::<T, I>::remove(proposal_index);
						log::info!(
							target: LOG_TARGET,
							"Released bond amount of {:?} to proposer {:?}",
							p.bond,
							p.proposer,
						);
					} else {
						defensive!(
							"err_amount is non zero for proposal {:?}",
							(proposal_index, err_amount)
						);
						Proposals::<T, I>::mutate_extant(proposal_index, |proposal| {
							proposal.value = err_amount;
						});
						log::info!(
							target: LOG_TARGET,
							"Released partial bond amount of {:?} to proposer {:?}",
							p.bond - err_amount,
							p.proposer,
						);
					}
					proposals_processed += 1;
				}
			}

			log::info!(
				target: LOG_TARGET,
				"Migration for pallet-treasury finished, released {} proposal bonds.",
				proposals_processed,
			);

			// calculate and return migration weights
			let approvals_read = 1;
			T::DbWeight::get().reads_writes(
				proposals_processed as u64 + approvals_read,
				proposals_processed as u64,
			) + UnreserveWeight::get() * proposals_processed
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let value = (
				Proposals::<T, I>::iter_values().count() as u32,
				Approvals::<T, I>::get().len() as u32,
			);
			log::info!(
				target: LOG_TARGET,
				"Proposals and Approvals count {:?}",
				value,
			);
			Ok(value.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (old_proposals_count, old_approvals_count) =
				<(u32, u32)>::decode(&mut &state[..]).expect("Known good");
			let new_proposals_count = Proposals::<T, I>::iter_values().count() as u32;
			let new_approvals_count = Approvals::<T, I>::get().len() as u32;

			log::info!(
				target: LOG_TARGET,
				"Proposals and Approvals count {:?}",
				(new_proposals_count, new_approvals_count),
			);

			ensure!(
				new_proposals_count <= old_proposals_count,
				"Proposals after migration should be less or equal to old proposals"
			);
			ensure!(
				new_approvals_count == old_approvals_count,
				"Approvals after migration should remain the same"
			);
			Ok(())
		}
	}
}

/// Migration to initialize the payout queue for existing spends (Solution 1.2).
///
/// Collects every live spend (dropping only `Pending`/`Failed` spends whose payout window has
/// already expired, and keeping in-flight `Attempted` ones), groups them by asset kind, sorts each
/// group by order key `max(now, valid_from)` then by index, and initializes `NextPayout` and
/// `PayoutQueue` for each asset kind.
mod migrate_to_ordered_payouts {
	use super::*;

	/// Migration to initialize the payout queue for existing spends.
	///
	/// Wrapped in [`MigrateToOrderedPayouts`](super::MigrateToOrderedPayouts) for the storage
	/// version check.
	pub struct UncheckedMigrateToOrderedPayouts<T, I = ()>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade
		for UncheckedMigrateToOrderedPayouts<T, I>
	{
		fn on_runtime_upgrade() -> Weight {
			log::info!(
				target: LOG_TARGET,
				"Running migration to initialize ordered payouts",
			);

			let now = T::BlockNumberProvider::current_block_number();
			let mut total_spends_read: u64 = 0;

			// Collect all pending/failed spends that haven't expired.
			let mut spends_vec: Vec<(T::AssetKind, SpendIndex, BlockNumberFor<T, I>)> = Vec::new();

			for (index, spend) in Spends::<T, I>::iter() {
				total_spends_read += 1;
				match spend.status {
					// Expired `Pending`/`Failed` spends are dropped, matching `check_status`, which
					// removes them once `now > expire_at`.
					PaymentState::Pending | PaymentState::Failed if spend.expire_at <= now => {
						log::debug!(
							target: LOG_TARGET,
							"Skipping expired spend {} (expire_at: {:?}, now: {:?})",
							index,
							spend.expire_at,
							now,
						);
					},
					// Every other live spend is ordered, including in-flight `Attempted` ones. An
					// `Attempted` spend whose payment later fails would otherwise be left out of
					// the payout order and could never be retried (lost); keeping it ordered
					// lets `check_status` resolve it and, on failure, promote/retry it. This
					// mirrors `check_status`, which never drops an `Attempted` spend on
					// expiration.
					_ => {
						spends_vec.push((spend.asset_kind, index, spend.valid_from));
					},
				}
			}

			// Group by encoded AssetKind
			let mut spends_by_asset: BTreeMap<
				Vec<u8>,
				(T::AssetKind, Vec<(SpendIndex, BlockNumberFor<T, I>)>),
			> = BTreeMap::new();

			for (asset_kind, index, valid_from) in spends_vec {
				let key = asset_kind.encode();
				spends_by_asset
					.entry(key)
					.or_insert_with(|| (asset_kind, Vec::new()))
					.1
					.push((index, valid_from));
			}

			let mut total_spends_processed = 0u32;
			let mut total_assets_processed = 0u32;

			// Process each AssetKind
			for (_, (asset_kind, mut spends)) in spends_by_asset {
				// Sort by the clamped order key (`max(now, valid_from)`), then by index for
				// deterministic ordering (consensus safety). Clamping is what keeps spends already
				// mature at migration ordered by approval (index) rather than by a back-dated
				// `valid_from`, matching the runtime insertion rule; not-yet-mature spends still
				// order by their maturity.
				spends.sort_by(|a, b| now.max(a.1).cmp(&now.max(b.1)).then_with(|| a.0.cmp(&b.0)));

				let spend_count = spends.len() as u32;
				total_spends_processed += spend_count;
				total_assets_processed += 1;

				log::info!(
					target: LOG_TARGET,
					"Processing asset kind with {} spends",
					spend_count,
				);

				// Build the payout queue (bounded by MaxQueuedSpends)
				let mut queue =
					BoundedVec::<(SpendIndex, BlockNumberFor<T, I>), T::MaxQueuedSpends>::default();
				let mut next_payout_set = false;

				for (index, valid_from) in spends {
					// The order key is clamped to `now` so that spends already mature at migration
					// are ordered by approval (index), while not-yet-mature spends keep their
					// maturity order. Clamping is monotonic, so the pre-sorted order is preserved.
					let order_key = now.max(valid_from);
					if !next_payout_set {
						// First spend (earliest-maturing) becomes NextPayout.
						let expire_at = order_key.saturating_add(T::OrderExpirationPeriod::get());
						NextPayout::<T, I>::insert(&asset_kind, (index, order_key, expire_at));
						next_payout_set = true;
						log::debug!(
							target: LOG_TARGET,
							"Set NextPayout for asset to {} with expiration at {:?}",
							index,
							expire_at,
						);
					} else if queue.len() < T::MaxQueuedSpends::get() as usize {
						// Add to queue
						if let Err(_) = queue.try_push((index, order_key)) {
							log::warn!(
								target: LOG_TARGET,
								"Failed to push spend {} to queue (queue full)",
								index
							);
						}
					} else {
						log::warn!(
							target: LOG_TARGET,
							"Payout queue is full, skipping spend {}",
							index
						);
					}
				}

				PayoutQueue::<T, I>::insert(&asset_kind, queue);
			}

			log::info!(
				target: LOG_TARGET,
				"Migration complete: processed {} spends across {} asset kinds",
				total_spends_processed,
				total_assets_processed,
			);

			let reads = total_spends_read + 1;
			let writes = total_assets_processed as u64 * 2;
			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let now = T::BlockNumberProvider::current_block_number();

			// Spends the migration will order, using the same inclusion rule as
			// `on_runtime_upgrade` (every live spend, plus in-flight `Attempted` ones regardless
			// of expiration), and the per-asset counts.
			let mut migrated: Vec<(T::AssetKind, SpendIndex)> = Vec::new();
			let mut per_asset: BTreeMap<Vec<u8>, u32> = BTreeMap::new();
			for (index, spend) in Spends::<T, I>::iter() {
				let include = match spend.status {
					PaymentState::Pending | PaymentState::Failed => spend.expire_at > now,
					PaymentState::Attempted { .. } => true,
				};
				if include {
					*per_asset.entry(spend.asset_kind.encode()).or_default() += 1;
					migrated.push((spend.asset_kind, index));
				}
			}

			// The migration places one spend at `NextPayout` and the rest in the queue (bounded by
			// `MaxQueuedSpends`). If an asset has more live spends than that capacity, the
			// migration would silently drop the overflow — fail here so the runtime raises
			// `MaxQueuedSpends` before deploying instead.
			let capacity = T::MaxQueuedSpends::get().saturating_add(1);
			for (_, count) in per_asset.iter() {
				ensure!(
					*count <= capacity,
					"MaxQueuedSpends too small: an asset has more live spends than NextPayout + queue can hold."
				);
			}

			log::info!(
				target: LOG_TARGET,
				"Pre-upgrade: {} spends to order across {} asset kinds",
				migrated.len(),
				per_asset.len(),
			);

			Ok(migrated.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let pre_migrated: Vec<(T::AssetKind, SpendIndex)> =
				Vec::decode(&mut &state[..]).expect("Known good");

			// Every spend the migration intended to order must land in exactly one of `NextPayout`
			// or `PayoutQueue` — nothing silently dropped (the `pre_upgrade` capacity check
			// guarantees this; verify it here).
			let mut post_ordered_count = 0usize;
			for (_, queue) in PayoutQueue::<T, I>::iter() {
				post_ordered_count += queue.len();
			}
			post_ordered_count += NextPayout::<T, I>::iter().count();

			log::info!(
				target: LOG_TARGET,
				"Post-upgrade: {} spends ordered (pre-upgrade had {} to order)",
				post_ordered_count,
				pre_migrated.len()
			);
			ensure!(
				post_ordered_count == pre_migrated.len(),
				"Migration dropped spends: ordered count does not match pre-upgrade count."
			);

			// Verify queue invariants for each asset kind.
			for (asset_kind, queue) in PayoutQueue::<T, I>::iter() {
				ensure!(
					queue.len() as u32 <= T::MaxQueuedSpends::get(),
					"Queue length exceeds MaxQueuedSpends"
				);

				// A queued spend is awaiting payout, so it may be `Pending`, `Failed`, or an
				// in-flight `Attempted` carried over by the migration.
				for (index, _) in queue.iter() {
					let spend = Spends::<T, I>::get(index)
						.ok_or(sp_runtime::TryRuntimeError::Other("Spend in queue not found"))?;
					ensure!(
						matches!(
							spend.status,
							PaymentState::Pending |
								PaymentState::Failed | PaymentState::Attempted { .. }
						),
						"Spend in queue has invalid status"
					);
					ensure!(spend.asset_kind == asset_kind, "Spend in queue has wrong asset kind");
				}

				// Verify NextPayout is NOT in queue
				if let Some((next_index, _, _)) = NextPayout::<T, I>::get(&asset_kind) {
					ensure!(
						!queue.iter().any(|(idx, _)| *idx == next_index),
						"NextPayout should not be in the queue"
					);
				}
			}

			Ok(())
		}
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use crate::{
			pallet::Spends,
			tests::{ExtBuilder, System, Test},
		};
		use frame_support::traits::{OnRuntimeUpgrade, StorageVersion, UncheckedOnRuntimeUpgrade};

		#[cfg(feature = "try-runtime")]
		use frame_support::assert_ok;

		/// Helper to directly insert a spend into storage
		fn insert_spend(
			index: SpendIndex,
			asset_kind: u32,
			amount: u64,
			beneficiary: u128,
			valid_from: u64,
			expire_at: u64,
			status: PaymentState<u64>,
		) {
			let spend = crate::SpendStatus {
				asset_kind,
				amount,
				beneficiary,
				valid_from,
				expire_at,
				status,
			};

			crate::pallet::Spends::<Test>::insert(index, spend);

			let current = crate::pallet::SpendCount::<Test>::get();

			if index >= current {
				crate::pallet::SpendCount::<Test>::put(index + 1);
			}
		}

		#[test]
		fn migration_runs_once() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);
				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Pending);

				StorageVersion::new(0).put::<crate::Pallet<Test>>();
				crate::migration::MigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				assert_eq!(StorageVersion::get::<crate::Pallet<Test>>(), 1);
				assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));

				// Re-run is a no-op: the version gate skips the migration.
				NextPayout::<Test>::remove(1u32);
				crate::migration::MigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				assert_eq!(StorageVersion::get::<crate::Pallet<Test>>(), 1);
				assert!(NextPayout::<Test>::get(1u32).is_none());
			});
		}

		#[test]
		fn migration_empty_state() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);
				assert_eq!(Spends::<Test>::iter().count(), 0);

				let weight = UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				assert!(weight.ref_time() == 0);
				assert!(NextPayout::<Test>::iter().next().is_none());
				assert!(PayoutQueue::<Test>::iter().next().is_none());
			});
		}

		#[test]
		fn migration_includes_attempted_spends() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);

				// An in-flight `Attempted` spend must stay in the payout order so a later payment
				// failure can be retried rather than lost.
				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Attempted { id: 123u64 });

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				// Only spend → becomes NextPayout (order key = max(now=100, valid_from=50) = 100).
				assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
				assert_eq!(PayoutQueue::<Test>::get(1u32).len(), 0);
			});
		}

		#[test]
		fn migration_skips_expired_spends() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);

				// expire_at (99) < now (100)
				insert_spend(0, 1, 100, 1000, 50, 99, PaymentState::Pending);

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				assert!(NextPayout::<Test>::get(1u32).is_none());
			});
		}

		#[test]
		fn migration_groups_by_asset() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);

				// Asset 1: 2 spends
				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Pending);
				insert_spend(1, 1, 200, 1001, 60, 200, PaymentState::Pending);
				// Asset 2: 1 spend
				insert_spend(2, 2, 300, 1002, 40, 200, PaymentState::Failed);

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				// Asset 1: First spend is NextPayout
				let (next_idx, _order_key, expire_at) = NextPayout::<Test>::get(1u32).unwrap();
				assert_eq!(next_idx, 0);
				assert_eq!(expire_at, 102); // 100 + OrderExpirationPeriod(2)

				// Asset 1 queue: spend 1 (mature at now=100, so order key clamps to 100)
				assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(1, 100)]);

				// Asset 2: Spend 2 is NextPayout
				assert_eq!(NextPayout::<Test>::get(2u32).map(|(idx, _, _)| idx), Some(2));
				assert_eq!(PayoutQueue::<Test>::get(2u32).len(), 0);
			});
		}

		#[test]
		fn migration_sorts_by_valid_from() {
			ExtBuilder::default().build().execute_with(|| {
				// Block is before every `valid_from`, so the order-key clamp is a no-op and spends
				// order by maturity (`valid_from`).
				System::set_block_number(40);

				// Insert out of order
				insert_spend(0, 1, 100, 1000, 100, 200, PaymentState::Pending); // latest
				insert_spend(1, 1, 100, 1001, 50, 200, PaymentState::Pending); // earliest
				insert_spend(2, 1, 100, 1002, 75, 200, PaymentState::Pending); // middle

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				// Sorted: 1 (50), 2 (75), 0 (100)
				assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
				assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(2, 75), (0, 100)]);
			});
		}

		#[test]
		fn migration_tie_breaks_by_index() {
			ExtBuilder::default().build().execute_with(|| {
				// Block before `valid_from`, so order keys equal `valid_from` and only the
				// index tie-break is exercised.
				System::set_block_number(40);

				// Same valid_from, different indices (inserted out of order)
				insert_spend(5, 1, 100, 1000, 50, 200, PaymentState::Pending);
				insert_spend(3, 1, 100, 1001, 50, 200, PaymentState::Pending);
				insert_spend(4, 1, 100, 1002, 50, 200, PaymentState::Pending);

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				// Sorted by index: 3, 4, 5
				assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(3));
				assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(4, 50), (5, 50)]);
			});
		}

		#[test]
		fn migration_respects_max_queue() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);

				// Create 105 spends (MaxQueuedSpends = 100)
				for i in 0..105u32 {
					insert_spend(
						i,
						1,
						100,
						1000 + i as u128,
						50 + i as u64,
						200,
						PaymentState::Pending,
					);
				}

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				// NextPayout is index 0
				assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));

				// Queue capped at 100
				let queue = PayoutQueue::<Test>::get(1u32);
				assert_eq!(queue.len(), 100);
				assert_eq!(queue[0].0, 1);
				assert_eq!(queue[99].0, 100);
			});
		}

		#[test]
		fn migration_mixed_statuses() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);

				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Pending);
				insert_spend(1, 1, 100, 1001, 51, 200, PaymentState::Failed);
				insert_spend(2, 1, 100, 1002, 52, 200, PaymentState::Attempted { id: 1 });
				insert_spend(3, 1, 100, 1003, 53, 99, PaymentState::Pending); // expired

				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				// Pending (0), Failed (1) and the in-flight Attempted (2) are ordered; the expired
				// Pending (3) is dropped. All are mature at `now = 100`, so order keys clamp to 100
				// and ties break by index.
				assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
				assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(1, 100), (2, 100)]);
			});
		}

		#[test]
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade_captures_state() {
			ExtBuilder::default().build().execute_with(|| {
				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Pending);
				insert_spend(1, 1, 100, 1001, 51, 200, PaymentState::Failed);
				insert_spend(2, 1, 100, 1002, 52, 200, PaymentState::Attempted { id: 1 });

				let state = UncheckedMigrateToOrderedPayouts::<Test>::pre_upgrade().unwrap();
				let decoded: Vec<(u32, u32)> = Vec::decode(&mut &state[..]).unwrap();

				// All three are ordered by the migration, including the in-flight Attempted spend.
				assert_eq!(decoded.len(), 3);
				assert!(decoded.contains(&(1, 0)));
				assert!(decoded.contains(&(1, 1)));
				assert!(decoded.contains(&(1, 2)));
			});
		}

		#[test]
		#[cfg(feature = "try-runtime")]
		fn post_upgrade_validates() {
			ExtBuilder::default().build().execute_with(|| {
				let pre_state = {
					let mut spends = Vec::new();
					spends.push((1u32, 0u32));
					spends.push((1u32, 1u32));
					spends.encode()
				};

				System::set_block_number(100);
				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Pending);
				insert_spend(1, 1, 100, 1001, 51, 200, PaymentState::Pending);
				UncheckedMigrateToOrderedPayouts::<Test>::on_runtime_upgrade();

				assert_ok!(UncheckedMigrateToOrderedPayouts::<Test>::post_upgrade(pre_state));
			});
		}

		#[test]
		#[cfg(feature = "try-runtime")]
		fn post_upgrade_detects_invalid() {
			ExtBuilder::default().build().execute_with(|| {
				System::set_block_number(100);

				// Create invalid state: NextPayout in queue
				insert_spend(0, 1, 100, 1000, 50, 200, PaymentState::Pending);
				NextPayout::<Test>::insert(1u32, (0u32, 100u64, 102u64));

				let bounded_vec: BoundedVec<(u32, u64), crate::tests::MaxQueuedSpends> =
					vec![(0u32, 50u64)].try_into().unwrap();
				crate::pallet::PayoutQueue::<Test>::insert(1u32, bounded_vec);

				let pre_state = vec![(1u32, 0u32)].encode();

				assert!(UncheckedMigrateToOrderedPayouts::<Test>::post_upgrade(pre_state).is_err());
			});
		}
	}
}

pub use cleanup_proposals::Migration as CleanupProposalsMigration;

/// Initialize the payout queue for existing spends, gated on storage version 0 and bumping it
/// to 1.
pub type MigrateToOrderedPayouts<T, I = ()> = frame_support::migrations::VersionedMigration<
	0,
	1,
	migrate_to_ordered_payouts::UncheckedMigrateToOrderedPayouts<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
