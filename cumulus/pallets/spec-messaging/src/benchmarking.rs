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

//! Benchmarks for the spec-messaging pallet. Each one sets up its worst case: frontiers with 47
//! peaks, one stream per send, and commitment-tree paths `MaxTreeDepth` nodes deep.

use super::*;
use alloc::vec;
use cumulus_primitives_spec_messaging::{PrivateKind, StreamsTree, STREAM_ID_LEN};
use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Hooks};
use frame_system::RawOrigin;

/// A sibling distinct from this chain, by index.
fn sibling<T: Config>(index: u32) -> ParaId {
	ParaId::from(u32::from(T::SelfParaId::get()).saturating_add(1).saturating_add(index))
}

/// A frontier with 47 peaks. Appending a leaf merges all of them, and the frontier still has room
/// for the payloads of one item.
fn wide_frontier() -> MmrFrontier {
	const LEAVES: u64 = (1 << 47) - 1;
	MmrFrontier::from_parts(vec![Hash::repeat_byte(1); LEAVES.count_ones() as usize], LEAVES)
		.expect("one peak per set bit, below the leaf cap; qed")
}

/// The block's roll-over has run, as `on_initialize` does before anything else touches the block.
fn mark_rolled_over<T: Config>() {
	let now = frame_system::Pallet::<T>::block_number().max(1u32.into());
	frame_system::Pallet::<T>::set_block_number(now);
	RolledOver::<T>::put((now, 0));
}

/// A private stream, with `index` spread over the high bits so streams share few tree nodes.
fn deep_stream(index: u32) -> StreamId {
	let mut body = [0u8; 7];
	body[..4].copy_from_slice(&index.reverse_bits().to_be_bytes());
	StreamId::Private { kind: PrivateKind::new(0x80).expect("in range; qed"), body }
}

/// `count` valid keys one bit away from `stream`, flipping bits from the last one up and never
/// at or before `after`. Each deepens `stream`'s path by one node.
fn neighbours(stream: StreamId, count: u32, after: Option<u8>) -> Vec<StreamId> {
	let first = after.map_or(0, |bit| bit as usize + 1);
	let found: Vec<StreamId> = (first..STREAM_ID_LEN * 8)
		.rev()
		.filter_map(|bit| {
			let mut other = stream.to_bytes();
			other[bit / 8] ^= 0x80 >> (bit % 8);
			StreamId::from_bytes(other).ok()
		})
		.take(count as usize)
		.collect();
	assert_eq!(found.len(), count as usize, "the key has room for the depth");
	found
}

/// Fold `streams` into the node cache, each with neighbours so its path is exactly
/// `MaxTreeDepth` nodes deep.
fn build_deep_tree<T: Config>(streams: &[StreamId]) {
	let depth = T::MaxTreeDepth::get();
	let shape = StreamsTree::from_entries(&streams.iter().map(|s| (*s, Hash::zero())).collect());
	let mut touched = BTreeMap::new();
	for stream in streams {
		touched.insert(*stream, Hash::repeat_byte(2));
		let (_, proof) = shape.gen_stream_proof(*stream).expect("the stream is in the shape; qed");
		let shared = proof.steps.len() as u32;
		let below = proof.steps.iter().map(|step| step.split_bit).max();
		for other in neighbours(*stream, depth.checked_sub(shared).expect("few streams"), below) {
			touched.insert(other, Hash::repeat_byte(3));
		}
	}
	if !touched.is_empty() {
		Pallet::<T>::fold_into_tree(&touched).expect("an empty cache folds; qed");
		let height = TreeRoot::<T>::get().map_or(0, |entry| entry.height());
		assert_eq!(height as u32, depth);
	}
}

/// Fold neighbours of `stream`, but not `stream`, so inserting it walks a path `MaxTreeDepth - 1`
/// nodes deep and ends `MaxTreeDepth` deep.
fn build_tree_around<T: Config>(stream: StreamId) {
	let depth = T::MaxTreeDepth::get();
	let touched = neighbours(stream, depth, None)
		.into_iter()
		.map(|other| (other, Hash::repeat_byte(3)))
		.collect();
	Pallet::<T>::fold_into_tree(&touched).expect("an empty cache folds; qed");
	let height = TreeRoot::<T>::get().map_or(0, |entry| entry.height());
	assert_eq!(height as u32 + 1, depth);
}

