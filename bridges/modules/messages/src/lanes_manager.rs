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

use crate::{
	BridgedChainOf, Config, InboundLane, InboundLaneStorage, InboundLanes, OutboundLane,
	OutboundLaneStorage, OutboundLanes, OutboundMessages, StoredInboundLaneData,
	StoredMessagePayload,
};

use bp_messages::{
	target_chain::MessageDispatch, ChainWithMessages, InboundLaneData, LaneState, MessageKey,
	MessageNonce, OutboundLaneData,
};
use bp_runtime::AccountIdOf;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{ensure, sp_runtime::RuntimeDebug, PalletError};
use scale_info::TypeInfo;
use sp_std::marker::PhantomData;

/// Lanes manager errors.
#[derive(
	Encode, Decode, DecodeWithMemTracking, RuntimeDebug, PartialEq, Eq, PalletError, TypeInfo,
)]
pub enum LanesManagerError {
	/// Inbound lane already exists.
	InboundLaneAlreadyExists,
	/// Outbound lane already exists.
	OutboundLaneAlreadyExists,
	/// No inbound lane with given id.
	UnknownInboundLane,
	/// No outbound lane with given id.
	UnknownOutboundLane,
	/// Inbound lane with given id is closed.
	ClosedInboundLane,
	/// Outbound lane with given id is closed.
	ClosedOutboundLane,
	/// Message dispatcher is inactive at given inbound lane. This is logical equivalent
	/// of the [`Self::ClosedInboundLane`] variant.
	LaneDispatcherInactive,
}

/// Message lanes manager.
pub struct LanesManager<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> Default for LanesManager<T, I> {
	fn default() -> Self {
		Self::new()
	}
}

impl<T: Config<I>, I: 'static> LanesManager<T, I> {
	/// Create new lanes manager.
	pub fn new() -> Self {
		Self(PhantomData)
	}

	/// Create new inbound lane in `Opened` state.
	pub fn create_inbound_lane(
		&self,
		lane_id: T::LaneId,
	) -> Result<InboundLane<RuntimeInboundLaneStorage<T, I>>, LanesManagerError> {
		InboundLanes::<T, I>::try_mutate(lane_id, |lane| match lane {
			Some(_) => Err(LanesManagerError::InboundLaneAlreadyExists),
			None => {
				*lane = Some(StoredInboundLaneData(InboundLaneData {
					state: LaneState::Opened,
					..Default::default()
				}));
				Ok(())
			},
		})?;

		self.active_inbound_lane(lane_id)
	}

	/// Create new outbound lane in `Opened` state.
	pub fn create_outbound_lane(
		&self,
		lane_id: T::LaneId,
	) -> Result<OutboundLane<RuntimeOutboundLaneStorage<T, I>>, LanesManagerError> {
		OutboundLanes::<T, I>::try_mutate(lane_id, |lane| match lane {
			Some(_) => Err(LanesManagerError::OutboundLaneAlreadyExists),
			None => {
				*lane = Some(OutboundLaneData { state: LaneState::Opened, ..Default::default() });
				Ok(())
			},
		})?;

		self.active_outbound_lane(lane_id)
	}

	/// Get existing inbound lane, checking that it is in usable state.
	pub fn active_inbound_lane(
		&self,
		lane_id: T::LaneId,
	) -> Result<InboundLane<RuntimeInboundLaneStorage<T, I>>, LanesManagerError> {
		Ok(InboundLane::new(RuntimeInboundLaneStorage::from_lane_id(lane_id, true)?))
	}

	/// Get existing outbound lane, checking that it is in usable state.
	pub fn active_outbound_lane(
		&self,
		lane_id: T::LaneId,
	) -> Result<OutboundLane<RuntimeOutboundLaneStorage<T, I>>, LanesManagerError> {
		Ok(OutboundLane::new(RuntimeOutboundLaneStorage::from_lane_id(lane_id, true)?))
	}

	/// Get existing inbound lane without any additional state checks.
	pub fn any_state_inbound_lane(
		&self,
		lane_id: T::LaneId,
	) -> Result<InboundLane<RuntimeInboundLaneStorage<T, I>>, LanesManagerError> {
		Ok(InboundLane::new(RuntimeInboundLaneStorage::from_lane_id(lane_id, false)?))
	}

	/// Get existing outbound lane without any additional state checks.
	pub fn any_state_outbound_lane(
		&self,
		lane_id: T::LaneId,
	) -> Result<OutboundLane<RuntimeOutboundLaneStorage<T, I>>, LanesManagerError> {
		Ok(OutboundLane::new(RuntimeOutboundLaneStorage::from_lane_id(lane_id, false)?))
	}
}

/// Runtime inbound lane storage.
pub struct RuntimeInboundLaneStorage<T: Config<I>, I: 'static = ()> {
	pub(crate) lane_id: T::LaneId,
	pub(crate) cached_data: InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>,
}

