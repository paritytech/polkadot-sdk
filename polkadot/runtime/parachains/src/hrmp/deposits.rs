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

//! Channel deposits.
//!
//! The pallet never reserves a deposit itself. Every call that takes or gives back one asks
//! [`Config::DepositProvider`] through [`ChannelDepositProvider::try_set_deposit`] and is
//! finished in [`OnDepositSet::on_deposit_set`] once the deposits are set, which lets a provider
//! on another chain finish the call later.
//!
//! When the deposits live on the relay chain the pallet is its own provider: [`Pallet`]
//! implements [`ChannelDepositProvider`] on [`Config::Currency`] and finishes the call right away.

use super::*;

fn from_primitive(channel_id: &ChannelId) -> HrmpChannelId {
	HrmpChannelId { sender: channel_id.sender.into(), recipient: channel_id.recipient.into() }
}

fn to_primitive(channel_id: &HrmpChannelId) -> ChannelId {
	ChannelId { sender: channel_id.sender.into(), recipient: channel_id.recipient.into() }
}

impl<T: Config> Pallet<T> {
	/// What the sender and the recipient owe for `channel_id` at the current prices. Channels
	/// with or amongst the system are free.
	pub(super) fn deposits_for(channel_id: &HrmpChannelId) -> (Balance, Balance) {
		if channel_id.sender.is_system() || channel_id.recipient.is_system() {
			return (0, 0);
		}
		let config = configuration::ActiveConfig::<T>::get();
		(config.hrmp_sender_deposit, config.hrmp_recipient_deposit)
	}

	/// What the sender and the recipient hold on `channel_id` by this pallet's records.
	fn recorded_deposits(channel_id: &HrmpChannelId) -> (Balance, Balance) {
		if let Some(channel) = HrmpChannels::<T>::get(channel_id) {
			return (channel.sender_deposit, channel.recipient_deposit);
		}
		match HrmpOpenChannelRequests::<T>::get(channel_id) {
			// The recipient's deposit is only recorded once the channel opens. Until then it is
			// what the accept cost, which is the current price unless the config changed since.
			Some(request) if request.confirmed => {
				(request.sender_deposit, Self::deposits_for(channel_id).1)
			},
			Some(request) => (request.sender_deposit, 0),
			None => (0, 0),
		}
	}

	/// Asks [`Config::DepositProvider`] to set `targets` on `channel_id` for `action`.
	pub(super) fn set_deposit(
		channel_id: &HrmpChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) -> DispatchResult {
		T::DepositProvider::try_set_deposit(to_primitive(channel_id), targets, action)
	}

	/// [`Self::set_deposit`] for callers that have nowhere to return an error to.
	pub(super) fn set_deposit_or_log(
		channel_id: &HrmpChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) {
		if let Err(e) = Self::set_deposit(channel_id, targets, action.clone()) {
			log::error!(
				target: "runtime::hrmp",
				"setting deposits of {channel_id:?} for {action:?} failed: {e:?}"
			);
		}
	}

	/// Finishes the call that asked for `action` once its deposits are set to `targets`.
	fn finish_deposit_action(
		channel_id: &HrmpChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) -> DispatchResult {
		match action {
			DepositAction::InitOpenChannel { max_capacity, max_message_size, max_total_size } => {
				Self::do_init_open_channel(
					channel_id,
					// A side left alone holds nothing.
					targets.sender.unwrap_or_default(),
					max_capacity,
					max_message_size,
					max_total_size,
				);
				Ok(())
			},
			DepositAction::AcceptOpenChannel => Self::do_accept_open_channel(channel_id),
			DepositAction::PokeChannelDeposits => {
				Self::do_poke_channel_deposits(channel_id, targets)
			},
			DepositAction::CancelOpenRequest => Self::do_cancel_open_request(channel_id),
			DepositAction::CloseChannel => {
				Self::do_close_hrmp_channel(channel_id);
				Ok(())
			},
		}
	}

	/// Records the open channel request once the sender's deposit is set.
	fn do_init_open_channel(
		channel_id: &HrmpChannelId,
		sender_deposit: Balance,
		max_capacity: u32,
		max_message_size: u32,
		max_total_size: u32,
	) {
		let config = configuration::ActiveConfig::<T>::get();
		let (sender, recipient) = (channel_id.sender, channel_id.recipient);

		HrmpOpenChannelRequestCount::<T>::mutate(&sender, |count| *count += 1);
		HrmpOpenChannelRequests::<T>::insert(
			channel_id,
			HrmpOpenChannelRequest {
				confirmed: false,
				_age: 0,
				sender_deposit,
				max_capacity,
				max_message_size,
				max_total_size,
			},
		);
		HrmpOpenChannelRequestsList::<T>::append(channel_id.clone());

		Self::send_to_para(
			"init_open_channel",
			&config,
			recipient,
			Self::wrap_notification(|| {
				use xcm::opaque::latest::{prelude::*, Xcm};
				Xcm(vec![HrmpNewChannelOpenRequest {
					sender: sender.into(),
					max_capacity,
					max_message_size,
				}])
			}),
		);
	}

