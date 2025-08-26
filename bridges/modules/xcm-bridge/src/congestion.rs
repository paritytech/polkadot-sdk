// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! The module contains utilities for handling congestion between the bridge hub and routers.

use crate::{Bridges, Config, DispatchChannelStatusProvider, LOG_TARGET};
use bp_xcm_bridge::{BridgeId, LocalXcmChannelManager, Receiver};
use codec::{Decode, Encode};
use sp_runtime::traits::Convert;
use sp_std::{marker::PhantomData, vec::Vec};
use xcm::latest::{send_xcm, Location, SendXcm, Xcm};
use xcm_builder::{DispatchBlob, DispatchBlobError};

/// Limits for handling congestion.
#[derive(Debug, Decode, Encode)]
pub struct CongestionLimits {
	/// Maximal number of messages in the outbound bridge queue. Once we reach this limit, we
	/// suspend a bridge.
	pub outbound_lane_congested_threshold: bp_messages::MessageNonce,
	/// After we have suspended the bridge, we wait until number of messages in the outbound bridge
	/// queue drops to this count, before sending resuming the bridge.
	pub outbound_lane_uncongested_threshold: bp_messages::MessageNonce,
	/// Maximal number of messages in the outbound bridge queue after we have suspended the bridge.
	/// Once we reach this limit, we stop exporting more messages.
	pub outbound_lane_stop_threshold: bp_messages::MessageNonce,
}

impl CongestionLimits {
	/// Checks if limits are valid.
	pub fn is_valid(&self) -> bool {
		self.outbound_lane_uncongested_threshold < self.outbound_lane_congested_threshold &&
			self.outbound_lane_stop_threshold > self.outbound_lane_congested_threshold
	}
}

impl Default for CongestionLimits {
	fn default() -> Self {
		Self {
			outbound_lane_congested_threshold: 8_192,
			outbound_lane_uncongested_threshold: 1_024,
			outbound_lane_stop_threshold: 12_288,
		}
	}
}

/// Switches the implementation of [`LocalXcmChannelManager`] based on the `local_origin`.
///
/// - `HereXcmChannelManager` is applied when the origin is `Here`.
/// - Otherwise, `LocalConsensusXcmChannelManager` is used.
///
/// This is useful when the `pallet-xcm-bridge` needs to support both:
/// - A local router deployed on the same chain as the `pallet-xcm-bridge`.
/// - A remote router deployed on a different chain than the `pallet-xcm-bridge`.
pub struct HereOrLocalConsensusXcmChannelManager<
	Bridge,
	HereXcmChannelManager,
	LocalConsensusXcmChannelManager,
>(PhantomData<(Bridge, HereXcmChannelManager, LocalConsensusXcmChannelManager)>);
impl<
		Bridge: Encode + sp_std::fmt::Debug + Copy,
		HereXcmChannelManager: LocalXcmChannelManager<Bridge>,
		LocalConsensusXcmChannelManager: LocalXcmChannelManager<Bridge>,
	> LocalXcmChannelManager<Bridge>
	for HereOrLocalConsensusXcmChannelManager<
		Bridge,
		HereXcmChannelManager,
		LocalConsensusXcmChannelManager,
	>
{
	type Error = ();

	fn suspend_bridge(local_origin: &Location, bridge: Bridge) -> Result<(), Self::Error> {
		if local_origin.eq(&Location::here()) {
			HereXcmChannelManager::suspend_bridge(local_origin, bridge).map_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"HereXcmChannelManager::suspend_bridge error: {e:?} for local_origin: {:?} and bridge: {:?}",
					local_origin,
					bridge,
				);
				()
			})
		} else {
			LocalConsensusXcmChannelManager::suspend_bridge(local_origin, bridge).map_err(|e| {
                log::error!(
                    target: LOG_TARGET,
					"LocalConsensusXcmChannelManager::suspend_bridge error: {e:?} for local_origin: {:?} and bridge: {:?}",
					local_origin,
					bridge,
				);
                ()
            })
		}
	}

	fn resume_bridge(local_origin: &Location, bridge: Bridge) -> Result<(), Self::Error> {
		if local_origin.eq(&Location::here()) {
			HereXcmChannelManager::resume_bridge(local_origin, bridge).map_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"HereXcmChannelManager::resume_bridge error: {e:?} for local_origin: {:?} and bridge: {:?}",
					local_origin,
					bridge,
				);
				()
			})
		} else {
			LocalConsensusXcmChannelManager::resume_bridge(local_origin, bridge).map_err(|e| {
                log::error!(
                    target: LOG_TARGET,
					"LocalConsensusXcmChannelManager::resume_bridge error: {e:?} for local_origin: {:?} and bridge: {:?}",
					local_origin,
					bridge,
				);
                ()
            })
		}
	}
}

