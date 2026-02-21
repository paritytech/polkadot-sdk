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
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use frame_support::{
	defensive,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::{GetStorageVersion, OptionQuery, StorageVersion, ValueQuery},
	traits::{Currency, ExistenceRequirement::KeepAlive, ReservableCurrency, WithdrawReasons},
	weights::WeightMeter,
	Twox64Concat,
};

/// The log target for this pallet.
const LOG_TARGET: &str = "runtime::treasury";

pub(crate) type ProposalIndex = u32;

/// A spending proposal.
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, MaxEncodedLen, Debug, TypeInfo,
)]
pub struct Proposal<AccountId, Balance> {
	/// The account proposing it.
	pub proposer: AccountId,
	/// The (total) amount that should be paid if the proposal is accepted.
	pub value: Balance,
	/// The account to whom the payment should be made if the proposal is accepted.
	pub beneficiary: AccountId,
	/// The amount held on deposit (reserved) for making this proposal.
	pub bond: Balance,
}

type ProposalOf<T, I> = Proposal<<T as frame_system::Config>::AccountId, BalanceOf<T, I>>;
/// A list of proposal indexes for the approved proposals.
type ApprovalsList<MaxApprovals> = BoundedVec<ProposalIndex, MaxApprovals>;

/// Number of proposals that have been made.
#[frame_support::storage_alias]
type ProposalCount<T: Config<I>, I: 'static> =
	StorageValue<Pallet<T, I>, ProposalIndex, ValueQuery>;

/// Proposals that have been made.
#[frame_support::storage_alias]
pub(crate) type Proposals<T: Config<I>, I: 'static> = StorageMap<
	Pallet<T, I>,
	Twox64Concat,
	ProposalIndex,
	Proposal<<T as frame_system::Config>::AccountId, BalanceOf<T, I>>,
	OptionQuery,
>;

/// Proposal indices that have been approved but not yet awarded.
#[frame_support::storage_alias]
pub(crate) type Approvals<T: Config<I>, I: 'static, MaxApprovals> =
	StorageValue<Pallet<T, I>, ApprovalsList<MaxApprovals>, ValueQuery>;

/// Migration to cleanup unapproved proposals to return the bonds back to the proposers.
/// Proposals can no longer be created and the `Proposal` and `Approval` storage items will be
/// removed.
///
/// `Currency` corresponds to the module that formerly handled [`ReservableCurrency`] operations in
/// the pallet.
/// `MigrationHelper` is used to get the `MaxApprovals` from the runtime.
pub struct LazyMigrationV0ToV1<T, I, C>(PhantomData<(T, I, C)>);

#[allow(dead_code)]
type PositiveImbalanceOf<AccountId, C> = <C as Currency<AccountId>>::PositiveImbalance;

#[derive(Encode, Decode, MaxEncodedLen)]
pub enum MigrationStep<Proposal, Approvals> {
	SpendApproval(ProposalIndex, Approvals),
	SpendApprovalsFinished,
	RemoveProposal((ProposalIndex, Proposal)),
	MigrationCompleted,
}

const PALLET_MIGRATIONS_ID: &[u8; 15] = b"pallet-treasury";

