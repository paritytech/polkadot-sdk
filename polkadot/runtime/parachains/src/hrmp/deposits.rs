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
use frame_support::transactional;

/// Holds HRMP channel deposits.
///
/// A provider either applies a request within the call or accepts it and reports the outcome
/// later through [`Self::OnDepositSet`]. While a request for one side of a channel is pending, it
/// refuses another request for that side.
///
/// A request whose action is [`DepositAction::Release`] must not fail: the provider always
/// applies it. It gives back everything the provider holds for each side the request names,
/// whatever the request says the side holds.
///
/// A provider that answers later keeps its own records of what each side holds, and applies the
/// requests for a side in the order they were made, releases included: a release made while a
/// request for the side is pending takes effect after that request does, so that it gives back
/// what the request took.
pub trait ChannelDepositProvider {
	/// Receives the report of a pending request once it is done.
	type OnDepositSet: OnDepositSet;

	/// Applies `request`, or accepts it to apply it later. Refuses it with an error, e.g. when
	/// the funds are short or a request for the same side is still pending. A refused request
	/// changes nothing. A [`DepositAction::Release`] is always successfully applied, never pending.
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
	///
	/// The worse-case weight of this call is [`Self::weight_of_on_deposit_set`].
	fn on_deposit_set(request: DepositChangeRequest, result: Result<(), DepositError>);

	/// Worse-case weight of [`Self::on_deposit_set`] for an `action`.
	fn weight_of_on_deposit_set(action: &DepositAction) -> Weight;
}

/// Why a provider refused a pending [`DepositChangeRequest`].
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
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
	/// Record and accept the open channel request of [`Pallet::init_and_accept_open_channel`] once
	/// both deposits are taken, so that it is opened whole or not at all.
	InitAndAcceptOpenChannel { max_capacity: u32, max_message_size: u32, max_total_size: u32 },
	/// Confirm the open channel request of [`Pallet::accept_open_channel`]. The request may be
	/// canceled and made anew before the recipient's deposit is taken, so the limits it was
	/// accepted with travel here: only a request with the same ones is confirmed.
	AcceptOpenChannel { max_capacity: u32, max_message_size: u32 },
	/// Record the deposits of [`Pallet::poke_channel_deposits`].
	PokeChannelDeposits,
	/// Deposits given back after their request or channel is already removed. Nothing to resume.
	Release,
}

/// The in-pallet provider: holds the deposits in [`Config::Currency`] and applies every request
/// within the call.
impl<T: Config> ChannelDepositProvider for Pallet<T> {
	type OnDepositSet = Self;

	// The recipient's side may fail after the sender's is changed.
	#[transactional]
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
		let channel_id = request.channel_id.clone();
		if let Err(error) = result {
			Self::deposit_event(Event::DepositRefused { channel_id, error });
			return;
		}