impl<T: Config<I>, I: 'static> RuntimeInboundLaneStorage<T, I> {
	/// Creates new runtime inbound lane storage for given **existing** lane.
	fn from_lane_id(
		lane_id: T::LaneId,
		check_active: bool,
	) -> Result<RuntimeInboundLaneStorage<T, I>, LanesManagerError> {
		let cached_data =
			InboundLanes::<T, I>::get(lane_id).ok_or(LanesManagerError::UnknownInboundLane)?;

		if check_active {
			// check that the lane is not explicitly closed
			ensure!(cached_data.state.is_active(), LanesManagerError::ClosedInboundLane);
			// apart from the explicit closure, the lane may be unable to receive any messages.
			// Right now we do an additional check here, but it may be done later (e.g. by
			// explicitly closing the lane and reopening it from
			// `pallet-xcm-bridge-hub::on-initialize`)
			//
			// The fact that we only check it here, means that the `MessageDispatch` may switch
			// to inactive state during some message dispatch in the middle of message delivery
			// transaction. But we treat result of `MessageDispatch::is_active()` as a hint, so
			// we know that it won't drop messages - just it experiences problems with processing.
			// This would allow us to check that in our signed extensions, and invalidate
			// transaction early, thus avoiding losing honest relayers funds. This problem should
			// gone with relayers coordination protocol.
			//
			// There's a limit on number of messages in the message delivery transaction, so even
			// if we dispatch (enqueue) some additional messages, we'll know the maximal queue
			// length;
			ensure!(
				T::MessageDispatch::is_active(lane_id),
				LanesManagerError::LaneDispatcherInactive
			);
		}

		Ok(RuntimeInboundLaneStorage { lane_id, cached_data: cached_data.into() })
	}

	/// Returns number of bytes that may be subtracted from the PoV component of
	/// `receive_messages_proof` call, because the actual inbound lane state is smaller than the
	/// maximal configured.
	///
	/// Maximal inbound lane state set size is configured by the
	/// `MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX` constant from the pallet configuration. The PoV
	/// of the call includes the maximal size of inbound lane state. If the actual size is smaller,
	/// we may subtract extra bytes from this component.
	pub fn extra_proof_size_bytes(&self) -> u64 {
		let max_encoded_len = StoredInboundLaneData::<T, I>::max_encoded_len();
		let relayers_count = self.data().relayers.len();
		let actual_encoded_len =
			InboundLaneData::<AccountIdOf<BridgedChainOf<T, I>>>::encoded_size_hint(relayers_count)
				.unwrap_or(usize::MAX);
		max_encoded_len.saturating_sub(actual_encoded_len) as _
	}
}

impl<T: Config<I>, I: 'static> InboundLaneStorage for RuntimeInboundLaneStorage<T, I> {
	type Relayer = AccountIdOf<BridgedChainOf<T, I>>;
	type LaneId = T::LaneId;

	fn id(&self) -> Self::LaneId {
		self.lane_id
	}

	fn max_unrewarded_relayer_entries(&self) -> MessageNonce {
		BridgedChainOf::<T, I>::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX
	}

	fn max_unconfirmed_messages(&self) -> MessageNonce {
		BridgedChainOf::<T, I>::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX
	}

	fn data(&self) -> InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>> {
		self.cached_data.clone()
	}

	fn set_data(&mut self, data: InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>) {
		self.cached_data = data.clone();
		InboundLanes::<T, I>::insert(self.lane_id, StoredInboundLaneData::<T, I>(data))
	}

	fn purge(self) {
		InboundLanes::<T, I>::remove(self.lane_id)
	}
}

/// Runtime outbound lane storage.
#[derive(Debug, PartialEq, Eq)]
pub struct RuntimeOutboundLaneStorage<T: Config<I>, I: 'static> {
	pub(crate) lane_id: T::LaneId,
	pub(crate) cached_data: OutboundLaneData,
	pub(crate) _phantom: PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> RuntimeOutboundLaneStorage<T, I> {
	/// Creates new runtime outbound lane storage for given **existing** lane.
	fn from_lane_id(lane_id: T::LaneId, check_active: bool) -> Result<Self, LanesManagerError> {
		let cached_data =
			OutboundLanes::<T, I>::get(lane_id).ok_or(LanesManagerError::UnknownOutboundLane)?;
		ensure!(
			!check_active || cached_data.state.is_active(),
			LanesManagerError::ClosedOutboundLane
		);
		Ok(Self { lane_id, cached_data, _phantom: PhantomData })
	}
}

impl<T: Config<I>, I: 'static> OutboundLaneStorage for RuntimeOutboundLaneStorage<T, I> {
	type StoredMessagePayload = StoredMessagePayload<T, I>;
	type LaneId = T::LaneId;

	fn id(&self) -> Self::LaneId {
		self.lane_id
	}

	fn data(&self) -> OutboundLaneData {
		self.cached_data.clone()
	}

	fn set_data(&mut self, data: OutboundLaneData) {
		self.cached_data = data.clone();
		OutboundLanes::<T, I>::insert(self.lane_id, data)
	}

	#[cfg(test)]
	fn message(&self, nonce: &MessageNonce) -> Option<Self::StoredMessagePayload> {
		OutboundMessages::<T, I>::get(MessageKey { lane_id: self.lane_id, nonce: *nonce })
			.map(Into::into)
	}

	fn save_message(&mut self, nonce: MessageNonce, message_payload: Self::StoredMessagePayload) {
		OutboundMessages::<T, I>::insert(
			MessageKey { lane_id: self.lane_id, nonce },
			message_payload,
		);
	}

	fn remove_message(&mut self, nonce: &MessageNonce) {
		OutboundMessages::<T, I>::remove(MessageKey { lane_id: self.lane_id, nonce: *nonce });
	}

	fn purge(self) {
		OutboundLanes::<T, I>::remove(self.lane_id)
	}
}