impl<T, I: 'static, C> LazyMigrationV0ToV1<T, I, C>
where
	T: Config<I>,
	C: LazyMigrationV0ToV1Config<T, I>,
{
	/// Having the previous migration step, calculates the next migration step.
	pub(crate) fn next_step(
		cursor: Option<MigrationStep<ProposalOf<T, I>, ApprovalsList<C::MaxApprovals>>>,
	) -> MigrationStep<ProposalOf<T, I>, ApprovalsList<C::MaxApprovals>> {
		use MigrationStep::*;

		match cursor {
			None => {
				let approvals = Approvals::<T, I, C::MaxApprovals>::take();
				approvals
					.split_first()
					.map(move |(next, rest)| {
						SpendApproval(*next, BoundedVec::truncate_from(rest.to_vec()))
					})
					.unwrap_or(SpendApprovalsFinished)
			},
			Some(SpendApproval(_, approvals)) => approvals
				.split_first()
				.map(move |(next, rest)| {
					SpendApproval(*next, BoundedVec::truncate_from(rest.to_vec()))
				})
				.unwrap_or(SpendApprovalsFinished),
			Some(SpendApprovalsFinished) => Proposals::<T, I>::iter()
				.next()
				.map(RemoveProposal)
				.unwrap_or(MigrationCompleted),
			Some(RemoveProposal((index, _))) => {
				Proposals::<T, I>::iter_from(Proposals::<T, I>::hashed_key_for(index))
					.next()
					.map(RemoveProposal)
					.unwrap_or(MigrationCompleted)
			},
			Some(MigrationCompleted) => MigrationCompleted,
		}
	}

	/// Gracefully attempts to spend funds for an approved proposal.
	///
	/// If there are not enough funds in the treasury to provide this payment, it should just
	/// continue without any issues.
	pub(crate) fn step_spend_approval(proposal_index: &ProposalIndex) {
		// If a proposal with the mentioned approval doesn't exist, just go to the next potential
		// approval (or finish spending approvals).
		let Some(p) = Proposals::<T, I>::get(proposal_index) else {
			return;
		};
		let budget_remaining = Pallet::<T, I>::pot();

		// TODO: Should we handle this differently? Because otherwise we might spam the
		// state with events?
		Pallet::<T, I>::deposit_event(Event::Spending { budget_remaining });

		let has_funds = p.value <= budget_remaining;
		if !has_funds {
			return;
		}

		// Provide the allocation.
		let imbalance = C::Currency::deposit_creating(&p.beneficiary, p.value);

		#[allow(deprecated)]
		Pallet::<T, I>::deposit_event(Event::Awarded {
			proposal_index: *proposal_index,
			award: p.value,
			account: p.beneficiary,
		});

		let account_id = Pallet::<T, I>::account_id();
		// Must never be an error, but better to be safe.
		// proof: budget_remaining is account free balance minus ED;
		// Thus we can't spend more than account free balance minus ED;
		// Thus account is kept alive; qed;
		if let Err(problem) =
			C::Currency::settle(&account_id, imbalance, WithdrawReasons::TRANSFER, KeepAlive)
		{
			print("Inconsistent state - couldn't settle imbalance for funds spent by treasury");
			// Nothing else to do here.
			drop(problem);
		}

		Proposals::<T, I>::mutate_extant(proposal_index, |proposal| {
			proposal.value = Zero::zero();
		});

		// TODO: Should we handle this differently? Because otherwise we might spam the
		// state with events?
		Pallet::<T, I>::deposit_event(Event::Rollover {
			rollover_balance: budget_remaining - p.value,
		});
	}

	/// Clears a proposal.
	///
	/// If bond release fails (i.e. balance on hold is less than), then bond remains and we'll
	/// see what to do manually case by case.
	pub(crate) fn step_remove_proposal((proposal_index, p): &(ProposalIndex, ProposalOf<T, I>)) {
		let err_amount = C::Currency::unreserve(&p.proposer, p.bond);
		if err_amount.is_zero() {
			Proposals::<T, I>::remove(proposal_index);
			log::info!(
				target: LOG_TARGET,
				"Released bond amount of {:?} to proposer {:?}",
				p.bond,
				p.proposer,
			);
		} else {
			defensive!("err_amount is non zero for proposal {:?}", (proposal_index, err_amount));
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
	}
}

impl<T, I: 'static, C> SteppedMigration for LazyMigrationV0ToV1<T, I, C>
where
	T: Config<I>,
	C: LazyMigrationV0ToV1Config<T, I>,
{
	type Cursor = MigrationStep<ProposalOf<T, I>, ApprovalsList<C::MaxApprovals>>;
	type Identifier = MigrationId<15>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		if Pallet::<T, I>::on_chain_storage_version() != Self::id().version_from as u16 {
			return Ok(None);
		}

		let required = T::WeightInfo::migration_v1_next_step();
		if !meter.can_consume(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			// Calculates next step
			meter.consume(T::WeightInfo::migration_v1_next_step());
			let next = Self::next_step(cursor);

			let required = match next {
				MigrationStep::SpendApproval(_, _) => T::WeightInfo::migration_v1_spend_approval(),
				MigrationStep::SpendApprovalsFinished => Zero::zero(),
				MigrationStep::RemoveProposal(_) => T::WeightInfo::migration_v1_remove_proposal(),
				MigrationStep::MigrationCompleted => Zero::zero(),
			};

			if meter.remaining().any_lt(required) {
				log::info!(
					target: LOG_TARGET,
					"Not enough weight to continue migration. Remaining: {:?}",
					meter.remaining()
				);
				return Ok(Some(next));
			}

			meter.consume(required);
			match &next {
				MigrationStep::SpendApproval(i, _) => Self::step_spend_approval(i),
				// Intentionally left empty to simplify transition logic.
				MigrationStep::SpendApprovalsFinished => {},
				MigrationStep::RemoveProposal(p) => Self::step_remove_proposal(p),
				MigrationStep::MigrationCompleted => {
					log::info!(
						target: LOG_TARGET,
						"Migration for pallet-treasury finished.",
					);
					StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T, I>>();

					return Ok(None);
				},
			}

			cursor = Some(next);
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let value = (
			Proposals::<T, I>::iter_values().count() as u32,
			Approvals::<T, I, C::MaxApprovals>::get().len() as u32,
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
		let new_approvals_count = Approvals::<T, I, C::MaxApprovals>::get().len() as u32;

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
			new_approvals_count <= old_approvals_count,
			"Approvals after migration should be less or equal to old approvals"
		);
		Ok(())
	}
}

pub trait LazyMigrationV0ToV1Config<T: Config<I>, I: 'static = ()> {
	type MaxApprovals: Get<ProposalIndex> + 'static;
	type Currency: Currency<T::AccountId, Balance = BalanceOf<T, I>>
		+ ReservableCurrency<T::AccountId>;
}
