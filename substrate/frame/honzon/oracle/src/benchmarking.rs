use super::*;
use crate::Pallet as Oracle;

use frame_benchmarking::v2::*;

use frame_support::assert_ok;
use frame_system::{Pallet as System, RawOrigin};

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn feed_values(
		x: Linear<0, { T::BenchmarkHelper::get_currency_id_value_pairs().len() as u32 }>,
	) {
		// Register the caller
		let caller: T::AccountId = whitelisted_caller();
		T::Members::add(&caller);

		let values = T::BenchmarkHelper::get_currency_id_value_pairs()[..x as usize]
			.to_vec()
			.try_into()
			.expect("Must succeed since at worst the length remained the same.");

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), values);

		assert!(HasDispatched::<T, I>::get().contains(&caller));
	}

	#[benchmark]
	fn on_finalize() {
		// Register the caller
		let caller: T::AccountId = whitelisted_caller();
		T::Members::add(&caller);

		// Feed some values before running `on_finalize` hook
		System::<T>::set_block_number(1u32.into());
		let values = T::BenchmarkHelper::get_currency_id_value_pairs();
		assert_ok!(Oracle::<T, I>::feed_values(RawOrigin::Signed(caller).into(), values));

		#[block]
		{
			Oracle::<T, I>::on_finalize(System::<T>::block_number());
		}

		assert!(!HasDispatched::<T, I>::exists());
	}

	impl_benchmark_test_suite! {
		Oracle,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	}
}
