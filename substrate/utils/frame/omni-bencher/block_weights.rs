
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 32.0.0
//! DATE: 2026-04-07 (Y/M/D)
//! HOSTNAME: `Jimohs-MacBook-Pro.local`, CPU: `<UNKNOWN>`
//!
//! SHORT-NAME: `block`, LONG-NAME: `BlockExecution`, RUNTIME: `test`
//! WARMUPS: `2`, REPEAT: `2`
//! WEIGHT-PATH: ``
//! WEIGHT-METRIC: `Average`, WEIGHT-MUL: `1.0`, WEIGHT-ADD: `0`

// Executed Command:
//   /Users/kanas/Desktop/Parity/polkadot-sdk/target/debug/frame-omni-bencher
//   v1
//   benchmark
//   overhead
//   --runtime
//   /var/folders/z4/cp6r4cpn1458f0z35p1r00z00000gn/T/.tmpiRNS0q
//   --warmup
//   2
//   --repeat
//   2

use sp_core::parameter_types;
use sp_weights::{constants::WEIGHT_REF_TIME_PER_NANOS, Weight};

parameter_types! {
	/// Weight of executing an empty block.
	/// Calculated by multiplying the *Average* with `1.0` and adding `0`.
	///
	/// Stats nanoseconds:
	///   Min, Max: 8_076_987, 10_643_188
	///   Average:  9_360_087
	///   Median:   8_076_987
	///   Std-Dev:  1283100.5
	///
	/// Percentiles nanoseconds:
	///   99th: 10_643_188
	///   95th: 10_643_188
	///   75th: 10_643_188
	pub const BlockExecutionWeight: Weight =
		Weight::from_parts(WEIGHT_REF_TIME_PER_NANOS.saturating_mul(9_360_087), 0);
}

#[cfg(test)]
mod test_weights {
	use sp_weights::constants;

	/// Checks that the weight exists and is sane.
	// NOTE: If this test fails but you are sure that the generated values are fine,
	// you can delete it.
	#[test]
	fn sane() {
		let w = super::BlockExecutionWeight::get();

		// At least 100 µs.
		assert!(
			w.ref_time() >= 100u64 * constants::WEIGHT_REF_TIME_PER_MICROS,
			"Weight should be at least 100 µs."
		);
		// At most 50 ms.
		assert!(
			w.ref_time() <= 50u64 * constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Weight should be at most 50 ms."
		);
	}
}
