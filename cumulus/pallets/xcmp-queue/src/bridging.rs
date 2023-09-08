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

use crate::pallet;
use cumulus_primitives_core::ParaId;
use frame_support::pallet_prelude::Get;

/// Adapter implementation for `bp_xcm_bridge_hub_router::XcmChannelStatusProvider` which checks
/// both `OutboundXcmpStatus` and `InboundXcmpStatus` for defined `ParaId` if any of those is
/// suspended.
pub struct InboundAndOutboundXcmpChannelCongestionStatusProvider<SiblingBridgeHubParaId, Runtime>(
	sp_std::marker::PhantomData<(SiblingBridgeHubParaId, Runtime)>,
);
impl<SiblingBridgeHubParaId: Get<ParaId>, Runtime: crate::Config>
	bp_xcm_bridge_hub_router::XcmChannelStatusProvider
	for InboundAndOutboundXcmpChannelCongestionStatusProvider<SiblingBridgeHubParaId, Runtime>
{
	fn is_congested() -> bool {
		// if the outbound channel with recipient is suspended, it means that one of further
		// bridge queues (e.g. bridge queue between two bridge hubs) is overloaded, so we shall
		// take larger fee for our outbound messages
		let sibling_bridge_hub_id: ParaId = SiblingBridgeHubParaId::get();
		let outbound_channels = pallet::OutboundXcmpStatus::<Runtime>::get();
		let outbound_channel =
			outbound_channels.iter().find(|c| c.recipient == sibling_bridge_hub_id);
		let is_outbound_channel_suspended =
			outbound_channel.map(|c| c.is_suspended()).unwrap_or(false);
		if is_outbound_channel_suspended {
			return true
		}

		// if the inbound channel with recipient is suspended, it means that we are unable to
		// receive congestion reports from the bridge hub. So we assume the bridge pipeline is
		// congested too
		let inbound_channels = pallet::InboundXcmpStatus::<Runtime>::get();
		let inbound_channel = inbound_channels.iter().find(|c| c.sender == sibling_bridge_hub_id);
		let is_inbound_channel_suspended =
			inbound_channel.map(|c| c.is_suspended()).unwrap_or(false);
		if is_inbound_channel_suspended {
			return true
		}

		// TODO: https://github.com/paritytech/cumulus/pull/2342 - once this PR is merged, we may
		// remove the following code
		//
		// if the outbound channel has at least `N` pages enqueued, let's assume it is congested.
		// Normally, the chain with a few opened HRMP channels, will "send" pages at every block.
		// Having `N` pages means that for last `N` blocks we either have not sent any messages,
		// or have sent signals.
		const MAX_OUTBOUND_PAGES_BEFORE_CONGESTION: u16 = 4;
		let is_outbound_channel_congested = outbound_channel.map(|c| c.queued_pages()).unwrap_or(0);
		is_outbound_channel_congested > MAX_OUTBOUND_PAGES_BEFORE_CONGESTION
	}
}

/// Adapter implementation for `bp_xcm_bridge_hub_router::XcmChannelStatusProvider` which checks
/// only `OutboundXcmpStatus` for defined `SiblingParaId` if is suspended.
pub struct OutboundXcmpChannelCongestionStatusProvider<SiblingBridgeHubParaId, Runtime>(
	sp_std::marker::PhantomData<(SiblingBridgeHubParaId, Runtime)>,
);
impl<SiblingParaId: Get<ParaId>, Runtime: crate::Config>
	bp_xcm_bridge_hub_router::XcmChannelStatusProvider
	for OutboundXcmpChannelCongestionStatusProvider<SiblingParaId, Runtime>
{
	fn is_congested() -> bool {
		// let's find the channel with the sibling parachain
		let sibling_para_id: cumulus_primitives_core::ParaId = SiblingParaId::get();
		let outbound_channels = pallet::OutboundXcmpStatus::<Runtime>::get();
		let channel_with_sibling_parachain =
			outbound_channels.iter().find(|c| c.recipient == sibling_para_id);

		// no channel => it is empty, so not congested
		let channel_with_sibling_parachain = match channel_with_sibling_parachain {
			Some(channel_with_sibling_parachain) => channel_with_sibling_parachain,
			None => return false,
		};

		// suspended channel => it is congested
		if channel_with_sibling_parachain.is_suspended() {
			return true
		}

		// TODO: the following restriction is arguable, we may live without that, assuming that
		// there can't be more than some `N` messages queued at the bridge queue (at the source BH)
		// AND before accepting next (or next-after-next) delivery transaction, we'll receive the
		// suspension signal from the target parachain and stop accepting delivery transactions

		// it takes some time for target parachain to suspend inbound channel with the target BH and
		// during that we will keep accepting new message delivery transactions. Let's also reject
		// new deliveries if there are too many "pages" (concatenated XCM messages) in the target BH
		// -> target parachain queue.
		const MAX_QUEUED_PAGES_BEFORE_DEACTIVATION: u16 = 4;
		if channel_with_sibling_parachain.queued_pages() > MAX_QUEUED_PAGES_BEFORE_DEACTIVATION {
			return true
		}

		true
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub fn suspend_channel_for_benchmarks<T: crate::Config>(target: ParaId) {
	pallet::Pallet::<T>::suspend_channel(target)
}
