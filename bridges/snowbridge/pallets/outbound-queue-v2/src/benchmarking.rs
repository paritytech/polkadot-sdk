// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use bridge_hub_common::AggregateMessageOrigin;
use codec::Encode;
use frame_benchmarking::v2::*;
use snowbridge_core::{
	outbound::{Command, Initializer},
	ChannelId,
};
use sp_core::{H160, H256};

#[allow(unused_imports)]
use crate::Pallet as OutboundQueue;

#[benchmarks(
	where
		<T as Config>::MaxMessagePayloadSize: Get<u32>,
)]
mod benchmarks {
	use super::*;

	/// Benchmark for processing a message.
	#[benchmark]
	fn do_process_message() -> Result<(), BenchmarkError> {
		let enqueued_message = QueuedMessage {
			id: H256::zero(),
			channel_id: ChannelId::from([1; 32]),
			command: Command::Upgrade {
				impl_address: H160::zero(),
				impl_code_hash: H256::zero(),
				initializer: Some(Initializer {
					params: [7u8; 256].into_iter().collect(),
					maximum_required_gas: 200_000,
				}),
			},
		};
		let origin = AggregateMessageOrigin::Snowbridge([1; 32].into());
		let encoded_enqueued_message = enqueued_message.encode();

		#[block]
		{
			let _ = OutboundQueue::<T>::do_process_message(origin, &encoded_enqueued_message);
		}

		assert_eq!(MessageLeaves::<T>::decode_len().unwrap(), 1);

		Ok(())
	}

	/// Benchmark for producing final messages commitment
	#[benchmark]
	fn commit() -> Result<(), BenchmarkError> {
		// Assume worst case, where `MaxMessagesPerBlock` messages need to be committed.
		for i in 0..T::MaxMessagesPerBlock::get() {
			let leaf_data: [u8; 1] = [i as u8];
			let leaf = <T as Config>::Hashing::hash(&leaf_data);
			MessageLeaves::<T>::append(leaf);
		}

		#[block]
		{
			OutboundQueue::<T>::commit();
		}

		Ok(())
	}

	/// Benchmark for producing commitment for a single message
	#[benchmark]
	fn commit_single() -> Result<(), BenchmarkError> {
		let leaf = <T as Config>::Hashing::hash(&[100; 1]);
		MessageLeaves::<T>::append(leaf);

		#[block]
		{
			OutboundQueue::<T>::commit();
		}

		Ok(())
	}

	impl_benchmark_test_suite!(OutboundQueue, crate::mock::new_tester(), crate::mock::Test,);
}
