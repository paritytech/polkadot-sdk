// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::Pallet as InboundQueue;
use frame_benchmarking::v2::*;
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;

#[benchmarks(
	where
		<T::Verifier as snowbridge_inbound_queue_primitives::Verifier>::Proof: Clone
			+ core::fmt::Debug
			+ PartialEq
			+ codec::Encode
			+ codec::Decode
			+ scale_info::TypeInfo
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn submit() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let create_message = T::Helper::initialize_storage();

		#[block]
		{
			assert_ok!(InboundQueue::<T>::submit(
				RawOrigin::Signed(caller.clone()).into(),
				Box::new(create_message.event),
			));
		}

		Ok(())
	}

	/// Benchmarks weight of rejecting invalid proof at worst-case bounds
	/// (DefaultMaxDepth nodes, each DefaultMaxNodeSize bytes).
	#[benchmark]
	fn submit_invalid_proof_with_worst_case_bounds() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();

		let create_message = T::Helper::initialize_storage_worst_case_invalid_proof();

		#[block]
		{
			assert_noop!(
				InboundQueue::<T>::submit(
					RawOrigin::Signed(caller.clone()).into(),
					Box::new(create_message.event)
				),
				Error::<T>::Verification(VerificationError::InvalidProof)
			);
		}

		Ok(())
	}

	impl_benchmark_test_suite!(InboundQueue, crate::mock::new_tester(), crate::mock::Test);
}
