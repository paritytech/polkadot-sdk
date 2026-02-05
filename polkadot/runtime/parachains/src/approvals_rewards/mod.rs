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

use crate::{
    configuration,
    initializer::SessionChangeNotification,
    session_info,
    shared,
};
use alloc::{vec, vec::Vec};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use polkadot_primitives::{
    vstaging::{ApprovalStatistics, ApprovalStatisticsTallyLine},
    SessionIndex, ValidatorIndex, ValidatorSignature,
    byzantine_threshold,
};
use sp_runtime::{
    traits::AppVerify,
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

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    /// Maps (SessionIndex, ValidatorIndex) to the approval statistics tallies submitted by that
    /// validator for that session. Pruned after the dispute period.
    #[pallet::storage]
    pub(super) type ApprovalsTallies<T: Config> =
        StorageMap<_, Twox64Concat, (SessionIndex, ValidatorIndex), Vec<ApprovalStatisticsTallyLine>>;

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

            let config = configuration::ActiveConfig::<T>::get();

            if payload_session_index > current_session {
                return Err(Error::<T>::ApprovalRewardsFutureSession.into())
            } else if payload_session_index < current_session.saturating_sub(config.dispute_period) {
                return Err(Error::<T>::ApprovalRewardsPassedSession.into())
            }

            let validator_public = if payload_session_index == current_session {
                let validators = shared::ActiveValidatorKeys::<T>::get();
                let validator_index = payload_validator_index.0 as usize;
                validators
                    .get(validator_index)
                    .ok_or(Error::<T>::ApprovalRewardsValidatorIndexOutOfBounds)?
                    .clone()
            } else {
                let session_info = match session_info::Sessions::<T>::get(payload_session_index) {
                    Some(s) => s,
                    None => return Err(Error::<T>::ApprovalRewardsUnknownSessionIndex.into()),
                };

                session_info.validators
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
            if ApprovalsTallies::<T>::contains_key(&approvals_key) {
                return Err(Error::<T>::ApprovalTalliesAlreadyStored.into())
            }

            ApprovalsTallies::<T>::insert(approvals_key, payload.2);
            Self::deposit_event(Event::ApprovalTalliesStored(approvals_key));

            // Return actual weight but don't charge fees (validators shouldn't pay)
            Ok(PostDispatchInfo {
                actual_weight: Some(<T as Config>::WeightInfo::include_approvals_rewards_statistics()),
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
                }
                _ => InvalidTransaction::Call.into(),
            }
        }

        fn pre_dispatch(_call: &Self::Call) -> Result<(), TransactionValidityError> {
            Ok(())
        }
    }
}

impl<T: Config> Pallet<T> {
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

        let config = configuration::ActiveConfig::<T>::get();

        // Check session bounds
        if payload_session_index > current_session {
            return Err(InvalidTransaction::Future.into());
        }

        if payload_session_index < current_session.saturating_sub(config.dispute_period) {
            return Err(InvalidTransaction::Stale.into());
        }

