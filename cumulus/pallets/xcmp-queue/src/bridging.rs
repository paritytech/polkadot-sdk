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

use crate::{pallet, OutboundState};
use cumulus_primitives_core::ParaId;
use frame_support::pallet_prelude::Get;

/// Adapter implementation for `bp_xcm_bridge_hub_router::XcmChannelStatusProvider` which checks
/// both `OutboundXcmpStatus` and `InboundXcmpStatus` for defined `ParaId` if any of those is
/// suspended.
pub struct InAndOutXcmpChannelStatusProvider<SiblingBridgeHubParaId, Runtime>(
	sp_std::marker::PhantomData<(SiblingBridgeHubParaId, Runtime)>,
);
impl<SiblingBridgeHubParaId: Get<ParaId>, Runtime: crate::Config>
	bp_xcm_bridge_hub_router::XcmChannelStatusProvider
	for InAndOutXcmpChannelStatusProvider<SiblingBridgeHubParaId, Runtime>
{
	fn is_congested() -> bool {
		// if the inbound channel with recipient is suspended, it means that we are unable to
		// receive congestion reports from the bridge hub. So we assume the bridge pipeline is
		// congested too
		if pallet::Pallet::<Runtime>::is_inbound_channel_suspended(SiblingBridgeHubParaId::get()) {
			return true
		}

		// if the outbound channel with recipient is suspended, it means that one of further
		// bridge queues (e.g. bridge queue between two bridge hubs) is overloaded, so we shall
		// take larger fee for our outbound messages
		OutXcmpChannelStatusProvider::<SiblingBridgeHubParaId, Runtime>::is_congested()
	}
}

/// Adapter implementation for `bp_xcm_bridge_hub_router::XcmChannelStatusProvider` which checks
/// only `OutboundXcmpStatus` for defined `SiblingParaId` if is suspended.
pub struct OutXcmpChannelStatusProvider<SiblingBridgeHubParaId, Runtime>(
	sp_std::marker::PhantomData<(SiblingBridgeHubParaId, Runtime)>,
);
impl<SiblingBridgeHubParaId: Get<ParaId>, Runtime: crate::Config>
	bp_xcm_bridge_hub_router::XcmChannelStatusProvider
	for OutXcmpChannelStatusProvider<SiblingBridgeHubParaId, Runtime>
{
	fn is_congested() -> bool {
		let sibling_bridge_hub_id: ParaId = SiblingBridgeHubParaId::get();

		// let's find the channel's state with the sibling parachain,
		let Some((outbound_state, queued_pages)) =
			pallet::Pallet::<Runtime>::outbound_channel_state(sibling_bridge_hub_id)
		else {
			return false
		};
		// suspended channel => it is congested
		if outbound_state == OutboundState::Suspended {
			return true
		}

		// It takes some time for target parachain to suspend inbound channel with the target BH and
		// during that we will keep accepting new message delivery transactions. Let's also reject
		// new deliveries if there are too many "pages" (concatenated XCM messages) in the target BH
		// -> target parachain queue.

		// If the outbound channel has at least `N` pages enqueued, let's assume it is congested.
		// Normally, the chain with a few opened HRMP channels, will "send" pages at every block.
		// Having `N` pages means that for last `N` blocks we either have not sent any messages,
		// or have sent signals.

		const MAX_QUEUED_PAGES_BEFORE_DEACTIVATION: u16 = 4;
		if queued_pages > MAX_QUEUED_PAGES_BEFORE_DEACTIVATION {
			return true
		}

		false
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub fn suspend_channel_for_benchmarks<T: crate::Config>(target: ParaId) {
	pallet::Pallet::<T>::suspend_channel(target)
}
