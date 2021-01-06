// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Implementation of `MessageDeliveryAndDispatchPayment` trait on top of `Currency` trait.
//!
//! The payment is first transferred to a special `relayers-fund` account and only transferred
//! to the actual relayer in case confirmation is received.

use bp_message_lane::source_chain::{MessageDeliveryAndDispatchPayment, Sender};
use frame_support::traits::{Currency as CurrencyT, ExistenceRequirement, Get};

/// Instant message payments made in given currency.
///
/// The balance is initally reserved in a special `relayers-fund` account, and transferred
/// to the relayer when message delivery is confirmed.
///
/// NOTE The `relayers-fund` account must always exist i.e. be over Existential Deposit (ED; the
/// pallet enforces that) to make sure that even if the message cost is below ED it is still payed
/// to the relayer account.
/// NOTE It's within relayer's interest to keep their balance above ED as well, to make sure they
/// can receive the payment.
pub struct InstantCurrencyPayments<T: frame_system::Config, Currency, RootAccount> {
	_phantom: sp_std::marker::PhantomData<(T, Currency, RootAccount)>,
}

impl<T, Currency, RootAccount> MessageDeliveryAndDispatchPayment<T::AccountId, Currency::Balance>
	for InstantCurrencyPayments<T, Currency, RootAccount>
where
	T: frame_system::Config,
	Currency: CurrencyT<T::AccountId>,
	RootAccount: Get<Option<T::AccountId>>,
{
	type Error = &'static str;

	fn initialize(relayer_fund_account: &T::AccountId) -> usize {
		assert!(
			frame_system::Module::<T>::account_exists(relayer_fund_account),
			"The relayer fund account ({:?}) must exist for the message lanes pallet to work correctly.",
			relayer_fund_account,
		);
		1
	}

	fn pay_delivery_and_dispatch_fee(
		submitter: &Sender<T::AccountId>,
		fee: &Currency::Balance,
		relayer_fund_account: &T::AccountId,
	) -> Result<(), Self::Error> {
		let root_account = RootAccount::get();
		let account = match submitter {
			Sender::Signed(submitter) => submitter,
			Sender::Root | Sender::None => root_account
				.as_ref()
				.ok_or("Sending messages using Root or None origin is disallowed.")?,
		};

		Currency::transfer(
			account,
			relayer_fund_account,
			*fee,
			// it's fine for the submitter to go below Existential Deposit and die.
			ExistenceRequirement::AllowDeath,
		)
		.map_err(Into::into)
	}

	fn pay_relayer_reward(
		_confirmation_relayer: &T::AccountId,
		relayer: &T::AccountId,
		reward: &Currency::Balance,
		relayer_fund_account: &T::AccountId,
	) {
		let pay_result = Currency::transfer(
			&relayer_fund_account,
			relayer,
			*reward,
			// the relayer fund account must stay above ED (needs to be pre-funded)
			ExistenceRequirement::KeepAlive,
		);

		// we can't actually do anything here, because rewards are paid as a part of unrelated transaction
		match pay_result {
			Ok(_) => frame_support::debug::trace!(
				target: "runtime",
				"Rewarded relayer {:?} with {:?}",
				relayer,
				reward,
			),
			Err(error) => frame_support::debug::trace!(
				target: "runtime",
				"Failed to pay relayer {:?} reward {:?}: {:?}",
				relayer,
				reward,
				error,
			),
		}
	}
}
