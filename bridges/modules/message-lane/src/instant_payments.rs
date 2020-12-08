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
//! All payments are instant.

use bp_message_lane::source_chain::{MessageDeliveryAndDispatchPayment, Sender};
use codec::Encode;
use frame_support::traits::{Currency as CurrencyT, ExistenceRequirement};
use sp_std::fmt::Debug;

/// Instant message payments made in given currency. Until claimed, fee is stored in special
/// 'relayers-fund' account.
pub struct InstantCurrencyPayments<AccountId, Currency> {
	_phantom: sp_std::marker::PhantomData<(AccountId, Currency)>,
}

impl<AccountId, Currency> MessageDeliveryAndDispatchPayment<AccountId, Currency::Balance>
	for InstantCurrencyPayments<AccountId, Currency>
where
	Currency: CurrencyT<AccountId>,
	AccountId: Debug + Default + Encode,
{
	type Error = &'static str;

	fn pay_delivery_and_dispatch_fee(
		submitter: &Sender<AccountId>,
		fee: &Currency::Balance,
		relayer_fund_account: &AccountId,
	) -> Result<(), Self::Error> {
		match submitter {
			Sender::Signed(submitter) => {
				Currency::transfer(submitter, relayer_fund_account, *fee, ExistenceRequirement::AllowDeath)
					.map_err(Into::into)
			}
			Sender::Root | Sender::None => {
				// fixme: we might want to add root account id to this struct.
				Err("Root and None account is not allowed to send regular messages.")
			}
		}
	}

	fn pay_relayer_reward(
		_confirmation_relayer: &AccountId,
		relayer: &AccountId,
		reward: &Currency::Balance,
		relayer_fund_account: &AccountId,
	) {
		let pay_result = Currency::transfer(
			&relayer_fund_account,
			relayer,
			*reward,
			ExistenceRequirement::AllowDeath,
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