        // Get validator public key
        let validator_public = if payload_session_index == current_session {
            let validators = shared::ActiveValidatorKeys::<T>::get();
            let validator_index = payload_validator_index.0 as usize;
            validators
                .get(validator_index)
                .ok_or(InvalidTransaction::BadProof)?
                .clone()
        } else {
            let session_info = session_info::Sessions::<T>::get(payload_session_index)
                .ok_or(InvalidTransaction::Stale)?;

            session_info.validators
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
        let approvals_key = (payload_session_index, payload_validator_index);
        if ApprovalsTallies::<T>::contains_key(&approvals_key) {
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
		ApprovalsTallies::<T>::get((session_index, validator_index))
	}

    /// Handle an incoming session change.
    pub(crate) fn initializer_on_new_session(
        notification: &SessionChangeNotification<BlockNumberFor<T>>,
    ) {
        // Skip processing for genesis session (session 0) as there's no previous session data
        if notification.session_index == 0 {
            return;
        }

        let config = configuration::ActiveConfig::<T>::get();
        let current_session = notification.session_index;

        // Calculate the valid session range for processing
        // We look back from the previous session up to dispute_period sessions ago
        let oldest_valid_session = current_session.saturating_sub(config.dispute_period);
        let newest_session_to_check = current_session.saturating_sub(1);

        // Find the OLDEST session within the valid range that:
        // 1. Has enough tallies (>= byzantine threshold)
        // 2. Hasn't had medians calculated yet
        let mut target_session = None;

        for session_idx in oldest_valid_session..=newest_session_to_check {
            // Skip if medians already calculated for this session
            if AvailableApprovalsMedians::<T>::contains_key(session_idx) {
                continue;
            }

            // Get session info
            let session_info = match session_info::Sessions::<T>::get(session_idx) {
                Some(s) => s,
                None => continue,
            };

            let validators_len = session_info.validators.len();

            // Bound check to prevent excessive computation
            const MAX_VALIDATORS_FOR_MEDIAN: usize = 2000;
            if validators_len > MAX_VALIDATORS_FOR_MEDIAN {
                log::warn!(
                    target: LOG_TARGET,
                    "skipping session {}: validator count {} exceeds maximum {}",
                    session_idx,
                    validators_len,
                    MAX_VALIDATORS_FOR_MEDIAN
                );
                continue;
            }

            // Count how many validators submitted tallies for this session
            let mut tally_count = 0;
            for idx in 0..validators_len {
                let v_idx = ValidatorIndex(idx as u32);
                if ApprovalsTallies::<T>::contains_key((session_idx, v_idx)) {
                    tally_count += 1;
                }
            }

            // Check if we have enough tallies to calculate medians
            if tally_count >= byzantine_threshold(validators_len) {
                target_session = Some((session_idx, session_info, validators_len));
                break; // Found the oldest eligible session
            }
        }

        // If we found an eligible session, process it
        if let Some((session_idx, session_info, validators_len)) = target_session {
            // Collect all tallies for this session
            let mut rewards_matrix: Vec<Vec<ApprovalStatisticsTallyLine>> = vec![];
            let mut tally_keys_to_remove = Vec::new();

            for idx in 0..validators_len {
                let v_idx = ValidatorIndex(idx as u32);
                let key = (session_idx, v_idx);

                if let Some(tally) = ApprovalsTallies::<T>::get(key) {
                    rewards_matrix.push(tally);
                    tally_keys_to_remove.push(key);
                }
            }

            // Calculate medians for each validator
            let mut approval_usages_medians = Vec::new();
            for (v_idx, _) in session_info.validators.into_iter().enumerate() {
                let mut v: Vec<u32> = rewards_matrix.iter()
                    .filter_map(|at| at.get(v_idx).map(|line| line.approvals_usage))
                    .collect();

                // Use 0 as median if we don't have data for this validator
                let median = if v.is_empty() {
                    0
                } else {
                    v.sort();
                    // Calculate proper median: average of two middle elements for even-sized vectors
                    if v.len() % 2 == 0 {
                        // Even length: average of two middle elements
                        let mid1 = v[v.len() / 2 - 1];
                        let mid2 = v[v.len() / 2];
                        // Use saturating operations to prevent overflow
                        mid1.saturating_add(mid2) / 2
                    } else {
                        // Odd length: middle element
                        v[v.len() / 2]
                    }
                };

                approval_usages_medians.push(median);
            }

            // Store the calculated medians
            AvailableApprovalsMedians::<T>::insert(session_idx, approval_usages_medians);

            // Remove the tallies for this session now that we've processed them
            for key in tally_keys_to_remove {
                ApprovalsTallies::<T>::remove(key);
            }

            // Emit event for monitoring
            Self::deposit_event(Event::MediansCalculated(
                session_idx,
                validators_len as u32,
                rewards_matrix.len() as u32,
            ));

            log::info!(
                target: LOG_TARGET,
                "calculated medians for session {} with {} tallies from {} validators",
                session_idx,
                rewards_matrix.len(),
                validators_len
            );
        }

        // Prune old approval tallies that are outside the dispute period
        // Note: Tallies for sessions with calculated medians are already removed above
        // This handles leftover tallies from sessions that didn't reach byzantine threshold
        let min_session_to_keep = current_session.saturating_sub(config.dispute_period);

        // Pre-allocate with reasonable capacity and limit deletions per block
        const MAX_DELETIONS_PER_BLOCK: usize = 100;
        let mut drop_keys = Vec::with_capacity(MAX_DELETIONS_PER_BLOCK);

        // Collect keys to drop, but limit the number to prevent excessive computation
        for (session_idx, validator_idx) in ApprovalsTallies::<T>::iter_keys() {
            if session_idx < min_session_to_keep {
                drop_keys.push((session_idx, validator_idx));

                // Early exit if we've collected enough keys for this block
                if drop_keys.len() >= MAX_DELETIONS_PER_BLOCK {
                    break;
                }
            }
        }

        // Remove collected keys
        let removed_count = drop_keys.len();
        for key in drop_keys {
            ApprovalsTallies::<T>::remove(key);
        }

		// Emit event for monitoring if any tallies were pruned
		if removed_count > 0 {
			Self::deposit_event(Event::TalliesPruned(
				min_session_to_keep,
				removed_count as u32,
			));
		}

        // Log if we hit the limit (indicates more pruning needed in future blocks)
        if removed_count == MAX_DELETIONS_PER_BLOCK {
            log::debug!(
                target: LOG_TARGET,
                "pruned {} approval tallies (limit reached, more may remain)",
                removed_count
            );
        } else if removed_count > 0 {
            log::debug!(
                target: LOG_TARGET,
                "pruned {} approval tallies",
                removed_count
            );
        }
    }
}

impl<T> Pallet<T>
where
    T: Config + frame_system::offchain::CreateBare<Call<T>>
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