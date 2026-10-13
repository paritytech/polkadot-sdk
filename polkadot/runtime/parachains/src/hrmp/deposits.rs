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
//! Every call that takes or returns a deposit goes through [`Config::DepositProvider`] and is
//! completed once the deposits are set: right away if the provider sets them within the call,
//! or in [`OnDepositSet::on_deposit_set`] if it holds the funds on another chain and reports
//! back in a later block.
//!
//! [`Pallet`] is itself a provider that holds the deposits in [`Config::Currency`].

use super::*;

impl<T: Config> ChannelDepositProvider for Pallet<T> {
	type OnDepositSet = Self;

	fn try_set_deposit(
		channel_id: ChannelId,
		changes: DepositChanges,
		_action: DepositAction,
	) -> Result<DepositStatus, DispatchError> {
		let channel_id = from_shared(channel_id);
		for (para, change) in
			[(channel_id.sender, changes.sender), (channel_id.recipient, changes.recipient)]
		{
			let Some(DepositChange { held, target }) = change else { continue };
			let account: T::AccountId = para.into_account_truncating();
			if target > held {
				T::Currency::reserve(&account, (target - held).unique_saturated_into())?;
			} else {
				T::Currency::unreserve(&account, (held - target).unique_saturated_into());
			}
		}
		Ok(DepositStatus::Set)
	}
}

impl<T: Config> OnDepositSet for Pallet<T> {
	fn on_deposit_set(
		channel_id: ChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> Weight {
		let channel_id = from_shared(channel_id);
		if let Err(e) = Self::complete(&channel_id, changes, action) {
			log::error!(
				target: "runtime::hrmp",
				"completing {action:?} on {channel_id:?} failed: {e:?}"
			);
		}
		// Bounded by the weight of the call that asked for the deposit.
		match action {
			DepositAction::InitOpenChannel { .. } => {
				<T as Config>::WeightInfo::hrmp_init_open_channel()
			},
			DepositAction::AcceptOpenChannel => {
				<T as Config>::WeightInfo::hrmp_accept_open_channel()
			},
			DepositAction::PokeChannelDeposits => {
				<T as Config>::WeightInfo::poke_channel_deposits()
			},
			DepositAction::Release => Weight::zero(),
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Deposits the sender and the recipient must hold for `channel_id` at the prices in
	/// `config`. Channels with or amongst the system are free.
	pub(super) fn required_deposits(
		channel_id: &HrmpChannelId,
		config: &HostConfiguration<BlockNumberFor<T>>,
	) -> (Balance, Balance) {
		if channel_id.sender.is_system() || channel_id.recipient.is_system() {
			return (0, 0);
		}
		(config.hrmp_sender_deposit, config.hrmp_recipient_deposit)
	}

	/// Asks [`Config::DepositProvider`] for `changes` and completes `action` if they are set
	/// within the call. Otherwise `action` is completed in [`OnDepositSet::on_deposit_set`].
	pub(super) fn set_deposit(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> DispatchResult {
		match T::DepositProvider::try_set_deposit(to_shared(channel_id), changes, action)? {
			DepositStatus::Set => Self::complete(channel_id, changes, action),
			DepositStatus::Pending => Ok(()),
		}
	}

	/// Asks [`Config::DepositProvider`] to give deposits back, for paths that cannot return an
	/// error. A failure is logged and the deposits are left with the provider.
	pub(super) fn release_deposit(channel_id: &HrmpChannelId, changes: DepositChanges) {
		if let Err(e) = Self::set_deposit(channel_id, changes, DepositAction::Release) {
			log::error!(
				target: "runtime::hrmp",
				"releasing the deposits of {channel_id:?} failed: {e:?}"
			);
		}
	}

	/// Completes the call that asked for `action` once the deposits are set to `changes`.
	fn complete(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> DispatchResult {
		match action {
			DepositAction::InitOpenChannel { max_capacity, max_message_size, max_total_size } => {
				// `init_open_channel` always changes the sender's side.
				let sender_deposit = changes.sender.map(|c| c.target).unwrap_or_default();
				Self::record_open_channel_request(
					channel_id,
					sender_deposit,
					max_capacity,
					max_message_size,
					max_total_size,
				);
				Ok(())
			},
			DepositAction::AcceptOpenChannel => Self::confirm_open_channel_request(channel_id),
			DepositAction::PokeChannelDeposits => {
				Self::record_channel_deposits(channel_id, changes)
			},
			DepositAction::Release => Ok(()),
		}
	}

	fn record_open_channel_request(
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
		HrmpOpenChannelRequestsList::<T>::append(channel_id);

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

	fn confirm_open_channel_request(channel_id: &HrmpChannelId) -> DispatchResult {
		let config = configuration::ActiveConfig::<T>::get();
		let (sender, recipient) = (channel_id.sender, channel_id.recipient);

		HrmpOpenChannelRequests::<T>::try_mutate(channel_id, |channel_req| -> DispatchResult {
			let channel_req =
				channel_req.as_mut().ok_or(Error::<T>::AcceptHrmpChannelDoesntExist)?;
			channel_req.confirmed = true;
			Ok(())
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

	fn record_channel_deposits(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
	) -> DispatchResult {
		HrmpChannels::<T>::try_mutate(channel_id, |channel| {
			let channel = channel.as_mut().ok_or(Error::<T>::OpenHrmpChannelDoesntExist)?;
			if let Some(change) = changes.sender {
				channel.sender_deposit = change.target;
			}
			if let Some(change) = changes.recipient {
				channel.recipient_deposit = change.target;
			}
			Ok(())
		})
	}
}

fn from_shared(channel_id: ChannelId) -> HrmpChannelId {
	HrmpChannelId { sender: channel_id.sender.into(), recipient: channel_id.recipient.into() }
}

fn to_shared(channel_id: &HrmpChannelId) -> ChannelId {
	ChannelId { sender: channel_id.sender.into(), recipient: channel_id.recipient.into() }
}
