// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Relay-chain message sending performed by `on_finalize`.

use alloc::vec::Vec;
use codec::{Compact, Encode};
use core::cmp;
use cumulus_primitives_core::{
	AbridgedHostConfiguration, CoreInfo, CumulusDigestItem, OutboundHrmpMessage,
	PersistedValidationData, UpwardMessage, XcmpMessageSource,
};
use polkadot_runtime_parachains::FeeTracker;

use crate::{
	relay_chain::{BlockNumber, UMPSignal, UpgradeGoAhead, UMP_SEPARATOR},
	ump_constants,
	unincluded_segment::{HrmpChannelUpdate, HrmpWatermarkUpdate, OutboundBandwidthLimits},
	AggregatedUnincludedSegment, Ancestor, AnnouncedHrmpMessagesPerCandidate, Config,
	HrmpWatermark, Pallet, PendingApprovedPeer, PendingUpwardMessages, PendingUpwardSignals,
	PoVMessages, PoVMessagesTracker, PreviousCoreCount, RelevantMessagingState, UnincludedSegment,
	UpwardMessages, UsedBandwidth,
};

/// Read and refresh the PoV message tracker for the current block.
pub(crate) fn refresh_pov_tracker<T: Config>(vfp: &PersistedValidationData) -> PoVMessages {
	let current_core_selector =
		CumulusDigestItem::find_core_info(&frame_system::Pallet::<T>::digest())
			.map_or(0, |ci| ci.selector.0);

	let current_bundle_index =
		CumulusDigestItem::find_block_bundle_info(&frame_system::Pallet::<T>::digest())
			.map_or(0, |bi| bi.index);

	let mut pov_tracker = PoVMessagesTracker::<T>::get()
		.filter(|tracker| {
			// If the relay parent changes, this is for sure a different `PoV`.
			tracker.relay_storage_root_or_hash == vfp.relay_parent_storage_root &&
			// A different core selector also means we are on a different `PoV`.
			tracker.core_selector == current_core_selector &&
			// The bundle index needs to increase, or we are in a different `PoV`.
			current_bundle_index > tracker.bundle_index
		})
		.unwrap_or_default();

	pov_tracker.bundle_index = current_bundle_index;
	pov_tracker.core_selector = current_core_selector;
	pov_tracker.relay_storage_root_or_hash = vfp.relay_parent_storage_root;

	pov_tracker
}

/// Move pending upward messages into `UpwardMessages`, respecting relay-chain capacity.
///
/// Decreases the delivery fee factor if after sending messages, the queue total size is less than
/// the threshold (see [`ump_constants::THRESHOLD_FACTOR`]).
pub(crate) fn send_ump_messages<T: Config>(
	pov_tracker: &mut PoVMessages,
	host_config: &AbridgedHostConfiguration,
) -> (u32, u32) {
	<PendingUpwardMessages<T>>::mutate(|up| {
		let (available_capacity, available_size) = match RelevantMessagingState::<T>::get() {
			Some(limits) => (
				limits.relay_dispatch_queue_remaining_capacity.remaining_count,
				limits.relay_dispatch_queue_remaining_capacity.remaining_size,
			),
			None => {
				debug_assert!(
					false,
					"relevant messaging state is promised to be set until `on_finalize`; \
						qed",
				);
				return (0, 0);
			},
		};

		let available_capacity = cmp::min(
			available_capacity,
			host_config
				.max_upward_message_num_per_candidate
				.saturating_sub(pov_tracker.ump_msg_count),
		);

		// Count the number of messages we can possibly fit in the given constraints, i.e.
		// available_capacity and available_size.
		let (num, total_size) = up
			.iter()
			.scan((0u32, 0u32), |state, msg| {
				let (cap_used, size_used) = *state;
				let new_cap = cap_used.saturating_add(1);
				let new_size = size_used.saturating_add(msg.len() as u32);
				match available_capacity
					.checked_sub(new_cap)
					.and(available_size.checked_sub(new_size))
				{
					Some(_) => {
						*state = (new_cap, new_size);
						Some(*state)
					},
					_ => None,
				}
			})
			.last()
			.unwrap_or_default();

		// TODO: #274 Return back messages that do not longer fit into the queue.

		UpwardMessages::<T>::put(&up[..num as usize]);
		*up = up.split_off(num as usize);

		pov_tracker.ump_msg_count = pov_tracker.ump_msg_count.saturating_add(num);

		let digest = frame_system::Pallet::<T>::digest();

		let core_info = CumulusDigestItem::find_core_info(&digest);
		PreviousCoreCount::<T>::put(
			core_info.as_ref().map_or(Compact(1u16), |ci| ci.number_of_cores),
		);

		// Only send UMP signals on the last block of a PoV.
		// For single-block PoVs (no BlockBundleInfo), always send signals.
		if CumulusDigestItem::is_last_block_in_core(&digest).unwrap_or(true) {
			send_ump_signals::<T>(core_info);
		}

		// If the total size of the pending messages is less than the threshold,
		// we decrease the fee factor, since the queue is less congested.
		// This makes delivery of new messages cheaper.
		let threshold = host_config
			.max_upward_queue_size
			.saturating_div(ump_constants::THRESHOLD_FACTOR);
		let remaining_total_size: usize = up.iter().map(UpwardMessage::len).sum();
		if remaining_total_size <= threshold as usize {
			Pallet::<T>::decrease_fee_factor(());
		}

		(num, total_size)
	})
}