/// Manages the local XCM channels by sending XCM messages with the `update_bridge_status` extrinsic
/// to the `local_origin`. The `XcmProvider` type converts the encoded call to `XCM`, which is then
/// sent by `XcmSender` to the `local_origin`. This is useful, for example, when a router with
/// [`xcm::prelude::ExportMessage`] is deployed on a different chain, and we want to control
/// congestion by sending XCMs.
pub struct UpdateBridgeStatusXcmChannelManager<T, I, XcmProvider, XcmSender>(
	PhantomData<(T, I, XcmProvider, XcmSender)>,
);
impl<T: Config<I>, I: 'static, XcmProvider: Convert<Vec<u8>, Xcm<()>>, XcmSender: SendXcm>
	UpdateBridgeStatusXcmChannelManager<T, I, XcmProvider, XcmSender>
{
	fn update_bridge_status(
		local_origin: &Location,
		bridge_id: BridgeId,
		is_congested: bool,
	) -> Result<(), ()> {
		// check the bridge and get `maybe_notify` callback.
		let bridge = Bridges::<T, I>::get(&bridge_id).ok_or(())?;
		let Some(Receiver { pallet_index, call_index }) = bridge.maybe_notify else {
			// `local_origin` did not set `maybe_notify`, so nothing to notify, so it is ok.
			return Ok(())
		};

		// constructing expected call
		let remote_runtime_call = (pallet_index, call_index, bridge_id, is_congested);
		// construct XCM
		let xcm = XcmProvider::convert(remote_runtime_call.encode());
		log::trace!(
			target: LOG_TARGET,
			"UpdateBridgeStatusXcmChannelManager is going to send status with is_congested: {:?} to the local_origin: {:?} and bridge: {:?} as xcm: {:?}",
			is_congested,
			local_origin,
			bridge,
			xcm,
		);

		// send XCM
		send_xcm::<XcmSender>(local_origin.clone(), xcm)
            .map(|result| {
                log::warn!(
                    target: LOG_TARGET,
					"UpdateBridgeStatusXcmChannelManager successfully sent status with is_congested: {:?} to the local_origin: {:?} and bridge: {:?} with result: {:?}",
                    is_congested,
					local_origin,
					bridge,
                    result,
				);
                ()
            })
            .map_err(|e| {
                log::error!(
                    target: LOG_TARGET,
					"UpdateBridgeStatusXcmChannelManager failed to send status with is_congested: {:?} to the local_origin: {:?} and bridge: {:?} with error: {:?}",
                    is_congested,
					local_origin,
					bridge,
                    e,
				);
                ()
            })
	}
}
impl<T: Config<I>, I: 'static, XcmProvider: Convert<Vec<u8>, Xcm<()>>, XcmSender: SendXcm>
	LocalXcmChannelManager<BridgeId>
	for UpdateBridgeStatusXcmChannelManager<T, I, XcmProvider, XcmSender>
{
	type Error = ();

	fn suspend_bridge(local_origin: &Location, bridge: BridgeId) -> Result<(), Self::Error> {
		Self::update_bridge_status(local_origin, bridge, true)
	}

	fn resume_bridge(local_origin: &Location, bridge: BridgeId) -> Result<(), Self::Error> {
		Self::update_bridge_status(local_origin, bridge, false)
	}
}

/// Adapter that ties together the [`DispatchBlob`] trait with the [`DispatchChannelStatusProvider`]
/// trait. The idea is that [`DispatchBlob`] triggers message dispatch/delivery on the receiver
/// side, while [`DispatchChannelStatusProvider`] provides a status check to ensure the dispatch
/// channel is active (not congested).
pub struct BlobDispatcherWithChannelStatus<ChannelDispatch, ChannelStatus>(
	PhantomData<(ChannelDispatch, ChannelStatus)>,
);
impl<ChannelDispatch: DispatchBlob, ChannelStatus> DispatchBlob
	for BlobDispatcherWithChannelStatus<ChannelDispatch, ChannelStatus>
{
	fn dispatch_blob(blob: Vec<u8>) -> Result<(), DispatchBlobError> {
		ChannelDispatch::dispatch_blob(blob)
	}
}
impl<ChannelDispatch, ChannelStatus: DispatchChannelStatusProvider> DispatchChannelStatusProvider
	for BlobDispatcherWithChannelStatus<ChannelDispatch, ChannelStatus>
{
	fn is_congested(with: &Location) -> bool {
		ChannelStatus::is_congested(with)
	}
}
