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

use bp_message_lane::source_chain::MessageDeliveryAndDispatchPayment;
use bp_runtime::{bridge_account_id, MESSAGE_LANE_MODULE_PREFIX, NO_INSTANCE_ID};
use codec::Decode;
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
	AccountId: Debug + Default + Decode,
{
	type Error = &'static str;

	fn pay_delivery_and_dispatch_fee(submitter: &AccountId, fee: &Currency::Balance) -> Result<(), Self::Error> {
		Currency::transfer(
			submitter,
			&relayers_fund_account(),
			*fee,
			ExistenceRequirement::AllowDeath,
		)
		.map_err(Into::into)
	}

	fn pay_relayer_reward(_confirmation_relayer: &AccountId, relayer: &AccountId, reward: &Currency::Balance) {
		let pay_result = Currency::transfer(
			&relayers_fund_account(),
			relayer,
			*reward,
			ExistenceRequirement::AllowDeath,
		);

		// we can't actually do anything here, because rewards are paid as a part of unrelated transaction
		if let Err(error) = pay_result {
			frame_support::debug::trace!(
				target: "runtime",
				"Failed to pay relayer {:?} reward {:?}: {:?}",
				relayer,
				reward,
				error,
			);
		}
	}
}

/// Return account id of shared relayers-fund account that is storing all fees
/// paid by submitters, until they're claimed by relayers.
fn relayers_fund_account<AccountId: Default + Decode>() -> AccountId {
	bridge_account_id(NO_INSTANCE_ID, MESSAGE_LANE_MODULE_PREFIX)
}