/// Send HRMP messages from the outbound message source.
pub(crate) fn send_hrmp_messages<T: Config>(
	pov_tracker: &mut PoVMessages,
	host_config: &AbridgedHostConfiguration,
) -> Vec<OutboundHrmpMessage> {
	let maximum_channels = host_config
		.hrmp_max_message_num_per_candidate
		.min(<AnnouncedHrmpMessagesPerCandidate<T>>::take()) as usize;

	let maximum_channels =
		maximum_channels.saturating_sub(pov_tracker.hrmp_outbound_count as usize);

	// Note: this internally calls the `GetChannelInfo` implementation for this
	// pallet, which draws on the `RelevantMessagingState`. That in turn has
	// been adjusted above to reflect the correct limits in all channels.
	let outbound_messages = T::OutboundXcmpMessageSource::take_outbound_messages(
		maximum_channels,
		&pov_tracker.hrmp_outbound_recipients,
	)
	.into_iter()
	.map(|(recipient, data)| OutboundHrmpMessage { recipient, data })
	.collect::<Vec<_>>();

	pov_tracker
		.hrmp_outbound_recipients
		.extend(outbound_messages.iter().map(|m| m.recipient));
	pov_tracker.hrmp_outbound_count =
		pov_tracker.hrmp_outbound_count.saturating_add(outbound_messages.len() as u32);
	PoVMessagesTracker::<T>::put(pov_tracker);

	outbound_messages
}

/// Update the unincluded segment with the bandwidth used this block.
pub(crate) fn update_unincluded_segment<T: Config>(
	outbound_messages: &[OutboundHrmpMessage],
	ump_msg_count: u32,
	ump_total_bytes: u32,
	relay_upgrade_go_ahead: Option<UpgradeGoAhead>,
	relay_parent_number: BlockNumber,
	total_bandwidth_out: &OutboundBandwidthLimits,
) {
	let hrmp_outgoing = outbound_messages
		.iter()
		.map(|msg| {
			(msg.recipient, HrmpChannelUpdate { msg_count: 1, total_bytes: msg.data.len() as u32 })
		})
		.collect();
	let used_bandwidth = UsedBandwidth { ump_msg_count, ump_total_bytes, hrmp_outgoing };

	let mut aggregated_segment = AggregatedUnincludedSegment::<T>::get().unwrap_or_default();
	let consumed_go_ahead_signal = if aggregated_segment.consumed_go_ahead_signal().is_some() {
		// Some ancestor within the segment already processed this signal --
		// validated during inherent creation.
		None
	} else {
		relay_upgrade_go_ahead
	};
	// The bandwidth constructed was ensured to satisfy relay chain constraints.
	let ancestor = Ancestor::new_unchecked(used_bandwidth, consumed_go_ahead_signal);

	let watermark = HrmpWatermark::<T>::get();
	let watermark_update = HrmpWatermarkUpdate::new(watermark, relay_parent_number);

	aggregated_segment
		.append(&ancestor, watermark_update, total_bandwidth_out)
		.expect("unincluded segment limits exceeded");
	AggregatedUnincludedSegment::<T>::put(aggregated_segment);
	// Check in `on_initialize` guarantees there's space for this block.
	UnincludedSegment::<T>::append(ancestor);
}

/// Send the pending ump signals.
pub(crate) fn send_ump_signals<T: Config>(core_info: Option<CoreInfo>) {
	let mut ump_signals = PendingUpwardSignals::<T>::take();

	if let Some(core_info) = core_info {
		ump_signals
			.push(UMPSignal::SelectCore(core_info.selector, core_info.claim_queue_offset).encode());
	}

	if let Some(approved_peer) = PendingApprovedPeer::<T>::take() {
		ump_signals.push(UMPSignal::ApprovedPeer(approved_peer).encode());
	}

	if !ump_signals.is_empty() {
		UpwardMessages::<T>::append(UMP_SEPARATOR);
		ump_signals.into_iter().for_each(|s| UpwardMessages::<T>::append(s));
	}
}
