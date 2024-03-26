#![cfg(feature = "runtime-benchmarks")]

use crate::{
	migrations::{
		v1,
		v1::{weights, weights::WeightInfo},
	},
	Config, Pallet,
};
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};

#[benchmarks]
mod benches {
	use super::*;

	/// Benchmark a single step of the `v1::LazyMigrationV1` migration.
	#[benchmark]
	fn step() {
		v1::old::MyMap::<T>::insert(0, 0);
		let mut meter = WeightMeter::new();

		#[block]
		{
			v1::LazyMigrationV1::<T>::step(None, &mut meter).unwrap();
		}

		// Check that the new storage is decodable:
		assert_eq!(crate::MyMap::<T>::get(0), Some(0));
		assert_eq!(meter.consumed(), weights::SubstrateWeight::<T>::step());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Runtime);
}
