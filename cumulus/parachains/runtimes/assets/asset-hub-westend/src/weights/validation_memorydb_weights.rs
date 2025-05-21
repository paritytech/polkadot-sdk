//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 32.0.0
//! DATE: 2025-04-28 (Y/M/D)
//! HOSTNAME: `versi-developer-0`, CPU: `Intel(R) Xeon(R) CPU @ 2.60GHz`
//!
//! DATABASE: `ValidationMemoryDb`, RUNTIME: `Polkadot Asset Hub`
//! BLOCK-NUM: `BlockId::Number(8404035)`
//! SKIP-WRITE: `false`, SKIP-READ: `false`, WARMUPS: `1`
//! STATE-VERSION: `V1`, STATE-CACHE-SIZE: ``
//! WEIGHT-PATH: ``
//! METRIC: `Average`, WEIGHT-MUL: `1.0`, WEIGHT-ADD: `0`

// Executed Command:
//   ./target/production/polkadot-parachain
//   benchmark
//   storage
//   --warmups
//   1
//   --state-version
//   1
//   --base-path
//   /opt/local-ssd/polkadot-asset-hub
//   --chain
//   cumulus/polkadot-parachain/chain-specs/asset-hub-polkadot.json
//   --detailed-log-output
//   --enable-trie-cache
//   --trie-cache-size
//   10737418240
//   --batch-size
//   10000
//   --on-block-validation

/// Storage DB weights for the `Polkadot Asset Hub` runtime and `ValidationMemoryDb`.
pub mod constants {
	use frame_support::{
		parameter_types,
		weights::{constants, RuntimeDbWeight},
	};

	parameter_types! {
		/// `ValidationMemoryDb` weights are measured in the context of the validation functions.
		/// To avoid submitting overweight blocks to the relay chain this is the configuration
		/// parachains should use.
		pub const ValidationMemoryDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			/// Time to read one storage item.
			/// Calculated by multiplying the *Average* of all values with `1.0` and adding `0`.
			///
			/// Stats nanoseconds:
			///   Min, Max: 19_022, 21_311
			///   Average:  19_776
			///   Median:   19_754
			///   Std-Dev:  259.72
			///
			/// Percentiles nanoseconds:
			///   99th: 20_617
			///   95th: 20_219
			///   75th: 19_924
			read: 19_776 * constants::WEIGHT_REF_TIME_PER_NANOS,

			/// Time to write one storage item.
			/// Calculated by multiplying the *Average* of all values with `1.0` and adding `0`.
			///
			/// Stats nanoseconds:
			///   Min, Max: 37_874, 39_649
			///   Average:  38_604
			///   Median:   38_590
			///   Std-Dev:  270.62
			///
			/// Percentiles nanoseconds:
			///   99th: 39_412
			///   95th: 39_099
			///   75th: 38_767
			write: 38_604 * constants::WEIGHT_REF_TIME_PER_NANOS,
		};
	}

	#[cfg(test)]
	mod test_db_weights {
		use super::constants::ValidationMemoryDbWeight as W;
		use sp_weights::constants;

		/// Checks that all weights exist and have sane values.
		// NOTE: If this test fails but you are sure that the generated values are fine,
		// you can delete it.
		#[test]
		fn bound() {
			// At least 1 µs.
			assert!(
				W::get().reads(1).ref_time() >= constants::WEIGHT_REF_TIME_PER_MICROS,
				"Read weight should be at least 1 µs."
			);
			assert!(
				W::get().writes(1).ref_time() >= constants::WEIGHT_REF_TIME_PER_MICROS,
				"Write weight should be at least 1 µs."
			);
			// At most 1 ms.
			assert!(
				W::get().reads(1).ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
				"Read weight should be at most 1 ms."
			);
			assert!(
				W::get().writes(1).ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
				"Write weight should be at most 1 ms."
			);
		}
	}
}
