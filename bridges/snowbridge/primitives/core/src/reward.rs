// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use sp_runtime::traits::Debug;
use xcm::latest::{NetworkId, Location, Asset, Junction::GlobalConsensus};

pub trait PaymentProcedure<AccountId> {
	/// Error that may be returned by the procedure.
	type Error: Debug;

	/// Pay reward to the relayer from the account with provided params.
	fn pay_reward(
		relayer: AccountId,
		reward: Asset,
	) -> Result<(), Self::Error>;
}

impl<AccountId> PaymentProcedure<AccountId> for () {
	type Error = &'static str;

	fn pay_reward(
		_: AccountId,
		_: Asset,
	) -> Result<(), Self::Error> {
		Ok(())
	}
}

/// XCM asset descriptor for native ether relative to AH
pub fn ether_asset(network: NetworkId, value: u128) -> Asset {
	(
		Location::new(
			2,
			[
				GlobalConsensus(network),
			],
		),
		value
	).into()
}