/// Queue one send on `stream` directly: payload, leaf hash and the block's send count.
fn queue_send<T: Config>(stream: StreamId) {
	let payload: BoundedVec<u8, T::MaxMsgLen> = vec![0u8].try_into().expect("1 byte fits; qed");
	let hash = leaf_hash_with_version(LEAF_VERSION, &payload);
	OutboundMessages::<T>::insert(stream, BoundedVec::truncate_from(vec![payload]));
	OutboundLeafHashes::<T>::insert(stream, BoundedVec::truncate_from(vec![hash]));
	SendsThisBlock::<T>::mutate(|sends| *sends = sends.saturating_add(1));
}

/// An inbound channel that is open and consuming, with a wide frontier.
fn open_inbound<T: Config>(channel: InChannelId) {
	InChannels::<T>::insert(
		channel,
		InChannelState {
			frontier: wide_frontier(),
			session: 1,
			status: RegisterStatus::Opened,
			published: Some(Register { up_to: MessagePosition(0), status: RegisterStatus::Opened }),
			open: Some(SessionParams { version: 1, max_message_size: T::MaxMsgLen::get() }),
			draining: false,
		},
	);
}

/// An outbound channel that took part in session 1 and applied its `Opened`.
fn open_outbound<T: Config>(channel: OutChannelId) {
	OutChannels::<T>::insert(
		channel,
		OutChannelState {
			want_open: true,
			session: 1,
			register: Some(AppliedRegister {
				position: MessagePosition(0),
				register: Register { up_to: MessagePosition(0), status: RegisterStatus::Opened },
			}),
			open: Some(OpenSession {
				session: 1,
				params: SessionParams { version: 1, max_message_size: T::MaxMsgLen::get() },
				opened: true,
			}),
			closed_session: None,
		},
	);
}

