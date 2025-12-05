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
    inclusion::{QueueFootprinter, UmpQueueId},
    initializer::SessionChangeNotification,
    session_info,
    shared,
};
use codec::{Decode, Encode};
use core::{cmp, mem};
use frame_support::{
    pallet_prelude::*,
    traits::{EnsureOriginWithArg, EstimateNextSessionRotation},
    DefaultNoBound,
};
use scale_info::{Type, TypeInfo};
use sp_runtime::{
    traits::{AppVerify, One, Saturating},
    DispatchResult, SaturatedConversion,
};
use frame_system::pallet_prelude::*;
use polkadot_primitives::{
    vstaging::ApprovalStatistics,
    slashing::{DisputeProof, DisputesTimeSlot, PendingSlashes},
    CandidateHash, DisputeOffenceKind, SessionIndex, ValidatorId, ValidatorIndex,
    ValidatorSignature,
};


const LOG_TARGET: &str = "runtime::approvals_rewards";

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use polkadot_parachain_primitives::primitives::ValidationCodeHash;
    use polkadot_primitives::v9::ParaId;
    use super::*;

    use sp_runtime::transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    };
    use crate::disputes::WeightInfo;
    use crate::paras::CodeByHash;

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
    }

    /// Actual past code hash, indicated by the para id as well as the block number at which it
    /// became outdated.
    #[pallet::storage]
    pub(super) type ApprovalsTallies<T: Config> =
        StorageMap<_, Twox64Concat, (SessionIndex, ValidatorIndex), ValidationCodeHash>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> { }

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
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(1)]
        pub fn include_approvals_rewards_statistics(
            origin: OriginFor<T>,
            payload: ApprovalStatistics,
            signature: ValidatorSignature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            let current_session = shared::CurrentSessionIndex::<T>::get();
            let payload_session_index = payload.0;
            let payload_validator_index = payload.1;

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

            Ok(Pays::No.into())
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::include_approvals_rewards_statistics { payload, signature } => {
                    ValidTransaction::with_tag_prefix("ApprovalRewardsStatistics")
                        .priority(TransactionPriority::max_value())
                        .longevity(64_u64)
                        .and_provides((payload.0, payload.1, payload.2.clone()))
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

impl <T> Pallet<T>
where
    T: Config + frame_system::offchain::CreateBare<Call<T>>
{
    /// Submits a given PVF check statement with corresponding signature as an unsigned transaction
    /// into the memory pool. Ultimately, that disseminates the transaction across the network.
    ///
    /// This function expects an offchain context and cannot be callable from the on-chain logic.
    ///
    /// The signature assumed to pertain to `stmt`.
    ///
    pub(crate) fn submit_approval_statistics(
        payload: ApprovalStatistics,
        signature: ValidatorSignature,
    ) {
        use frame_system::offchain::{CreateBare, SubmitTransaction};
        let call = Call::include_approvals_rewards_statistics { payload, signature };

        let xt = <T as CreateBare<Call<T>>>::create_bare(call.into());

        if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
            log::error!(target: LOG_TARGET, "Error submitting pvf check statement: {:?}", e,);
        }
    }
}