	/// Confirms the open channel request once the recipient's deposit is set.
	fn do_accept_open_channel(channel_id: &HrmpChannelId) -> DispatchResult {
		let config = configuration::ActiveConfig::<T>::get();
		let (sender, recipient) = (channel_id.sender, channel_id.recipient);

		// persist the updated open channel request and then increment the number of accepted
		// channels.
		HrmpOpenChannelRequests::<T>::try_mutate(channel_id, |channel_req| {
			let channel_req =
				channel_req.as_mut().ok_or(Error::<T>::AcceptHrmpChannelDoesntExist)?;
			channel_req.confirmed = true;
			DispatchResult::Ok(())
		})?;
		HrmpAcceptedChannelRequestCount::<T>::mutate(&recipient, |count| *count += 1);

		Self::send_to_para(
			"accept_open_channel",
			&config,
			sender,
			Self::wrap_notification(|| {
				use xcm::opaque::latest::{prelude::*, Xcm};
				Xcm(vec![HrmpChannelAccepted { recipient: recipient.into() }])
			}),
		);

		Ok(())
	}

	/// Records the deposits of an open channel once they are set.
	fn do_poke_channel_deposits(
		channel_id: &HrmpChannelId,
		targets: DepositTargets,
	) -> DispatchResult {
		HrmpChannels::<T>::try_mutate(channel_id, |channel| {
			let channel = channel.as_mut().ok_or(Error::<T>::OpenHrmpChannelDoesntExist)?;
			channel.sender_deposit = targets.sender.unwrap_or(channel.sender_deposit);
			channel.recipient_deposit = targets.recipient.unwrap_or(channel.recipient_deposit);
			Ok(())
		})
	}

	/// Drops the open channel request once its deposits are given back.
	fn do_cancel_open_request(channel_id: &HrmpChannelId) -> DispatchResult {
		let request = HrmpOpenChannelRequests::<T>::take(channel_id)
			.ok_or(Error::<T>::OpenHrmpChannelDoesntExist)?;

		// Sync the accompanying list with the set.
		HrmpOpenChannelRequestsList::<T>::mutate(|open_req_channels| {
			if let Some(pos) = open_req_channels.iter().position(|x| x == channel_id) {
				open_req_channels.swap_remove(pos);
			}
		});

		Self::decrease_open_channel_request_count(channel_id.sender);
		if request.confirmed {
			Self::decrease_accepted_channel_request_count(channel_id.recipient);
		}

		Ok(())
	}

	/// Removes the channel once its deposits are given back.
	fn do_close_hrmp_channel(channel_id: &HrmpChannelId) {
		HrmpChannels::<T>::remove(channel_id);
		HrmpChannelContents::<T>::remove(channel_id);

		HrmpEgressChannelsIndex::<T>::mutate(&channel_id.sender, |v| {
			if let Ok(i) = v.binary_search(&channel_id.recipient) {
				v.remove(i);
			}
		});
		HrmpIngressChannelsIndex::<T>::mutate(&channel_id.recipient, |v| {
			if let Ok(i) = v.binary_search(&channel_id.sender) {
				v.remove(i);
			}
		});
	}
}

/// The provider for deposits held on the relay chain, in [`Config::Currency`].
impl<T: Config> ChannelDepositProvider for Pallet<T> {
	type OnDepositSet = Self;

	fn try_set_deposit(
		channel_id: ChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) -> DispatchResult {
		let channel_id = from_primitive(&channel_id);
		let (held_sender, held_recipient) = Self::recorded_deposits(&channel_id);
		for (para, held, target) in [
			(channel_id.sender, held_sender, targets.sender),
			(channel_id.recipient, held_recipient, targets.recipient),
		] {
			let Some(target) = target else { continue };
			let account: T::AccountId = para.into_account_truncating();
			if target > held {
				T::Currency::reserve(&account, (target - held).unique_saturated_into())?;
			} else {
				T::Currency::unreserve(&account, (held - target).unique_saturated_into());
			}
		}
		// Finished within the same call rather than reported through `on_deposit_set`, so that an
		// error reaches the caller and reverts the reserve along with the rest of the call.
		Self::finish_deposit_action(&channel_id, targets, action)
	}
}

impl<T: Config> OnDepositSet for Pallet<T> {
	fn on_deposit_set(
		channel_id: ChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) -> Weight {
		let channel_id = from_primitive(&channel_id);
		// Bounded by the weight of the call that asked for the deposit.
		let weight = match action {
			DepositAction::InitOpenChannel { .. } => {
				<T as Config>::WeightInfo::hrmp_init_open_channel()
			},
			DepositAction::AcceptOpenChannel => {
				<T as Config>::WeightInfo::hrmp_accept_open_channel()
			},
			DepositAction::PokeChannelDeposits => {
				<T as Config>::WeightInfo::poke_channel_deposits()
			},
			DepositAction::CancelOpenRequest => {
				<T as Config>::WeightInfo::hrmp_cancel_open_request(
					HrmpOpenChannelRequestsList::<T>::decode_len().unwrap_or_default() as u32,
				)
			},
			DepositAction::CloseChannel => <T as Config>::WeightInfo::force_process_hrmp_close(1),
		};
		if let Err(e) = Self::finish_deposit_action(&channel_id, targets, action.clone()) {
			log::error!(
				target: "runtime::hrmp",
				"finishing {action:?} on {channel_id:?} failed: {e:?}"
			);
		}
		weight
	}
}
