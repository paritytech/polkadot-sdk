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

//! This pallet is designed to go into a source chain and destination chain to migrate data. The
//! design motivations are:
//!
//! - Call some function on the source chain that executes some migration (clearing state,
//!   forwarding an XCM program).
//! - Call some function (probably from an XCM program) on the destination chain.
//! - Avoid cluttering the source pallet with new dispatchables that are unrelated to its
//!   functionality and only used for migration.
//!
//! After the migration is complete, the pallet may be removed from both chains' runtimes as well as
//! the `polkadot-runtime-common` crate.

use frame_support::{dispatch::DispatchResult, traits::Currency, weights::Weight};
pub use pallet::*;
use pallet_identity;
use sp_core::Get;

#[cfg(feature = "runtime-benchmarks")]
use frame_benchmarking::{account, impl_benchmark_test_suite, v2::*, BenchmarkError};

pub trait WeightInfo {
	fn reap_identity(r: u32, s: u32) -> Weight;
	fn poke_deposit() -> Weight;
}

impl WeightInfo for () {
	fn reap_identity(_r: u32, _s: u32) -> Weight {
		Weight::MAX
	}
	fn poke_deposit() -> Weight {
		Weight::MAX
	}
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn reap_identity(_r: u32, _s: u32) -> Weight {
		Weight::zero()
	}
	fn poke_deposit() -> Weight {
		Weight::zero()
	}
}

// Must use the same `Balance` as `T`'s Identity pallet to handle deposits.
type BalanceOf<T> = <<T as pallet_identity::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::{DispatchResultWithPostInfo, PostDispatchInfo},
		pallet_prelude::*,
		traits::EnsureOrigin,
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_identity::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The origin that can reap identities. Expected to be `EnsureSigned<AccountId>` on the
		/// source chain such that anyone can all this function.
		type Reaper: EnsureOrigin<Self::RuntimeOrigin>;

		/// A handler for what to do when an identity is reaped.
		type ReapIdentityHandler: OnReapIdentity<Self::AccountId>;

		/// Weight information for the extrinsics in the pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The identity and all sub accounts were reaped for `who`.
		IdentityReaped { who: T::AccountId },
		/// The deposits held for `who` were updated. `identity` is the new deposit held for
		/// identity info, and `subs` is the new deposit held for the sub-accounts.
		DepositUpdated { who: T::AccountId, identity: BalanceOf<T>, subs: BalanceOf<T> },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reap the `IdentityInfo` of `who` from the Identity pallet of `T`, unreserving any
		/// deposits held and removing storage items associated with `who`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::reap_identity(
				T::MaxRegistrars::get(),
				T::MaxSubAccounts::get()
		))]
		pub fn reap_identity(
			origin: OriginFor<T>,
			who: T::AccountId,
		) -> DispatchResultWithPostInfo {
			T::Reaper::ensure_origin(origin)?;
			// - number of registrars (required to calculate weight)
			// - byte size of `IdentityInfo` (required to calculate remote deposit)
			// - number of sub accounts (required to calculate both weight and remote deposit)
			let (registrars, bytes, subs) = pallet_identity::Pallet::<T>::reap_identity(&who)?;
			T::ReapIdentityHandler::on_reap_identity(&who, bytes, subs)?;
			Self::deposit_event(Event::IdentityReaped { who });
			let post = PostDispatchInfo {
				actual_weight: Some(<T as pallet::Config>::WeightInfo::reap_identity(
					registrars, subs,
				)),
				pays_fee: Pays::No,
			};
			Ok(post)
		}

		/// Update the deposit of `who`. Meant to be called by the system with an XCM `Transact`
		/// Instruction.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::poke_deposit())]
		pub fn poke_deposit(origin: OriginFor<T>, who: T::AccountId) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let (id_deposit, subs_deposit) = pallet_identity::Pallet::<T>::poke_deposit(&who)?;
			Self::deposit_event(Event::DepositUpdated {
				who,
				identity: id_deposit,
				subs: subs_deposit,
			});
			Ok(Pays::No.into())
		}
	}
}

/// Trait to handle reaping identity from state.
pub trait OnReapIdentity<AccountId> {
	/// What to do when an identity is reaped. For example, the implementation could send an XCM
	/// program to another chain. Concretely, a type implementing this trait in the Polkadot
	/// runtime would teleport enough DOT to the People Chain to cover the Identity deposit there.
	///
	/// This could also directly include `Transact { poke_deposit(..), ..}`.
	///
	/// Inputs
	/// - `who`: Whose identity was reaped.
	/// - `bytes`: The byte size of `IdentityInfo`.
	/// - `subs`: The number of sub-accounts they had.
	fn on_reap_identity(who: &AccountId, bytes: u32, subs: u32) -> DispatchResult;
}

impl<AccountId> OnReapIdentity<AccountId> for () {
	fn on_reap_identity(_who: &AccountId, _bytes: u32, _subs: u32) -> DispatchResult {
		Ok(())
	}
}

