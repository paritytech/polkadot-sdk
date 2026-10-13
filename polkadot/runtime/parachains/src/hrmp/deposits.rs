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
//! This pallet does not reserve deposits itself. Each call that takes or gives back one sends a
//! [`DepositChangeRequest`] to [`Config::DepositProvider`]. If the provider applies the request
//! within the call, the call completes right away. If the provider holds the funds on another
//! chain, it answers pending and the call completes in [`OnDepositChanged::on_deposit_changed`]
//! once the provider reports back.
//!
//! The pallet implements [`ChannelDepositProvider`] itself to hold the deposits in
//! [`Config::Currency`].

use super::*;

/// Reserves and releases the channel deposits of this pallet.
///
/// Implemented by this pallet to hold the deposits in [`Config::Currency`], applying every
/// request within the call. A provider that holds them on another chain instead answers
/// [`DepositChangeResponse::Pending`] and reports through [`Self::OnDepositChanged`] once the
/// funds are moved. Until then it must refuse another request for the same side of the same
/// channel.
pub trait ChannelDepositProvider {
	/// Receives the report of a pending request once it is done.
	type OnDepositChanged: OnDepositChanged;

	/// Applies `request`, or accepts it to apply it later.
	fn request_deposit_change(
		request: DepositChangeRequest,
	) -> Result<DepositChangeResponse, DispatchError>;
}

/// Receives the report that a pending [`DepositChangeRequest`] is done.
///
/// Implemented by this pallet, which completes the call named in
/// [`DepositChangeRequest::action`].
pub trait OnDepositChanged {
	/// Called by the provider once the deposits are changed as `request` asked. Returns the
	/// weight spent completing the call.
	fn on_deposit_changed(request: DepositChangeRequest) -> Weight;
}

/// A change to the deposits of one channel, asked for by one call of this pallet.
///
/// Sent to [`ChannelDepositProvider::request_deposit_change`] and handed back unchanged in
/// [`OnDepositChanged::on_deposit_changed`] if it was pending, so the pallet knows which call to
/// complete.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct DepositChangeRequest {
	/// The channel whose deposits change.
	pub channel_id: HrmpChannelId,
	/// How the sender's deposit changes, or `None` to leave it as is.
	pub sender: Option<DepositChange>,
	/// How the recipient's deposit changes, or `None` to leave it as is.
	pub recipient: Option<DepositChange>,
	/// The call that asked for the change. Not for the provider to interpret.
	pub action: DepositAction,
}

/// What one side of a channel holds before and after a [`DepositChangeRequest`].
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DepositChange {
	/// What the side holds now, by this pallet's records.
	pub current: Balance,
	/// What the side holds once the request is done.
	pub target: Balance,
}

/// The provider's answer to a [`DepositChangeRequest`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum DepositChangeResponse {
	/// The request is applied. The call completes right away.
	Applied,
	/// The request is accepted and is reported through [`OnDepositChanged`] once applied.
	Pending,
}

/// The call that asked for a [`DepositChangeRequest`], with what it needs to complete.
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum DepositAction {
	/// [`Pallet::init_open_channel`], with the limits the open request is recorded with.
	InitOpenChannel { max_capacity: u32, max_message_size: u32, max_total_size: u32 },
	/// [`Pallet::accept_open_channel`].
	AcceptOpenChannel,
	/// [`Pallet::poke_channel_deposits`].
	PokeChannelDeposits,
	/// A call giving deposits back. Nothing to complete.
	Release,
}

impl<T: Config> ChannelDepositProvider for Pallet<T> {
	type OnDepositChanged = Self;

	fn request_deposit_change(
		request: DepositChangeRequest,
	) -> Result<DepositChangeResponse, DispatchError> {
		let DepositChangeRequest { channel_id, sender, recipient, .. } = request;
		for (para, change) in [(channel_id.sender, sender), (channel_id.recipient, recipient)] {
			let Some(DepositChange { current, target }) = change else { continue };
			let account: T::AccountId = para.into_account_truncating();
			if target > current {
				T::Currency::reserve(&account, (target - current).unique_saturated_into())?;
			} else {
				T::Currency::unreserve(&account, (current - target).unique_saturated_into());
			}
		}
		Ok(DepositChangeResponse::Applied)
	}
}

impl<T: Config> OnDepositChanged for Pallet<T> {
	fn on_deposit_changed(request: DepositChangeRequest) -> Weight {
		Self::complete_call(&request)
	}
}

impl<T: Config> Pallet<T> {
	/// What the sender and the recipient must each hold for `channel_id` at the prices in
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

	/// Sends `request` to [`Config::DepositProvider`] and completes its call if the provider
	/// applied it right away. A pending request completes in
	/// [`OnDepositChanged::on_deposit_changed`] instead.
	pub(super) fn request_deposit_change(request: DepositChangeRequest) -> DispatchResult {
		if T::DepositProvider::request_deposit_change(request.clone())? ==
			DepositChangeResponse::Applied
		{
			Self::complete_call(&request);
		}
		Ok(())
	}

	/// [`Self::request_deposit_change`] for giving deposits back from paths that cannot return
	/// an error. A failure is logged and the deposits stay with the provider.
	pub(super) fn release_deposits(request: DepositChangeRequest) {
		if let Err(e) = Self::request_deposit_change(request.clone()) {
			log::error!(
				target: "runtime::hrmp",
				"giving back deposits failed: {e:?}, request: {request:?}"
			);
		}
	}

	/// Completes the call that asked for `request`, now that its deposits are changed. Returns
	/// the weight spent, bounded by the weight of that call.
	fn complete_call(request: &DepositChangeRequest) -> Weight {
		let channel_id = &request.channel_id;
		match request.action {
			DepositAction::InitOpenChannel { max_capacity, max_message_size, max_total_size } => {
				// Always set by `init_open_channel`.
				let sender_deposit = request.sender.map(|c| c.target).unwrap_or_default();
				Self::finish_init_open_channel(
					channel_id,
					sender_deposit,
					max_capacity,
					max_message_size,
					max_total_size,
				);
				<T as Config>::WeightInfo::hrmp_init_open_channel()
			},
			DepositAction::AcceptOpenChannel => {
				Self::finish_accept_open_channel(channel_id);
				<T as Config>::WeightInfo::hrmp_accept_open_channel()
			},
			DepositAction::PokeChannelDeposits => {
				Self::finish_poke_channel_deposits(channel_id, request.sender, request.recipient);
				<T as Config>::WeightInfo::poke_channel_deposits()
			},
			DepositAction::Release => Weight::zero(),
		}
	}
}
