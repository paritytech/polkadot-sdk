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

/// Reserves and releases HRMP channel deposits on behalf of the pallet that owns the channels.
///
/// Implemented by whoever holds the funds: the owning pallet itself when they are on the relay
/// chain, or a pallet that forwards the request to another chain. The latter cannot set the
/// deposits within the call, so it accepts the request and reports the outcome through
/// [`Self::OnDepositSet`] later; until then it must refuse another request touching the same
/// side of the channel.
pub trait ChannelDepositProvider {
	/// Where the provider reports a pending request once its deposits are set.
	type OnDepositSet: OnDepositSet;

	/// Requests that every side of `channel_id` named in `changes` ends up holding its target.
	/// `action` is handed back in [`OnDepositSet::on_deposit_set`] if the request is pending.
	fn try_set_deposit(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> Result<DepositStatus, DispatchError>;
}

/// Callback through which a [`ChannelDepositProvider`] reports that the deposits of a pending
/// request are set.
///
/// Implemented by the pallet that owns the channels, which then completes the call that asked
/// for the change.
pub trait OnDepositSet {
	/// Called once every side of `channel_id` named in `changes` holds its target. `action` is
	/// the one passed to [`ChannelDepositProvider::try_set_deposit`]. Returns the weight spent.
	fn on_deposit_set(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> Weight;
}

/// Whether a deposit request is done when [`ChannelDepositProvider::try_set_deposit`] returns.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum DepositStatus {
	/// The deposits are set. Nothing will be reported.
	Set,
	/// The request is accepted and its outcome is reported through [`OnDepositSet`].
	Pending,
}

/// The deposit changes a call asks for, one per side of the channel. `None` leaves that side
/// untouched, e.g. only the sender's side changes when a channel is requested.
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DepositChanges {
	pub sender: Option<DepositChange>,
	pub recipient: Option<DepositChange>,
}

/// A change to the deposit one side of a channel holds.
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DepositChange {
	/// What the side holds now, by the records of the pallet that owns the channels.
	pub held: Balance,
	/// What the side must hold once the change is done.
	pub target: Balance,
}

/// Which HRMP call asked for a deposit change.
///
/// Passed to [`ChannelDepositProvider::try_set_deposit`] and handed back unchanged in
/// [`OnDepositSet::on_deposit_set`], so the pallet that owns the channels knows which call to
/// complete. A provider never inspects it.
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
	type OnDepositSet = Self;

	fn try_set_deposit(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		_action: DepositAction,
	) -> Result<DepositStatus, DispatchError> {
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
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> Weight {
		Self::complete(channel_id, changes, action)
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
		if T::DepositProvider::try_set_deposit(channel_id, changes, action)? == DepositStatus::Set {
			Self::complete(channel_id, changes, action);
		}
		Ok(())
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

	/// Completes the call that asked for `action` once the deposits are set to `changes`,
	/// returning the weight spent, bounded by the weight of that call.
	fn complete(
		channel_id: &HrmpChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> Weight {
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
				<T as Config>::WeightInfo::hrmp_init_open_channel()
			},
			DepositAction::AcceptOpenChannel => {
				Self::confirm_open_channel_request(channel_id);
				<T as Config>::WeightInfo::hrmp_accept_open_channel()
			},
			DepositAction::PokeChannelDeposits => {
				Self::record_channel_deposits(channel_id, changes);
				<T as Config>::WeightInfo::poke_channel_deposits()
			},
			DepositAction::Release => Weight::zero(),
		}
	}
}
