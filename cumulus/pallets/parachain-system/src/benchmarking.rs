// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Benchmarking for the parachain-system pallet.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
	block_weight::{
		mock::has_use_full_core_digest, BlockWeightMode, DynamicMaxBlockWeight,
		MaxParachainBlockWeight,
	},
	parachain_inherent::InboundDownwardMessages,
};
use cumulus_primitives_core::{
	relay_chain::Hash as RelayHash, BundleInfo, CoreInfo, InboundDownwardMessage,
};
use frame_benchmarking::v2::*;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	weights::constants::WEIGHT_REF_TIME_PER_SECOND,
};
use frame_system::RawOrigin;
use sp_core::ConstU32;
use sp_runtime::traits::{BlakeTwo256, DispatchTransaction, Dispatchable};

#[benchmarks(where
	T: Send + Sync,
    T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
)]
mod benchmarks {
	use super::*;

	/// Enqueue `n` messages via `enqueue_inbound_downward_messages`.
	///
	/// The limit is set to `1000` for benchmarking purposes as the actual limit is only known at
	/// runtime. However, the limit (and default) for Dotsama are magnitudes smaller.
	#[benchmark]
	fn enqueue_inbound_downward_messages(n: Linear<0, 1000>) {
		let msg = InboundDownwardMessage {
			sent_at: n, // The block number does not matter.
			msg: vec![0u8; MaxDmpMessageLenOf::<T>::get() as usize],
		};
		let msgs = vec![msg; n as usize];
		let head = mqp_head(&msgs);

		#[block]
		{
			Pallet::<T>::enqueue_inbound_downward_messages(
				head,
				InboundDownwardMessages::new(msgs).into_abridged(&mut usize::MAX.clone()),
			);
		}

		assert_eq!(ProcessedDownwardMessages::<T>::get(), n);
		assert_eq!(LastDmqMqcHead::<T>::get().head(), head);
	}

	/// Re-implements an easy version of the `MessageQueueChain` for testing purposes.
	fn mqp_head(msgs: &Vec<InboundDownwardMessage>) -> RelayHash {
		let mut head = Default::default();
		for msg in msgs.iter() {
			let msg_hash = BlakeTwo256::hash_of(&msg.msg);
			head = BlakeTwo256::hash_of(&(head, msg.sent_at, msg_hash));
		}
		head
	}

	#[benchmark]
	fn block_weight_tx_extension_max_weight() -> Result<(), BenchmarkError> {
		let caller = account("caller", 0, 0);

		frame_system::Pallet::<T>::inherents_applied();

		frame_system::Pallet::<T>::deposit_log(
			BundleInfo { index: 0, maybe_last: false }.to_digest_item(),
		);
		frame_system::Pallet::<T>::deposit_log(
			CoreInfo {
				selector: 0.into(),
				claim_queue_offset: 0.into(),
				number_of_cores: 1.into(),
			}
			.to_digest_item(),
		);
		let target_weight = MaxParachainBlockWeight::<T, ConstU32<4>>::get();

		let info = DispatchInfo {
			// The weight needs to be more than the target weight.
			call_weight: target_weight
				.saturating_add(Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND, 0)),
			extension_weight: Weight::zero(),
			class: DispatchClass::Normal,
			..Default::default()
		};
		let call: T::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
		let post_info = PostDispatchInfo { actual_weight: None, pays_fee: Default::default() };
		let len = 0_usize;

		crate::BlockWeightMode::<T>::put(BlockWeightMode::FractionOfCore {
			first_transaction_index: None,
		});

		let ext = DynamicMaxBlockWeight::<T, (), ConstU32<4>>::new(());

		#[block]
		{
			ext.test_run(RawOrigin::Signed(caller).into(), &call, &info, len, 0, |_| Ok(post_info))
				.unwrap()
				.unwrap();
		}

		assert!(has_use_full_core_digest());
		assert_eq!(
			MaxParachainBlockWeight::<T, ConstU32<4>>::get(),
			MaxParachainBlockWeight::<T, ConstU32<4>>::FULL_CORE_WEIGHT
		);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
