//! Benchmarking setup for pallet-voting
#![cfg(feature = "runtime-benchmarks")]
use super::*;

#[allow(unused)]
use crate::Pallet as Multisig;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_multisig() {
		let mut owners: BoundedBTreeSet<T::AccountId, T::MaxSignatories> = BoundedBTreeSet::new();
		let signatory: T::AccountId = account("signatory", 0, 0);
		owners.try_insert(signatory.clone()).unwrap();
		// owners.try_insert(1).unwrap();

		let multisig_account =
			Multisig::<T>::get_multisig_account_id(&owners, Multisig::<T>::timepoint());

		#[extrinsic_call]
		_(RawOrigin::Signed(signatory), owners, 1);
		assert_eq!(true, false);
		// assert_eq!(MultisigAccount::<Test>::get(multisig_account).unwrap().owners, owners);
	}

	// #[benchmark]
	// fn cause_error() {
	// 	Something::<T>::put(100u32);
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	#[extrinsic_call]
	// 	cause_error(RawOrigin::Signed(caller));

	// 	assert_eq!(MultisigAccount::<T>::get(1), Some());
	// }

	impl_benchmark_test_suite!(Multisig, crate::mock::new_test_ext(), crate::mock::Test);
}
