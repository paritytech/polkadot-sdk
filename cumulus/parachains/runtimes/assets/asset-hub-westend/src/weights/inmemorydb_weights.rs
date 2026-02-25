// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 32.0.0
//! DATE: 2025-05-25 (Y/M/D)
//! HOSTNAME: `versi-developer-0`, CPU: `Intel(R) Xeon(R) CPU @ 2.60GHz`
//!
//! DATABASE: `InMemoryDb`, RUNTIME: `Polkadot Asset Hub`
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
//   --mode
//   validate-block
//   --validate-block-rounds
//   100

/// Storage DB weights for the `Polkadot Asset Hub` runtime and `InMemoryDb`.
pub mod constants {
	use frame_support::weights::{constants, RuntimeDbWeight};
	use sp_core::parameter_types;

	parameter_types! {
		/// `InMemoryDb` weights are measured in the context of the validation functions.
		/// To avoid submitting overweight blocks to the relay chain this is the configuration
		/// parachains should use.
		pub const InMemoryDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			// Time to read one storage item.
			// Calculated by multiplying the *Average* of all values with `1.0` and adding `0`.
			//
			// Stats nanoseconds:
			//   Min, Max: 12_883, 13_516
			//   Average:  13_036
			//   Median:   13_031
			//   Std-Dev:  69.49
			//
			// Percentiles nanoseconds:
			//   99th: 13_242
			//   95th: 13_152
			//   75th: 13_070
			read: 13_036 * constants::WEIGHT_REF_TIME_PER_NANOS,

			// Time to write one storage item.
			// Calculated by multiplying the *Average* of all values with `1.0` and adding `0`.
			//
			// Stats nanoseconds:
			//   Min, Max: 28_998, 32_249
			//   Average:  31_215
			//   Median:   31_667
			//   Std-Dev:  1047.8
			//
			// Percentiles nanoseconds:
			//   99th: 32_195
			//   95th: 32_114
			//   75th: 31_852
			write: 31_215 * constants::WEIGHT_REF_TIME_PER_NANOS,
		};
	}

	#[cfg(test)]
	mod test_db_weights {
		use super::InMemoryDbWeight as W;
		use frame_support::weights::constants;

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
