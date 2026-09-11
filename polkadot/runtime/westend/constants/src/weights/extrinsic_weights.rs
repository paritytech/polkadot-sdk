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

//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 32.0.0
//! DATE: 2026-09-08 (Y/M/D)
//! HOSTNAME: `<UNKNOWN>`, CPU: `<UNKNOWN>`
//!
//! SHORT-NAME: `extrinsic`, LONG-NAME: `ExtrinsicBase`, RUNTIME: `westend`
//! WARMUPS: `10`, REPEAT: `100`
//! WEIGHT-PATH: `./polkadot/runtime/westend/constants/src/weights/`
//! WEIGHT-METRIC: `Average`, WEIGHT-MUL: `1.0`, WEIGHT-ADD: `0`

// Executed Command:
//   frame-omni-bencher
//   v1
//   benchmark
//   overhead
//   --runtime
//   target/release/wbuild/westend-runtime/westend_runtime.compact.compressed.wasm
//   --weight-path=./polkadot/runtime/westend/constants/src/weights/
//   --warmup=10
//   --repeat=100
//   --header=./polkadot/file_header.txt
//   --extrinsic-subtract-weight
//   --signature-weight
//   42814000

use sp_core::parameter_types;
use sp_weights::{constants::WEIGHT_REF_TIME_PER_NANOS, Weight};

parameter_types! {
	/// Weight of executing a NO-OP extrinsic, for example `System::remark`.
	///
	/// NOTE: This benchmark uses the overhead extrinsic builder (default:
	/// `SubstrateRemarkBuilder`, a signed subxt remark) unless a custom builder is wired in by
	/// code. If subtraction is enabled, this generated constant explicitly subtracts the configured
	/// signature and extension weights.
	///
	/// For signed transactions, ensure signature/extension costs are re-accounted at runtime.
	/// Signature verification weight is a static property of the signature type via
	/// [`sp_runtime::traits::SignatureWeight`]. Pass the same ref-time value as
	/// `--signature-weight` when running this benchmark.
	/// Calculated by multiplying the *Average* with `1.0` and adding `0`.
	///
	/// Stats nanoseconds:
	///   Min, Max: 143_049, 215_064
	///   Average:  144_735
	///   Median:   143_821
	///   Std-Dev:  7294.67
	///
	/// Percentiles nanoseconds:
	///   99th: 161_640
	///   95th: 144_495
	///   75th: 144_075
	pub const ExtrinsicBaseWeight: Weight = Weight::from_parts(
		WEIGHT_REF_TIME_PER_NANOS.saturating_mul(144_735),
		0,
	)
	// Subtract configured signature weight for this benchmark setup.
	.saturating_sub(Weight::from_parts(
		42_814_000,
		0,
	))
	// Subtract configured transaction extension weight for this benchmark setup.
	.saturating_sub(Weight::from_parts(
		0,
		0,
	))
	;
}

#[cfg(test)]
mod test_weights {
	use sp_weights::constants;

	/// Checks that the weight exists and is sane.
	// NOTE: If this test fails but you are sure that the generated values are fine,
	// you can delete it.
	#[test]
	fn sane() {
		let w = super::ExtrinsicBaseWeight::get();

		// At least 10 µs.
		assert!(
			w.ref_time() >= 10u64 * constants::WEIGHT_REF_TIME_PER_MICROS,
			"Weight should be at least 10 µs."
		);
		// At most 1 ms.
		assert!(
			w.ref_time() <= constants::WEIGHT_REF_TIME_PER_MILLIS,
			"Weight should be at most 1 ms."
		);
	}
}
