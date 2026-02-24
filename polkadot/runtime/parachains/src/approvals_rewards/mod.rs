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

//! Approvals Rewards pallet.

use crate::{configuration, initializer::SessionChangeNotification, session_info, shared};
use alloc::{vec, vec::Vec};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use polkadot_primitives::{
	byzantine_threshold,
	vstaging::{ApprovalStatistics, ApprovalStatisticsTallyLine},
	SessionIndex, SessionInfo, ValidatorIndex, ValidatorSignature,
};
use sp_runtime::{
	traits::{AppVerify, One},
	Saturating,
};

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "runtime::approvals_rewards";

/// Transaction longevity for approval statistics submissions (in blocks)
const APPROVAL_STATS_LONGEVITY: u64 = 64;

pub use pallet::*;

pub trait WeightInfo {
	fn include_approvals_rewards_statistics() -> Weight;
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn include_approvals_rewards_statistics() -> Weight {
		// This special value is to distinguish from the finalizing variants above in tests.
		Weight::MAX - Weight::from_parts(1, 1)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::dispatch::PostDispatchInfo;
	use polkadot_primitives::vstaging::ApprovalStatisticsTallyLine;
	use sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		ValidTransaction,
	};

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ configuration::Config
		+ shared::Config
		+ session_info::Config
		+ frame_system::offchain::CreateBare<Call<Self>>
	{
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum number of tallies that can be submitted in a single payload.
		/// Should be set to the maximum expected validator count to prevent DoS attacks.
		#[pallet::constant]
		type MaxTalliesPerSubmission: Get<u32>;

		#[pallet::constant]
		type ApprovalStatsWindowSize: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Block number when the current session started
	/// used to enforce the tally submission window
	#[pallet::storage]
	pub(super) type CurrentSessionStartBlock<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Sessions that did not receive enough tallies within the
	/// submission window.
	/// ApprovalsTallies entries for these sessions are intentionally kept.
	#[pallet::storage]
	pub(super) type IncompleteSessions<T: Config> = StorageMap<_, Twox64Concat, SessionIndex, ()>;

	/// Maps (SessionIndex, ValidatorIndex) to the approval statistics tallies submitted by that
	/// validator for that session. Pruned after the dispute period.
	/// Uses StorageDoubleMap so all entries for a session can be removed via `clear_prefix`.
	#[pallet::storage]
	pub(super) type ApprovalsTallies<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		SessionIndex,
		Twox64Concat,
		ValidatorIndex,
		Vec<ApprovalStatisticsTallyLine>,
	>;

	/// Stores the calculated median approval usage values for each validator in a session.
	/// Only populated when at least byzantine threshold validators submitted tallies.
	#[pallet::storage]
	pub(super) type AvailableApprovalsMedians<T: Config> =
		StorageMap<_, Twox64Concat, SessionIndex, Vec<u32>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Approval tallies successfully stored
		/// [session_index, validator_index]
		ApprovalTalliesStored((SessionIndex, ValidatorIndex)),
		/// Submission rejected due to excessive tallies
		/// [session_index, validator_index, tallies_count, max_allowed]
		TooManyTalliesRejected(SessionIndex, ValidatorIndex, u32, u32),
		/// Approval medians calculated for a session
		/// [session_index, validator_count, submissions_received]
		MediansCalculated(SessionIndex, u32, u32),
		/// Old approval tallies pruned
		/// [min_session_kept, count_pruned]
		TalliesPruned(SessionIndex, u32),
		/// Submission window closed without reaching BFT threshold.
		/// [session index, tallies received, threshold required]
		SessionMarkedIncomplete(SessionIndex, u32, u32),
		/// Window closed and medians were calculated
		SubmissionWindowClosed(SessionIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The approval rewards payload has a future session index.
		ApprovalRewardsFutureSession,

		/// The approval rewards payloads has an already pruned session index.
		ApprovalRewardsPassedSession,

		/// The session index has no available data and is not the current session index
		ApprovalRewardsUnknownSessionIndex,

		/// Validator index is not in the session validators bounds
		ApprovalRewardsValidatorIndexOutOfBounds,

		/// Invalid signed payload
		ApprovalRewardsInvalidSignature,

		/// The validator already have submitted a tally for that session
		ApprovalTalliesAlreadyStored,

		/// Too many tallies in the submission payload
		TooManyTallies,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let session_start = CurrentSessionStartBlock::<T>::get();
			let window_end = session_start.saturating_add(T::ApprovalStatsWindowSize::get().into());

			if n == window_end.saturating_add(One::one()) {
				Self::settle_submission_window();
			}

			Weight::zero()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::include_approvals_rewards_statistics())]
		pub fn include_approvals_rewards_statistics(
			origin: OriginFor<T>,
			payload: ApprovalStatistics,
			signature: ValidatorSignature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			let payload_session_index = payload.0;
			let payload_validator_index = payload.1;
			let current_session = shared::CurrentSessionIndex::<T>::get();

			// Validate payload size to prevent DoS attacks
			let max_tallies = T::MaxTalliesPerSubmission::get();
			if payload.2.len() > max_tallies as usize {
				// Emit event for monitoring before rejecting
				Self::deposit_event(Event::TooManyTalliesRejected(
					payload_session_index,
					payload_validator_index,
					payload.2.len() as u32,
					max_tallies,
				));
				return Err(Error::<T>::TooManyTallies.into());
			}

			// Only accept tallies for the immediately previous session
			let prev_session = current_session.saturating_sub(1);
			if payload_session_index != prev_session {
				return Err(Error::<T>::ApprovalRewardsFutureSession.into());
			}

			// Only accept within the submission window
			let session_start = CurrentSessionStartBlock::<T>::get();
			let window_end = session_start.saturating_add(T::ApprovalStatsWindowSize::get().into());
			let current_block = frame_system::Pallet::<T>::block_number();
			if current_block > window_end {
				return Err(Error::<T>::ApprovalRewardsPassedSession.into());
			}

			let validator_public = {
				let session_info = match session_info::Sessions::<T>::get(payload_session_index) {
					Some(s) => s,
					None => return Err(Error::<T>::ApprovalRewardsUnknownSessionIndex.into()),
				};

				session_info
					.validators
					.get(payload_validator_index)
					.ok_or(Error::<T>::ApprovalRewardsValidatorIndexOutOfBounds)?
					.clone()
			};

			let signing_payload = payload.signing_payload();
			ensure!(
				signature.verify(&signing_payload[..], &validator_public),
				Error::<T>::ApprovalRewardsInvalidSignature,
			);

			let approvals_key = (payload_session_index, payload_validator_index);

			// Ensure that it is a fresh session tally.
			if ApprovalsTallies::<T>::contains_key(payload_session_index, payload_validator_index) {
				return Err(Error::<T>::ApprovalTalliesAlreadyStored.into());
			}

			ApprovalsTallies::<T>::insert(
				payload_session_index,
				payload_validator_index,
				payload.2,
			);
			Self::deposit_event(Event::ApprovalTalliesStored(approvals_key));

			// Return actual weight but don't charge fees (validators shouldn't pay)
			Ok(PostDispatchInfo {
				actual_weight: Some(
					<T as Config>::WeightInfo::include_approvals_rewards_statistics(),
				),
				pays_fee: Pays::No,
			})
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::include_approvals_rewards_statistics { payload, signature } => {
					// Validate the payload and signature
					Self::validate_approval_statistics(payload, signature)?;

					ValidTransaction::with_tag_prefix("ApprovalRewardsStatistics")
						.priority(TransactionPriority::max_value())
						.longevity(APPROVAL_STATS_LONGEVITY)
						.and_provides(vec![(b"ApprovalStats", payload.0, payload.1).encode()])
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}

		fn pre_dispatch(_call: &Self::Call) -> Result<(), TransactionValidityError> {
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn is_session_settled(session_index: SessionIndex) -> bool {
		AvailableApprovalsMedians::<T>::contains_key(session_index)
			|| IncompleteSessions::<T>::contains_key(session_index)
	}

	fn settle_submission_window() {
		let current_session = shared::CurrentSessionIndex::<T>::get();
		let prev_session = current_session.saturating_sub(1);

		if Self::is_session_settled(prev_session) {
			return;
		}

		let session_info = match session_info::Sessions::<T>::get(prev_session) {
			Some(s) => s,
			None => return,
		};

		let validators_len = session_info.validators.len();
		let tally_count = (0..validators_len)
			.filter(|&idx| {
				ApprovalsTallies::<T>::contains_key(prev_session, ValidatorIndex(idx as u32))
			})
			.count();

		let threshold = byzantine_threshold(validators_len);
		if tally_count >= threshold {
			Self::calculate_medians_and_reward(prev_session, &session_info);
			Self::deposit_event(Event::SubmissionWindowClosed(prev_session));
		} else {
			IncompleteSessions::<T>::insert(prev_session, ());
			Self::deposit_event(Event::SessionMarkedIncomplete(
				prev_session,
				tally_count as u32,
				threshold as u32,
			));
		}
	}

	/// Calculates per-validator median approval usage from submitted tallies for a session
	/// and stores the result in `AvailableApprovalsMedians`. Tallies are removed after
	/// processing. Era point rewards will be applied here once `RewardsReporter` is wired.
	fn calculate_medians_and_reward(session_index: SessionIndex, session_info: &SessionInfo) {
		let validators_len = session_info.validators.len();

		// Collect all tallies submitted for this session
		let mut rewards_matrix: Vec<Vec<ApprovalStatisticsTallyLine>> = vec![];

		for idx in 0..validators_len {
			let v_idx = ValidatorIndex(idx as u32);
			if let Some(tally) = ApprovalsTallies::<T>::get(session_index, v_idx) {
				rewards_matrix.push(tally);
			}
		}

		// Calculate the median approval usage for each validator across all reporter PoVs
		let mut approval_usages_medians = Vec::new();
		for v_idx in 0..validators_len {
			let mut v: Vec<u32> = rewards_matrix
				.iter()
				.filter_map(|at| at.get(v_idx).map(|line| line.approvals_usage))
				.collect();

			let median = if v.is_empty() {
				0
			} else {
				v.sort_unstable();
				if v.len() % 2 == 0 {
					let mid1 = v[v.len() / 2 - 1];
					let mid2 = v[v.len() / 2];
					mid1.saturating_add(mid2) / 2
				} else {
					v[v.len() / 2]
				}
			};

			approval_usages_medians.push(median);
		}

		// Store the calculated medians
		AvailableApprovalsMedians::<T>::insert(session_index, &approval_usages_medians);

		// Remove all tallies for this session in one prefix sweep
		let _ = ApprovalsTallies::<T>::clear_prefix(session_index, u32::MAX, None);

		Self::deposit_event(Event::MediansCalculated(
			session_index,
			validators_len as u32,
			rewards_matrix.len() as u32,
		));

		log::info!(
			target: LOG_TARGET,
			"calculated medians for session {} with {} tallies from {} validators",
			session_index,
			rewards_matrix.len(),
			validators_len,
		);

		// TODO: apply era points once RewardsReporter is wired into Config
		// let rewards: Vec<(ValidatorId, u32)> = approval_usages_medians
		//     .iter()
		//     .enumerate()
		//     .filter_map(|(idx, &median)| {
		//         let account = account_keys.get(idx)?;
		//         Some((account.clone(), median.saturating_mul(APPROVALS_POINTS)))
		//     })
		//     .collect();
		// T::RewardsReporter::reward_by_ids(rewards);
	}

	/// Validates approval statistics payload and signature.
	/// Returns Ok(()) if valid, or InvalidTransaction error if not.
	fn validate_approval_statistics(
		payload: &ApprovalStatistics,
		signature: &ValidatorSignature,
	) -> Result<(), TransactionValidityError> {
		let current_session = shared::CurrentSessionIndex::<T>::get();
		let payload_session_index = payload.0;
		let payload_validator_index = payload.1;

		// Check payload size
		if payload.2.len() > T::MaxTalliesPerSubmission::get() as usize {
			return Err(InvalidTransaction::ExhaustsResources.into());
		}

		// Only accept tallies for the immediately previous session
		let prev_session_index = current_session.saturating_sub(1);
		if payload_session_index != prev_session_index {
			return Err(InvalidTransaction::Stale.into());
		}

		// Only accept within the submission window
		let session_start_block = CurrentSessionStartBlock::<T>::get();
		let window_end =
			session_start_block.saturating_add(T::ApprovalStatsWindowSize::get().into());
		let current_block = frame_system::Pallet::<T>::block_number();

		if current_block < session_start_block || current_block > window_end {
			return Err(InvalidTransaction::Stale.into());
		}

		// Get validator public key
		let validator_public = {
			let session_info = session_info::Sessions::<T>::get(payload_session_index)
				.ok_or(InvalidTransaction::Stale)?;

			session_info
				.validators
				.get(payload_validator_index)
				.ok_or(InvalidTransaction::BadProof)?
				.clone()
		};

		// Verify signature
		let signing_payload = payload.signing_payload();
		if !signature.verify(&signing_payload[..], &validator_public) {
			return Err(InvalidTransaction::BadProof.into());
		}

		// Check for duplicate submission
		if ApprovalsTallies::<T>::contains_key(payload_session_index, payload_validator_index) {
			return Err(InvalidTransaction::Stale.into());
		}

		Ok(())
	}

	/// Returns the calculated median approval usage values for a given session.
	/// Returns None if medians haven't been calculated yet (not enough submissions).
	pub fn get_approval_medians(session_index: SessionIndex) -> Option<Vec<u32>> {
		AvailableApprovalsMedians::<T>::get(session_index)
	}

	/// Returns the approval statistics tallies submitted by a specific validator
	/// for a given session. Returns None if the validator hasn't submitted tallies.
	pub fn get_validator_tallies(
		session_index: SessionIndex,
		validator_index: ValidatorIndex,
	) -> Option<Vec<ApprovalStatisticsTallyLine>> {
		ApprovalsTallies::<T>::get(session_index, validator_index)
	}

	/// Handle an incoming session change.
	pub(crate) fn initializer_on_new_session(
		notification: &SessionChangeNotification<BlockNumberFor<T>>,
	) {
		// Skip processing for genesis session (session 0) as there's no previous session data
		if notification.session_index == 0 {
			return;
		}

		let current_session = notification.session_index;

		// Edge case guard: if the previous session was not settled (i.e. the session was
		// shorter than the window size), force-settle it now before overwriting
		// CurrentSessionStartBlock.
		let prev_session = current_session.saturating_sub(1);
		if !Self::is_session_settled(prev_session) {
			Self::settle_submission_window();
		}

		// Record when this session started, opening the submission window for prev_session
		// tallies. This must happen AFTER the force-settle above.
		let current_block = frame_system::Pallet::<T>::block_number();
		CurrentSessionStartBlock::<T>::put(current_block);

		let config = configuration::ActiveConfig::<T>::get();
		let min_session_to_keep = current_session.saturating_sub(config.dispute_period);

		// Prune incomplete sessions outside the dispute period. IncompleteSessions is the
		// source of truth for sessions with remaining tallies — sessions that reached
		// consensus already had their tallies removed in calculate_medians_and_reward.
		let expired: Vec<SessionIndex> = IncompleteSessions::<T>::iter_keys()
			.filter(|&session_idx| session_idx < min_session_to_keep)
			.collect();

		let pruned_count = expired.len();
		for session_idx in expired {
			let _ = ApprovalsTallies::<T>::clear_prefix(session_idx, u32::MAX, None);
			IncompleteSessions::<T>::remove(session_idx);
		}

		if pruned_count > 0 {
			Self::deposit_event(Event::TalliesPruned(min_session_to_keep, pruned_count as u32));
			log::debug!(target: LOG_TARGET, "pruned {} incomplete sessions", pruned_count);
		}
	}
}

impl<T> Pallet<T>
where
	T: Config + frame_system::offchain::CreateBare<Call<T>>,
{
	/// Submits approval statistics with corresponding signature as an unsigned transaction
	/// into the memory pool for dissemination across the network.
	///
	/// This function expects an offchain context and cannot be called from on-chain logic.
	///
	/// # Arguments
	/// * `payload` - The approval statistics data
	/// * `signature` - Validator signature over the payload
	pub(crate) fn submit_approval_statistics(
		payload: ApprovalStatistics,
		signature: ValidatorSignature,
	) {
		use frame_system::offchain::{CreateBare, SubmitTransaction};
		let call = Call::include_approvals_rewards_statistics { payload, signature };

		let xt = <T as CreateBare<Call<T>>>::create_bare(call.into());

		if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
			log::error!(target: LOG_TARGET, "Error submitting approval statistics: {:?}", e);
		}
	}
}