/// An outbound channel that wants to open, with the recipient's `Accepting` register applied.
fn accepting_outbound<T: Config>(channel: OutChannelId, want_open: bool) {
	let supported = T::SupportedVersions::get();
	OutChannels::<T>::insert(
		channel,
		OutChannelState {
			want_open,
			session: 1,
			register: Some(AppliedRegister {
				position: MessagePosition(0),
				register: Register {
					up_to: MessagePosition(0),
					status: RegisterStatus::Accepting {
						session: 1,
						min: *supported.start(),
						max: *supported.end(),
						max_message_size: T::MaxMessageSize::get(),
					},
				},
			}),
			open: None,
			closed_session: None,
		},
	);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// The recipient's `Accepting` register is applied, so the call sends the `Hello`.
	#[benchmark]
	fn open_channel() -> Result<(), BenchmarkError> {
		let origin = T::OpenChannelOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let channel = OutChannelId { recipient: sibling::<T>(0), domain: 0, num: 0 };
		accepting_outbound::<T>(channel, false);
		build_tree_around::<T>(channel.data_stream());
		mark_rolled_over::<T>();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, channel.recipient, channel.domain, channel.num);

		assert_eq!(OutboundLeafHashes::<T>::get(channel.data_stream()).len(), 1);
		assert_eq!(PendingTreeKeys::<T>::count(), 1);
		Ok(())
	}

	/// Reaccepting a closed channel whose frontier is wide.
	#[benchmark]
	fn accept_open_channel() -> Result<(), BenchmarkError> {
		let origin = T::AcceptChannelOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let channel = InChannelId { sender: sibling::<T>(0), domain: 0, num: 0 };
		InChannels::<T>::insert(
			channel,
			InChannelState {
				frontier: wide_frontier(),
				session: 1,
				status: RegisterStatus::Closed { session: 1 },
				published: Some(Register {
					up_to: MessagePosition(0),
					status: RegisterStatus::Closed { session: 1 },
				}),
				open: None,
				draining: false,
			},
		);
		build_tree_around::<T>(channel.ack_stream());

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, channel.sender, channel.domain, channel.num);

		assert!(matches!(
			InChannels::<T>::get(channel).map(|s| s.status),
			Some(RegisterStatus::Accepting { .. })
		));
		assert_eq!(PendingTreeKeys::<T>::count(), 1);
		Ok(())
	}

	/// The channel took part in its session, so the call sends the `CloseChannel`.
	#[benchmark]
	fn close_channel() -> Result<(), BenchmarkError> {
		let origin = T::OpenChannelOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let channel = OutChannelId { recipient: sibling::<T>(0), domain: 0, num: 0 };
		open_outbound::<T>(channel);
		mark_rolled_over::<T>();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, channel.recipient, channel.domain, channel.num);

		assert_eq!(OutboundLeafHashes::<T>::get(channel.data_stream()).len(), 1);
		Ok(())
	}

	#[benchmark]
	fn close_inbound_channel() -> Result<(), BenchmarkError> {
		let origin = T::AcceptChannelOrigin::try_successful_origin()
			.map_err(|_| BenchmarkError::Weightless)?;
		let channel = InChannelId { sender: sibling::<T>(0), domain: 0, num: 0 };
		open_inbound::<T>(channel);

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, channel.sender, channel.domain, channel.num);

		assert!(InChannels::<T>::get(channel).is_some_and(|s| s.draining));
		Ok(())
	}

	/// `c` open inbound channels consumed, one message each. The `DataHandler` is not measured: its
	/// weight is added per payload.
	#[benchmark(pov_mode = Measured)]
	fn enact_channels(c: Linear<1, { T::MaxTouchedStreams::get() }>) {
		let mut data = MessagingInherentData::default();
		for i in 0..c {
			let channel = InChannelId { sender: sibling::<T>(i), domain: 0, num: 0 };
			open_inbound::<T>(channel);
			data.channels.insert(channel, vec![ChannelMessage::Data(Vec::new()).encode()]);
		}
		mark_rolled_over::<T>();

		#[extrinsic_call]
		enact_messages(RawOrigin::None, data);

		assert_eq!(ConsumptionOutbox::<T>::get().len() as u32, c);
	}

	/// One channel consumed with `n` messages beyond the first.
	#[benchmark(pov_mode = Measured)]
	fn enact_payloads(n: Linear<0, { T::MaxInboundMessagesPerItem::get().saturating_sub(1) }>) {
		let channel = InChannelId { sender: sibling::<T>(0), domain: 0, num: 0 };
		open_inbound::<T>(channel);
		let payloads = (0..=n).map(|_| ChannelMessage::Data(Vec::new()).encode()).collect();
		let data = MessagingInherentData {
			channels: BTreeMap::from([(channel, payloads)]),
			..Default::default()
		};
		mark_rolled_over::<T>();

		#[extrinsic_call]
		enact_messages(RawOrigin::None, data);

		assert_eq!(ConsumptionOutbox::<T>::get().len(), 1);
	}

	/// One channel consumed with a `b`-byte message.
	#[benchmark(pov_mode = Measured)]
	fn enact_bytes(b: Linear<0, { T::MaxMsgLen::get().saturating_sub(8) }>) {
		let channel = InChannelId { sender: sibling::<T>(0), domain: 0, num: 0 };
		open_inbound::<T>(channel);
		let data = MessagingInherentData {
			channels: BTreeMap::from([(
				channel,
				vec![ChannelMessage::Data(vec![0u8; b as usize]).encode()],
			)]),
			..Default::default()
		};
		mark_rolled_over::<T>();

		#[extrinsic_call]
		enact_messages(RawOrigin::None, data);

		assert_eq!(ConsumptionOutbox::<T>::get().len(), 1);
	}

	/// `r` register reads that each make the sender send a `Hello`, on new streams.
	#[benchmark(pov_mode = Measured)]
	fn enact_registers(r: Linear<0, { T::MaxTouchedStreams::get() }>) {
		let supported = T::SupportedVersions::get();
		let register = Register {
			up_to: MessagePosition(0),
			status: RegisterStatus::Accepting {
				session: 1,
				min: *supported.start(),
				max: *supported.end(),
				max_message_size: T::MaxMessageSize::get(),
			},
		};
		let mut data = MessagingInherentData::default();
		for j in 0..r {
			let channel = OutChannelId { recipient: sibling::<T>(j), domain: 0, num: 0 };
			OutChannels::<T>::insert(
				channel,
				OutChannelState { want_open: true, ..Default::default() },
			);
			let read = LossyRead { frontier: wide_frontier(), payloads: vec![register.encode()] };
			data.registers.insert(channel, read);
		}
		mark_rolled_over::<T>();

		#[extrinsic_call]
		enact_messages(RawOrigin::None, data);

		assert_eq!(SendsThisBlock::<T>::get(), r);
	}

	/// `on_initialize` draining `d` leaf hashes, each in its own stream with a wide frontier.
	#[benchmark(pov_mode = Measured)]
	fn on_initialize(d: Linear<0, { T::MaxSendsPerBlock::get() }>) {
		for i in 0..d {
			let stream = deep_stream(i);
			OutboundFrontier::<T>::insert(stream, wide_frontier());
			queue_send::<T>(stream);
		}
		mark_rolled_over::<T>();

		// The sends were the previous block's.
		let next = frame_system::Pallet::<T>::block_number() + 1u32.into();
		frame_system::Pallet::<T>::set_block_number(next);

		#[block]
		{
			Pallet::<T>::on_initialize(next);
		}

		assert!(OutboundLeafHashes::<T>::iter().next().is_none());
	}

	/// The end-of-block work for `k` streams with one send each. Every frontier is wide, and every
	/// stream's path in the tree is `MaxTreeDepth` deep.
	#[benchmark(pov_mode = Measured)]
	fn commit_sends(k: Linear<0, 16>) {
		let sends: Vec<StreamId> = (0..k).map(deep_stream).collect();
		build_deep_tree::<T>(&sends);
		for stream in &sends {
			OutboundFrontier::<T>::insert(*stream, wide_frontier());
			queue_send::<T>(*stream);
		}
		mark_rolled_over::<T>();

		#[block]
		{
			Pallet::<T>::commit_streams_root();
		}

		assert_eq!(Pallet::<T>::current_streams_root().is_some(), k > 0);
	}

	/// The end-of-block work for `p` register publishes, each on its own stream, with a path
	/// `MaxTreeDepth` deep.
	#[benchmark(pov_mode = Measured)]
	fn commit_publishes(p: Linear<0, 16>) {
		let channels: Vec<InChannelId> = (0..p)
			.map(|i| InChannelId { sender: sibling::<T>(i.reverse_bits() >> 8), domain: 0, num: 0 })
			.collect();
		let acks: Vec<StreamId> = channels.iter().map(InChannelId::ack_stream).collect();
		build_deep_tree::<T>(&acks);
		for (channel, ack) in channels.iter().zip(&acks) {
			open_inbound::<T>(*channel);
			// Nothing published yet, so the block publishes the register.
			InChannels::<T>::mutate(channel, |state| {
				if let Some(state) = state {
					state.published = None;
				}
			});
			OutboundFrontier::<T>::insert(*ack, wide_frontier());
			PendingRegisterPublishes::<T>::mutate(|set| set.insert(*channel));
		}
		mark_rolled_over::<T>();

		#[block]
		{
			Pallet::<T>::commit_streams_root();
		}

		assert_eq!(Pallet::<T>::current_streams_root().is_some(), p > 0);
	}

	/// One send of `b` bytes on a stream of an open channel one message short of the in-flight
	/// cap, with a wide frontier.
	#[benchmark]
	fn send(b: Linear<0, { T::MaxMsgLen::get().saturating_sub(8) }>) {
		let channel = OutChannelId { recipient: sibling::<T>(0), domain: 0, num: 0 };
		open_outbound::<T>(channel);
		OutboundFrontier::<T>::insert(channel.data_stream(), wide_frontier());
		OutChannels::<T>::mutate(channel, |state| {
			if let Some(AppliedRegister { register, .. }) =
				state.as_mut().and_then(|s| s.register.as_mut())
			{
				register.up_to = MessagePosition((1 << 47) - 1);
			}
		});
		frame_system::Pallet::<T>::note_finished_initialize();
		mark_rolled_over::<T>();
		let data = vec![0u8; b as usize];

		#[block]
		{
			Pallet::<T>::send(channel, data).expect("the channel has credit; qed");
		}

		assert_eq!(OutboundLeafHashes::<T>::get(channel.data_stream()).len(), 1);
	}

	/// The end-of-block work of a block with nothing sent or published.
	#[benchmark]
	fn idle_commit() {
		mark_rolled_over::<T>();

		#[block]
		{
			Pallet::<T>::commit_streams_root();
		}

		assert!(Pallet::<T>::current_streams_root().is_none());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
