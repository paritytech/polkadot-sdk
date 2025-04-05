// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Benchmarking for the ambassador registration pallet.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as AmbassadorRegistration;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_runtime::traits::{Bounded, StaticLookup};
use frame_support::traits::Currency;

const SEED: u32 = 0;

benchmarks! {
    lock_dot {
        let caller: T::AccountId = whitelisted_caller();

        // Ensure caller has enough balance for the lock
        let lock_amount = MIN_LOCK_AMOUNT.into();
        T::Currency::make_free_balance_be(&caller, lock_amount * 2u32.into());
    }: _(RawOrigin::Signed(caller.clone()))
    verify {
        assert_eq!(
            AmbassadorRegistration::<T>::ambassador_registration_statuses(&caller),
            Some(RegistrationStatus::LockedOnly)
        );
    }

    verify_introduction {
        let caller: T::AccountId = whitelisted_caller();
        let verifier: T::AccountId = account("verifier", 0, SEED);

        // Setup verifier to have the required origin
        let origin = T::VerifierOrigin::try_successful_origin()
            .expect("Failed to create verifier origin");

        // Convert caller account to a lookup source
        let caller_lookup = T::Lookup::unlookup(caller.clone());
    }: {
        AmbassadorRegistration::<T>::verify_introduction(origin, caller_lookup)?
    }
    verify {
        assert_eq!(
            AmbassadorRegistration::<T>::ambassador_registration_statuses(&caller),
            Some(RegistrationStatus::IntroducedOnly)
        );
    }

    remove_registration {
        let caller: T::AccountId = whitelisted_caller();

        // Ensure caller has enough balance for the lock
        let lock_amount = MIN_LOCK_AMOUNT.into();
        T::Currency::make_free_balance_be(&caller, lock_amount * 2u32.into());

        // Lock DOT
        AmbassadorRegistration::<T>::lock_dot(RawOrigin::Signed(caller.clone()).into())?;

        // Verify introduction
        let origin = T::VerifierOrigin::try_successful_origin()
            .expect("Failed to create verifier origin");
        let caller_lookup = T::Lookup::unlookup(caller.clone());
        AmbassadorRegistration::<T>::verify_introduction(origin, caller_lookup)?;

        // Ensure registration is complete
        assert_eq!(
            AmbassadorRegistration::<T>::ambassador_registration_statuses(&caller),
            Some(RegistrationStatus::Complete)
        );

        // Setup admin origin
        let admin_origin = T::AdminOrigin::try_successful_origin()
            .expect("Failed to create admin origin");

        // Convert caller account to a lookup source for admin call
        let caller_lookup = T::Lookup::unlookup(caller.clone());
    }: {
        AmbassadorRegistration::<T>::remove_registration(admin_origin, caller_lookup)?
    }
    verify {
        assert_eq!(
            AmbassadorRegistration::<T>::ambassador_registration_statuses(&caller),
            None
        );
    }
}

impl_benchmark_test_suite!(
    AmbassadorRegistration,
    crate::mock::new_test_ext(),
    crate::mock::Test,
);
