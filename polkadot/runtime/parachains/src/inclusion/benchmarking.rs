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

use bitvec::{bitvec, prelude::Lsb0};
use frame_benchmarking::v2::*;
use pallet_message_queue as mq;
use polkadot_primitives::{
	vstaging::CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CandidateCommitments,
	HrmpChannelId, OutboundHrmpMessage, SessionIndex,
};

use super::*;
use crate::{
	builder::generate_validator_pairs,
	configuration,
	hrmp::{HrmpChannel, HrmpChannels},
	initializer, HeadData, ValidationCode,
};

fn create_candidate_commitments<T: crate::hrmp::pallet::Config>(
	para_id: ParaId,
	head_data: HeadData,
	max_msg_len: usize,
	ump_msg_count: u32,
	hrmp_msg_count: u32,
	code_upgrade: bool,
) -> CandidateCommitments {
	let upward_messages = {
		let unbounded = create_messages(max_msg_len, ump_msg_count as _);
		BoundedVec::truncate_from(unbounded)
	};

	let horizontal_messages = {
		let unbounded = create_messages(max_msg_len, hrmp_msg_count as _);

		for n in 0..unbounded.len() {
			let channel_id = HrmpChannelId { sender: para_id, recipient: para_id + n as u32 + 1 };
			HrmpChannels::<T>::insert(
				&channel_id,
				HrmpChannel {
					sender_deposit: 42,
					recipient_deposit: 42,
					max_capacity: 10_000_000,
					max_total_size: 1_000_000_000,
					max_message_size: 10_000_000,
					msg_count: 0,
					total_size: 0,
					mqc_head: None,
				},
			);
		}

		let unbounded = unbounded
			.into_iter()
			.enumerate()
			.map(|(n, data)| OutboundHrmpMessage { recipient: para_id + n as u32 + 1, data })
			.collect();
		BoundedVec::truncate_from(unbounded)
	};

	let new_validation_code = code_upgrade.then_some(ValidationCode(vec![42_u8; 1024]));

	CandidateCommitments::<u32> {
		upward_messages,
		horizontal_messages,
		new_validation_code,
		head_data,
		processed_downward_messages: 0,
		hrmp_watermark: 10,
	}
}

fn create_messages(msg_len: usize, n_msgs: usize) -> Vec<Vec<u8>> {
	let best_number = 73_u8; // Chuck Norris of numbers
	vec![vec![best_number; msg_len]; n_msgs]
}

#[benchmarks(where T: mq::Config + configuration::Config + initializer::Config)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn enact_candidate(u: Linear<0, 2>, h: Linear<0, 2>, c: Linear<0, 1>) {
		let para = 42_u32.into(); // not especially important.

		let max_len = mq::MaxMessageLenOf::<T>::get() as usize;

		let config = configuration::ActiveConfig::<T>::get();
		let n_validators = config.max_validators.unwrap_or(500);
		let validators = generate_validator_pairs::<T>(n_validators);

		let session = SessionIndex::from(0_u32);
		initializer::Pallet::<T>::test_trigger_on_new_session(
			false,
			session,
			validators.iter().map(|(a, v)| (a, v.clone())),
			None,
		);
		let backing_group_size = config.scheduler_params.max_validators_per_core.unwrap_or(5);
		let head_data = HeadData(vec![0xFF; 1024]);

		let relay_parent_number = BlockNumberFor::<T>::from(10_u32);
		let commitments = create_candidate_commitments::<T>(para, head_data, max_len, u, h, c != 0);
		let backers = bitvec![u8, Lsb0; 1; backing_group_size as usize];
		let availability_votes = bitvec![u8, Lsb0; 1; n_validators as usize];
		let core_index = CoreIndex::from(0);
		let backing_group = GroupIndex::from(0);

		let descriptor = CandidateDescriptor::<T::Hash>::new(
			para,
			Default::default(),
			CoreIndex(0),
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			ValidationCode(vec![1, 2, 3]).hash(),
		);

		let receipt = CommittedCandidateReceipt::<T::Hash> { descriptor, commitments };

		Pallet::<T>::receive_upward_messages(para, &vec![vec![0; max_len]; 1]);

		#[block]
		{
			Pallet::<T>::enact_candidate(
				relay_parent_number,
				receipt,
				backers,
				availability_votes,
				core_index,
				backing_group,
			);
		}
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	}
}
