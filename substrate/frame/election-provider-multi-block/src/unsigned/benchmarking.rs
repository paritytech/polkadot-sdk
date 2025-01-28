use super::{Pallet as UnsignedPallet, *};
use crate::{helpers, types::*};
use frame_support::ensure;

const SEED: u64 = 1;

frame_benchmarking::benchmarks! {
	foo {}: {} verify {}
}

frame_benchmarking::impl_benchmark_test_suite!(
	UnsignedPallet,
	crate::mock::ExtBuilder::unsigned().build_offchainify().0,
	crate::mock::Runtime,
);