#[cfg(feature = "runtime-benchmarks")]
#[benchmarks]
mod benchmarks {
	use super::*;
	use frame_support::traits::EnsureOrigin;
	use frame_system::RawOrigin;
	use pallet_identity::{Data, IdentityInformationProvider, Judgement, Pallet as Identity};
	use parity_scale_codec::Encode;
	use sp_runtime::{
		traits::{Bounded, Hash, StaticLookup},
		Saturating,
	};
	use sp_std::{boxed::Box, vec::Vec, *};

	const SEED: u32 = 0;

	fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
		let events = frame_system::Pallet::<T>::events();
		let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
		let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
		assert_eq!(event, &system_event);
	}

	#[benchmark]
	fn reap_identity(
		r: Linear<0, { T::MaxRegistrars::get() }>,
		s: Linear<0, { T::MaxSubAccounts::get() }>,
	) -> Result<(), BenchmarkError> {
		// set up target
		let target: T::AccountId = account("target", 0, SEED);
		let target_origin =
			<T as frame_system::Config>::RuntimeOrigin::from(RawOrigin::Signed(target.clone()));
		let target_lookup = T::Lookup::unlookup(target.clone());
		let _ = T::Currency::make_free_balance_be(&target, BalanceOf::<T>::max_value());

		// set identity
		let info = <T as pallet_identity::Config>::IdentityInformation::create_identity_info();
		Identity::<T>::set_identity(
			RawOrigin::Signed(target.clone()).into(),
			Box::new(info.clone()),
		)?;

		// create and set subs
		let mut subs = Vec::new();
		let data = Data::Raw(vec![0; 32].try_into().unwrap());
		for ii in 0..s {
			let sub_account = account("sub", ii, SEED);
			subs.push((sub_account, data.clone()));
		}
		Identity::<T>::set_subs(target_origin.clone(), subs.clone())?;

		// add registrars and provide judgements
		let registrar_origin = T::RegistrarOrigin::try_successful_origin()
			.expect("RegistrarOrigin has no successful origin required for the benchmark");
		for ii in 0..r {
			// registrar account
			let registrar: T::AccountId = account("registrar", ii, SEED);
			let registrar_lookup = T::Lookup::unlookup(registrar.clone());
			let _ = <T as pallet_identity::Config>::Currency::make_free_balance_be(
				&registrar,
				<T as pallet_identity::Config>::Currency::minimum_balance(),
			);

			// add registrar
			Identity::<T>::add_registrar(registrar_origin.clone(), registrar_lookup)?;
			Identity::<T>::set_fee(RawOrigin::Signed(registrar.clone()).into(), ii, 10u32.into())?;
			let fields = <T as pallet_identity::Config>::IdentityInformation::all_fields();
			Identity::<T>::set_fields(RawOrigin::Signed(registrar.clone()).into(), ii, fields)?;

			// request and provide judgement
			Identity::<T>::request_judgement(target_origin.clone(), ii, 10u32.into())?;
			Identity::<T>::provide_judgement(
				RawOrigin::Signed(registrar).into(),
				ii,
				target_lookup.clone(),
				Judgement::Reasonable,
				<T as frame_system::Config>::Hashing::hash_of(&info),
			)?;
		}

		let origin = T::Reaper::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, target.clone());

		assert_last_event::<T>(Event::<T>::IdentityReaped { who: target.clone() }.into());

		let fields = <T as pallet_identity::Config>::IdentityInformation::all_fields();
		assert!(!Identity::<T>::has_identity(&target, fields));
		assert_eq!(Identity::<T>::subs(&target).len(), 0);

		Ok(())
	}

	#[benchmark]
	fn poke_deposit() -> Result<(), BenchmarkError> {
		let target: T::AccountId = account("target", 0, SEED);
		let _ = T::Currency::make_free_balance_be(&target, BalanceOf::<T>::max_value());
		let info = <T as pallet_identity::Config>::IdentityInformation::create_identity_info();

		let _ = Identity::<T>::set_identity_no_deposit(&target, info.clone());

		let sub_account: T::AccountId = account("sub", 0, SEED);
		let name = Data::Raw(b"benchsub".to_vec().try_into().unwrap());
		let _ = Identity::<T>::set_subs_no_deposit(&target, vec![(sub_account.clone(), name)]);

		// expected deposits
		let expected_id_deposit = <T as pallet_identity::Config>::BasicDeposit::get()
			.saturating_add(
				<T as pallet_identity::Config>::ByteDeposit::get()
					.saturating_mul(<BalanceOf<T>>::from(info.encoded_size() as u32)),
			);
		// only 1 sub
		let expected_sub_deposit = <T as pallet_identity::Config>::SubAccountDeposit::get();

		#[extrinsic_call]
		_(RawOrigin::Root, target.clone());

		assert_last_event::<T>(
			Event::<T>::DepositUpdated {
				who: target,
				identity: expected_id_deposit,
				subs: expected_sub_deposit,
			}
			.into(),
		);

		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::integration_tests::new_test_ext(),
		crate::integration_tests::Test,
	);
}