		// The state may have changed since the request was made. A failed check leaves nothing
		// behind, so only what was just taken needs giving back.
		if let Err(error) = Self::resume(&request) {
			Self::release_deposits(
				&channel_id,
				request.sender.map(|c| c.target),
				request.recipient.map(|c| c.target),
			);
			Self::deposit_event(Event::DepositReturned { channel_id, error });
		}
	}

	/// The action's call, then giving back what was taken if its checks no longer pass.
	fn weight_of_on_deposit_set(action: &DepositAction) -> Weight {
		let resume = match action {
			DepositAction::InitOpenChannel { .. } => {
				<T as Config>::WeightInfo::hrmp_init_open_channel()
			},
			DepositAction::InitAndAcceptOpenChannel { .. } => {
				<T as Config>::WeightInfo::force_open_hrmp_channel(0)
			},
			DepositAction::AcceptOpenChannel { .. } => {
				<T as Config>::WeightInfo::hrmp_accept_open_channel()
			},
			DepositAction::PokeChannelDeposits => {
				<T as Config>::WeightInfo::poke_channel_deposits()
			},
			DepositAction::Release => Weight::zero(),
		};
		resume.saturating_add(T::DepositProvider::weight_of_try_set_deposit())
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
	/// instead.
	///
	/// Fails if the provider refuses the request right away, or if the action fails when it
	/// resumes right away.
	///
	/// A release request is always successful.
	#[transactional]
	pub(super) fn set_deposit(request: DepositChangeRequest) -> DispatchResult {
		// Nothing to change, e.g. on a channel with the system, needs no provider.
		let unchanged =
			|change: Option<DepositChange>| change.is_none_or(|c| c.current == c.target);
		if unchanged(request.sender) && unchanged(request.recipient) {
			return Self::resume(&request);
		}
		match T::DepositProvider::try_set_deposit(request.clone())? {
			DepositChangeResponse::Applied => Self::resume(&request),
			DepositChangeResponse::Pending => Ok(()),
		}
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
			// A release request is always successful, per `set_deposit` doc.
			frame_support::defensive!("giving back deposits failed", (e, &request));
		}
	}

	/// Resumes the action of `request` now that its deposits are updated. Its checks run again,
	/// since a pending request may come back after the state has changed.
	///
	/// Writes nothing if it fails: every check runs before the first write.
	///
	/// A release request always succeeds.
	fn resume(request: &DepositChangeRequest) -> DispatchResult {
		let channel_id = &request.channel_id;
		match request.action {
			DepositAction::InitOpenChannel { max_capacity, max_message_size, max_total_size } => {
				let config = configuration::ActiveConfig::<T>::get();
				Self::check_init_open_channel(channel_id, max_capacity, max_message_size, &config)?;
				// Always set by `init_open_channel`.
				let sender_deposit = request.sender.map(|c| c.target).unwrap_or_default();
				Self::finish_init_open_channel(
					channel_id,
					sender_deposit,
					max_capacity,
					max_message_size,
					max_total_size,
				);
				Ok(())
			},
			DepositAction::InitAndAcceptOpenChannel {
				max_capacity,
				max_message_size,
				max_total_size,
			} => {
				let config = configuration::ActiveConfig::<T>::get();
				Self::check_init_open_channel(channel_id, max_capacity, max_message_size, &config)?;
				Self::check_accept_limit(channel_id.recipient, &config)?;
				Self::check_recipient_deposit(request, &config)?;
				// Always set by `init_and_accept_open_channel`.
				let sender_deposit = request.sender.map(|c| c.target).unwrap_or_default();
				Self::finish_init_open_channel(
					channel_id,
					sender_deposit,
					max_capacity,
					max_message_size,
					max_total_size,
				);
				Self::finish_accept_open_channel(channel_id);
				let HrmpChannelId { sender, recipient } = *channel_id;
				Self::deposit_event(Event::HrmpChannelForceOpened {
					sender,
					recipient,
					proposed_max_capacity: max_capacity,
					proposed_max_message_size: max_message_size,
				});
				Ok(())
			},
			DepositAction::AcceptOpenChannel { max_capacity, max_message_size } => {
				let config = configuration::ActiveConfig::<T>::get();
				let channel_req = Self::check_accept_open_channel(channel_id, &config)?;
				// The request may have been canceled and made anew with other limits meanwhile.
				ensure!(
					(channel_req.max_capacity, channel_req.max_message_size) ==
						(max_capacity, max_message_size),
					Error::<T>::AcceptHrmpChannelRequestChanged
				);
				Self::check_recipient_deposit(request, &config)?;
				Self::finish_accept_open_channel(channel_id);
				Ok(())
			},
			DepositAction::PokeChannelDeposits => {
				Self::check_poke_channel_deposits(channel_id, request.sender, request.recipient)?;
				Self::finish_poke_channel_deposits(channel_id, request.sender, request.recipient);
				Ok(())
			},
			DepositAction::Release => Ok(()),
		}
	}

	/// Whether the recipient's deposit taken by `request` is still the price in `config`.
	///
	/// The channel does not record the deposit its recipient paid: it is opened at the session
	/// change with the price of `config`, which cannot change before then. But a pending request
	/// may come back after a session change, with a deposit taken at an older price. This is not
	/// expected often, as the price rarely changes.
	fn check_recipient_deposit(
		request: &DepositChangeRequest,
		config: &HostConfiguration<BlockNumberFor<T>>,
	) -> DispatchResult {
		let taken = request.recipient.map(|c| c.target).unwrap_or_default();
		let (_, price) = Self::required_deposits(&request.channel_id, config);
		ensure!(taken == price, Error::<T>::AcceptHrmpChannelDepositChanged);
		Ok(())
	}
}
