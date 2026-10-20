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
//! The channel logic never touches the funds directly. Each call that takes or gives back a
//! deposit sends a [`DepositChangeRequest`] to [`Config::DepositProvider`]. If the provider
//! applies the request within the call, the call completes right away. If the provider holds the
//! funds on another chain, it answers pending and the call completes in
//! [`OnDepositSet::on_deposit_set`] once the provider reports back, or stops there if the
//! provider refused the request.
//!
//! The pallet implements [`ChannelDepositProvider`] itself to hold the deposits in
//! [`Config::Currency`], which keeps the behaviour from before the provider existed.

use super::*;

/// Holds HRMP channel deposits.
///
/// A provider either applies a request within the call or accepts it and reports the outcome
/// later through [`Self::OnDepositSet`]. While a request for one side of a channel is pending, it
/// refuses another request for that side.
pub trait ChannelDepositProvider {
	/// Receives the report of a pending request once it is done.
	type OnDepositSet: OnDepositSet;

	/// Applies `request`, or accepts it to apply it later. Refuses it with an error, e.g. when
	/// the funds are short or a request for the same side is still pending.
	fn try_set_deposit(
		request: DepositChangeRequest,
	) -> Result<DepositChangeResponse, DispatchError>;

	/// Upper bound of what [`Self::try_set_deposit`] spends on top of what the calls' benchmarks
	/// measure with the in-pallet provider, e.g. sending a message.
	fn weight_of_try_set_deposit() -> Weight;
}

/// Receives the outcome of a pending [`DepositChangeRequest`].
pub trait OnDepositSet {
	/// Called once `request` is applied, or refused with `Err`.
	fn on_deposit_set(request: DepositChangeRequest, result: Result<(), DepositError>);
}

/// Why a provider refused a pending [`DepositChangeRequest`].
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum DepositError {
	/// A side cannot cover its deposit.
	InsufficientFunds,
	/// A reason of the provider's own.
	Other,
}

/// A request to change the deposits of a channel, with the action to resume once they are
/// changed.
///
/// It carries everything the action needs, so the requester keeps no pending state of its own:
/// the request comes back unchanged in [`OnDepositSet::on_deposit_set`] and the action resumes
/// from it.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct DepositChangeRequest {
	/// The channel whose deposits change.
	pub channel_id: HrmpChannelId,
	/// How the sender's deposit changes, or `None` to leave it as is.
	pub sender: Option<DepositChange>,
	/// How the recipient's deposit changes, or `None` to leave it as is.
	pub recipient: Option<DepositChange>,
	/// The action to resume once the deposits are changed.
	pub action: DepositAction,
}

/// What one side of a channel holds before and after a [`DepositChangeRequest`].
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DepositChange {
	/// What the side holds now, by the requester's records. A provider with records of its own
	/// goes by those and acts on `target` alone.
	pub current: Balance,
	/// What the side holds once the request is done.
	pub target: Balance,
}

/// The provider's answer to a [`DepositChangeRequest`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum DepositChangeResponse {
	/// The request is applied. The action resumes right away.
	Applied,
	/// The request is accepted and its outcome is reported through [`OnDepositSet`].
	Pending,
}

/// The action a [`DepositChangeRequest`] resumes once the deposits are changed.
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum DepositAction {
	/// Record the open channel request of [`Pallet::init_open_channel`]. The request is only
	/// recorded once the sender's deposit is taken, so the limits it was asked with travel here.
	InitOpenChannel { max_capacity: u32, max_message_size: u32, max_total_size: u32 },
	/// Confirm the open channel request of [`Pallet::accept_open_channel`].
	AcceptOpenChannel,
	/// Record the deposits of [`Pallet::poke_channel_deposits`].
	PokeChannelDeposits,
	/// Deposits given back after their request or channel is already removed. Nothing to resume.
	Release,
}

/// The in-pallet provider: holds the deposits in [`Config::Currency`] and applies every request
/// within the call.
impl<T: Config> ChannelDepositProvider for Pallet<T> {
	type OnDepositSet = Self;

	fn try_set_deposit(
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

	fn weight_of_try_set_deposit() -> Weight {
		// Measured by the calls' benchmarks.
		Weight::zero()
	}
}

impl<T: Config> OnDepositSet for Pallet<T> {
	fn on_deposit_set(request: DepositChangeRequest, result: Result<(), DepositError>) {
		match result {
			Ok(()) => Self::resume(&request),
			// TODO(ahm-v2): emit an event (later on hrmp_relay) so the para learns its deposit was
			// refused.
			Err(e) => {
				log::warn!(target: LOG_TARGET, "deposit refused: {e:?}, request: {request:?}")
			},
		}
		// TODO(ahm-v2): the report arrives through xcm message so the weight of resuming is
		// registered here: per action, the weight of the call it belongs to. Zero for now as no
		// async provider.
		frame_system::Pallet::<T>::register_extra_weight_unchecked(
			Weight::zero(),
			DispatchClass::Normal,
		);
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

	/// Sends `request` to [`Config::DepositProvider`] and resumes its action if the provider
	/// applied it right away. A pending request resumes in [`OnDepositSet::on_deposit_set`]
	/// instead. Fails only if the provider refuses the request right away.
	pub(super) fn set_deposit(request: DepositChangeRequest) -> DispatchResult {
		// Nothing to change, e.g. on a channel with the system, needs no provider.
		let unchanged =
			|change: Option<DepositChange>| change.is_none_or(|c| c.current == c.target);
		if unchanged(request.sender) && unchanged(request.recipient) {
			Self::resume(&request);
			return Ok(());
		}
		match T::DepositProvider::try_set_deposit(request.clone())? {
			DepositChangeResponse::Applied => Self::resume(&request),
			DepositChangeResponse::Pending => {},
		}
		Ok(())
	}

	/// Gives back the deposits of `channel_id`: `sender` and `recipient` are what each side holds
	/// by this pallet's records, or `None` to leave that side alone. A refused release is logged.
	pub(super) fn release_deposits(
		channel_id: &HrmpChannelId,
		sender: Option<Balance>,
		recipient: Option<Balance>,
	) {
		let release = |current| DepositChange { current, target: 0 };
		let request = DepositChangeRequest {
			channel_id: channel_id.clone(),
			sender: sender.map(release),
			recipient: recipient.map(release),
			action: DepositAction::Release,
		};
		if let Err(e) = Self::set_deposit(request.clone()) {
			// TODO(ahm-v2): emit an event (later on hrmp_relay) if release fails.
			log::error!(target: LOG_TARGET, "giving back deposits failed: {e:?}, request: {request:?}");
		}
	}

	/// Resumes the action of `request` now that its deposits are updated.
	fn resume(request: &DepositChangeRequest) {
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
			},
			DepositAction::AcceptOpenChannel => Self::finish_accept_open_channel(channel_id),
			DepositAction::PokeChannelDeposits => {
				Self::finish_poke_channel_deposits(channel_id, request.sender, request.recipient)
			},
			DepositAction::Release => {},
		}
	}
}
