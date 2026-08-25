// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::Pallet as InboundQueue;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::Get};
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Benchmark `submit`, parameterized by:
	/// - `n`: number of nodes in the receipt-inclusion proof. The verifier's per-node cost (RLP
	///   decode + branch traversal) scales linearly with `n`.
	/// - `s`: size in bytes of the receipt that the proof terminates at. The leaf node's size is
	///   dominated by the receipt body, and decode/scan cost scales linearly with `s`. The lower
	///   bound `320` reflects the smallest realistic receipt envelope: an empty-logs Eip2930/1559
	///   receipt with a 256-byte logs bloom.
	///
	/// `MaxProofNodes` and `MaxReceiptBytes` are runtime-benchmarks-only Config items; they
	/// bound the benchmark's exploration of the worst case but do NOT bound proof or
	/// receipt sizes at runtime. The framework fits a slope from these samples and the
	/// dispatch attribute scales the declared weight using the actual `n`/`s` of the
	/// submitted event.
	#[benchmark]
	fn submit(
		n: Linear<1, { T::MaxProofNodes::get() }>,
		s: Linear<320, { T::MaxReceiptBytes::get() }>,
	) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let create_message = T::Helper::initialize_storage(n, s);

		#[block]
		{
			assert_ok!(InboundQueue::<T>::submit(
				RawOrigin::Signed(caller.clone()).into(),
				Box::new(create_message.event),
			));
		}

		Ok(())
	}

	impl_benchmark_test_suite!(InboundQueue, crate::mock::new_tester(), crate::mock::Test);
}
