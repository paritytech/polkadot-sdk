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
#![cfg(feature = "runtime-benchmarks")]

use super::*;
use alloc::vec;
use frame_benchmarking::v2::*;
use polkadot_primitives::{PvfCheckStatement, ValidatorId, ValidatorIndex, ValidatorSignature};
use polkadot_primitives::{
    vstaging::ApprovalStatistics,
    SessionIndex,
};
use frame_system::{RawOrigin};
use sp_application_crypto::RuntimeAppPublic;
use crate::{configuration, shared};

// Constants for the benchmarking
const VALIDATOR_NUM: usize = 800;
const SESSION_INDEX: SessionIndex = 1;

fn initialize<T>()
where
    T: Config + shared::Config,
{
    // 0. generate a list of validators
    let validators = (0..VALIDATOR_NUM)
        .map(|_| <ValidatorId as RuntimeAppPublic>::generate_pair(None))
        .collect::<Vec<_>>();

    // 1. Make sure PVF pre-checking is enabled in the config.
    let config = configuration::ActiveConfig::<T>::get();
    configuration::Pallet::<T>::force_set_active_config(config.clone());

    // 2. initialize a new session with deterministic validator set.
    crate::shared::pallet::Pallet::<T>::set_active_validators_ascending(validators.clone());
    crate::shared::pallet::Pallet::<T>::set_session_index(SESSION_INDEX);
}

fn generate_approvals_tallies<T>() -> impl Iterator<Item = (ApprovalStatistics, ValidatorSignature)>
where
    T: Config + shared::Config
{
    let validators = shared::ActiveValidatorKeys::<T>::get();

    (0..validators.len()).map(move |validator_index| {
        let mut tally = vec![];
        let payload = ApprovalStatistics(SESSION_INDEX, ValidatorIndex(validator_index as u32), tally);
        let signature = validators[validator_index].sign(&payload.signing_payload()).unwrap();
        (payload, signature)
    })
}

#[benchmarks]
mod benchmarks {
    use super::*;
    #[benchmark]
    fn include_approvals_rewards_statistics() {
        initialize::<T>();
        let (payload, signature) = generate_approvals_tallies::<T>().next().unwrap();;

        #[block]
        {
            let _ =
                Pallet::<T>::include_approvals_rewards_statistics(RawOrigin::None.into(), payload, signature);
        }
    }

    impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	}
}