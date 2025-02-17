// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Tools for supporting message lanes between two Substrate-based chains.

use crate::TaggedAccount;

use bp_relayers::{RewardsAccountOwner, RewardsAccountParams};
use codec::{Decode, EncodeLike};
use frame_system::AccountInfo;
use messages_relay::Labeled;
use pallet_balances::AccountData;
use relay_substrate_client::{
	metrics::{FixedU128OrOne, FloatStorageValue, FloatStorageValueMetric},
	AccountIdOf, BalanceOf, Chain, ChainWithBalances, ChainWithMessages, ChainWithRewards, Client,
	Error as SubstrateError, NonceOf,
};
use relay_utils::metrics::{MetricsParams, StandaloneMetric};
use sp_core::storage::StorageData;
use sp_runtime::{FixedPointNumber, FixedU128};
use std::{fmt::Debug, marker::PhantomData};

/// Add relay accounts balance metrics.
pub async fn add_relay_balances_metrics<C: ChainWithBalances>(
	client: impl Client<C>,
	metrics: &MetricsParams,
	relay_accounts: &Vec<TaggedAccount<AccountIdOf<C>>>,
) -> anyhow::Result<()>
where
	BalanceOf<C>: Into<u128> + std::fmt::Debug,
{
	if relay_accounts.is_empty() {
		return Ok(())
	}

	// if `tokenDecimals` is missing from system properties, we'll be using
	let token_decimals = client
		.token_decimals()
		.await?
		.inspect(|token_decimals| {
			log::info!(target: "bridge", "Read `tokenDecimals` for {}: {}", C::NAME, token_decimals);
		})
		.unwrap_or_else(|| {
			// turns out it is normal not to have this property - e.g. when polkadot binary is
			// started using `polkadot-local` chain. Let's use minimal nominal here
			log::info!(target: "bridge", "Using default (zero) `tokenDecimals` value for {}", C::NAME);
			0
		});
	let token_decimals = u32::try_from(token_decimals).map_err(|e| {
		anyhow::format_err!(
			"Token decimals value ({}) of {} doesn't fit into u32: {:?}",
			token_decimals,
			C::NAME,
			e,
		)
	})?;

	for account in relay_accounts {
		let relay_account_balance_metric = FloatStorageValueMetric::new(
			AccountBalanceFromAccountInfo::<C> { token_decimals, _phantom: Default::default() },
			client.clone(),
			C::account_info_storage_key(account.id()),
			format!("at_{}_relay_{}_balance", C::NAME, account.tag()),
			format!("Balance of the {} relay account at the {}", account.tag(), C::NAME),
		)?;
		relay_account_balance_metric.register_and_spawn(&metrics.registry)?;
	}

	Ok(())
}

/// Add relay accounts rewards metrics.
pub async fn add_relay_rewards_metrics<C: ChainWithRewards, BC: ChainWithMessages, LaneId>(
	client: impl Client<C>,
	metrics: &MetricsParams,
	relay_accounts: &Vec<TaggedAccount<AccountIdOf<C>>>,
	lanes: &[LaneId],
) -> anyhow::Result<()>
where
	C::RewardBalance: Into<u128> + std::fmt::Debug,
	C::Reward: From<RewardsAccountParams<LaneId>>,
	LaneId: Clone + Copy + Decode + EncodeLike + Send + Sync + Labeled,
{
	if relay_accounts.is_empty() {
		return Ok(())
	}

	for account in relay_accounts {
		if let Some(_) = C::WITH_CHAIN_RELAYERS_PALLET_NAME {
			for lane in lanes {
				FloatStorageValueMetric::new(
					FixedU128OrOne,
					client.clone(),
					C::account_reward_storage_key(account.id(), RewardsAccountParams::new(*lane, BC::ID, RewardsAccountOwner::ThisChain)),
					format!("at_{}_relay_{}_reward_for_msgs_from_{}_on_lane_{}", C::NAME, account.tag(), BC::NAME, lane.label()),
					format!("Reward of the {} relay account at {} for delivering messages from {} on lane {:?}", account.tag(), C::NAME, BC::NAME, lane.label()),
				)?.register_and_spawn(&metrics.registry)?;

				FloatStorageValueMetric::new(
					FixedU128OrOne,
					client.clone(),
					C::account_reward_storage_key(account.id(), RewardsAccountParams::new(*lane, BC::ID, RewardsAccountOwner::BridgedChain)),
					format!("at_{}_relay_{}_reward_for_msgs_to_{}_on_lane_{}", C::NAME, account.tag(), BC::NAME, lane.label()),
					format!("Reward of the {} relay account at {} for delivering messages confirmations from {} on lane {:?}", account.tag(), C::NAME, BC::NAME, lane.label()),
				)?.register_and_spawn(&metrics.registry)?;
			}
		}
	}

	Ok(())
}

/// Adapter for `FloatStorageValueMetric` to decode account free balance.
#[derive(Clone, Debug)]
struct AccountBalanceFromAccountInfo<C> {
	token_decimals: u32,
	_phantom: PhantomData<C>,
}

impl<C> FloatStorageValue for AccountBalanceFromAccountInfo<C>
where
	C: Chain,
	BalanceOf<C>: Into<u128>,
{
	type Value = FixedU128;

	fn decode(
		&self,
		maybe_raw_value: Option<StorageData>,
	) -> Result<Option<Self::Value>, SubstrateError> {
		maybe_raw_value
			.map(|raw_value| {
				AccountInfo::<NonceOf<C>, AccountData<BalanceOf<C>>>::decode(&mut &raw_value.0[..])
					.map_err(SubstrateError::ResponseParseFailed)
					.map(|account_data| {
						convert_to_token_balance(account_data.data.free.into(), self.token_decimals)
					})
			})
			.transpose()
	}
}

/// Convert from raw `u128` balance (nominated in smallest chain token units) to the float regular
/// tokens value.
fn convert_to_token_balance(balance: u128, token_decimals: u32) -> FixedU128 {
	FixedU128::from_inner(balance.saturating_mul(FixedU128::DIV / 10u128.pow(token_decimals)))
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn token_decimals_used_properly() {
		let plancks = 425_000_000_000;
		let token_decimals = 10;
		let dots = convert_to_token_balance(plancks, token_decimals);
		assert_eq!(dots, FixedU128::saturating_from_rational(425, 10));
	}
}
