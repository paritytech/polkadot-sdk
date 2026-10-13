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
//! Every call that takes or returns a deposit sends a [`DepositChangeRequest`] to
//! [`Config::DepositProvider`] and completes once the request is done: right away if the
//! provider applies it within the call, or in [`OnDepositChanged::on_deposit_changed`] if the
//! provider holds the funds on another chain and reports back in a later block.
//!
//! [`Pallet`] is itself a provider that holds the deposits in [`Config::Currency`].

use super::*;

/// Holds the channel deposits for this pallet.
///
/// The pallet itself implements it to hold them in [`Config::Currency`], in which case every
/// request is applied within the call. A provider that holds them on another chain answers
/// [`DepositChangeResponse::Pending`] and reports through [`Self::OnDepositChanged`] once the
/// funds are moved. Until then it must refuse another request for the same side of the same
/// channel.
pub trait ChannelDepositProvider {
	/// Where a pending request is reported once done.
	type OnDepositChanged: OnDepositChanged;

	/// Changes the deposits as `request` says, now or later.
	fn request_deposit_change(
		request: DepositChangeRequest,
	) -> Result<DepositChangeResponse, DispatchError>;
}

/// How a [`ChannelDepositProvider`] reports that a pending request is done.
///
/// The pallet implements it and completes the call named in [`DepositChangeRequest::action`].
pub trait OnDepositChanged {
	/// Called once the deposits are changed as `request` asked. Returns the weight spent.
	fn on_deposit_changed(request: DepositChangeRequest) -> Weight;
}

/// What a call asks the provider to do with the deposits of one channel.
///
/// Built by the pallet, handed to [`ChannelDepositProvider::request_deposit_change`], and handed
/// back unchanged in [`OnDepositChanged::on_deposit_changed`] once a pending request is done, so
/// the pallet knows which call to complete.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct DepositChangeRequest {
	pub channel_id: HrmpChannelId,
	/// Change for the sender, or `None` to leave its deposit as is.
	pub sender: Option<DepositChange>,
	/// Change for the recipient, or `None` to leave its deposit as is.
	pub recipient: Option<DepositChange>,
	/// The call that made the request. The provider passes it through untouched.
	pub action: DepositAction,
}

/// How the deposit of one side of a channel must change.
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DepositChange {
	/// What the side holds now, according to the pallet's own records.
	pub current: Balance,
	/// What the side must hold once the request is done.
	pub target: Balance,
}

/// What the provider answers to a [`DepositChangeRequest`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum DepositChangeResponse {
	/// The deposits are changed. The call completes right away.
	Applied,
	/// The provider accepted the request and will report through [`OnDepositChanged`].
	Pending,
}

/// The call that made a [`DepositChangeRequest`], and what it needs to complete.
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum DepositAction {
	/// A new open channel request, carrying the limits it is recorded with.
	InitOpenChannel { max_capacity: u32, max_message_size: u32, max_total_size: u32 },
	/// The recipient accepting an open channel request.
	AcceptOpenChannel,
	/// Bringing the deposits of an open channel in line with the current prices.
	PokeChannelDeposits,
	/// Giving deposits back. Nothing to complete.
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

	/// Sends `request` to [`Config::DepositProvider`] and completes its call if the provider
	/// applied it within the call. A pending request is completed in
	/// [`OnDepositChanged::on_deposit_changed`] instead.
	pub(super) fn request_deposit_change(request: DepositChangeRequest) -> DispatchResult {
		if T::DepositProvider::request_deposit_change(request.clone())? ==
			DepositChangeResponse::Applied
		{
			Self::complete_call(&request);
		}
		Ok(())
	}

	/// [`Self::request_deposit_change`] for giving deposits back on paths that cannot return an
	/// error. A failure is logged and the deposits are left with the provider.
	pub(super) fn release_deposits(request: DepositChangeRequest) {
		if let Err(e) = Self::request_deposit_change(request.clone()) {
			log::error!(target: "runtime::hrmp", "releasing deposits failed: {e:?} for {request:?}");
		}
	}

	/// Completes the call that made `request` once its deposits are changed, returning the
	/// weight spent, bounded by the weight of that call.
	fn complete_call(request: &DepositChangeRequest) -> Weight {
		let channel_id = &request.channel_id;
		match request.action {
			DepositAction::InitOpenChannel { max_capacity, max_message_size, max_total_size } => {
				// `init_open_channel` always changes the sender's side.
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
