// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! # Runtime Common
//!
//! Common traits and types shared by runtimes.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

use codec::FullCodec;
use core::marker::PhantomData;
use frame_support::traits::Get;
use snowbridge_core::outbound::SendMessageFeeProvider;
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use sp_std::fmt::Debug;
use xcm::prelude::*;
use xcm_builder::HandleFee;
use xcm_executor::traits::{FeeReason, TransactAsset};

pub const LOG_TARGET: &str = "xcm::export-fee-to-sibling";

/// A `HandleFee` implementation that takes fees from `ExportMessage` XCM instructions
/// to Snowbridge and splits off the remote fee and deposits it to the origin
/// parachain sovereign account. The local fee is then returned back to be handled by
/// the next fee handler in the chain. Most likely the treasury account.
pub struct XcmExportFeeToSibling<
	Balance,
	AccountId,
	FeeAssetLocation,
	EthereumNetwork,
	AssetTransactor,
	FeeProvider,
>(
	PhantomData<(
		Balance,
		AccountId,
		FeeAssetLocation,
		EthereumNetwork,
		AssetTransactor,
		FeeProvider,
	)>,
);

impl<Balance, AccountId, FeeAssetLocation, EthereumNetwork, AssetTransactor, FeeProvider> HandleFee
	for XcmExportFeeToSibling<
		Balance,
		AccountId,
		FeeAssetLocation,
		EthereumNetwork,
		AssetTransactor,
		FeeProvider,
	> where
	Balance: BaseArithmetic + Unsigned + Copy + From<u128> + Into<u128> + Debug,
	AccountId: Clone + FullCodec,
	FeeAssetLocation: Get<Location>,
	EthereumNetwork: Get<NetworkId>,
	AssetTransactor: TransactAsset,
	FeeProvider: SendMessageFeeProvider<Balance = Balance>,
{
	fn handle_fee(fees: Assets, context: Option<&XcmContext>, reason: FeeReason) -> Assets {
		let token_location = FeeAssetLocation::get();

		// Check the reason to see if this export is for snowbridge.
		if !matches!(
			reason,
			FeeReason::Export { network: bridged_network, ref destination }
				if bridged_network == EthereumNetwork::get() && destination == &Here
		) {
			return fees
		}

		// Get the parachain sovereign from the `context`.
		let maybe_para_id: Option<u32> =
			if let Some(XcmContext { origin: Some(Location { parents: 1, interior }), .. }) =
				context
			{
				if let Some(Parachain(sibling_para_id)) = interior.first() {
					Some(*sibling_para_id)
				} else {
					None
				}
			} else {
				None
			};
		if maybe_para_id.is_none() {
			log::error!(
				target: LOG_TARGET,
				"invalid location in context {:?}",
				context,
			);
			return fees
		}
		let para_id = maybe_para_id.unwrap();

		// Get the total fee offered by export message.
		let maybe_total_supplied_fee: Option<(usize, Balance)> = fees
			.inner()
			.iter()
			.enumerate()
			.filter_map(|(index, asset)| {
				if let Asset { id: location, fun: Fungible(amount) } = asset {
					if location.0 == token_location {
						return Some((index, (*amount).into()))
					}
				}
				None
			})
			.next();
		if maybe_total_supplied_fee.is_none() {
			log::error!(
				target: LOG_TARGET,
				"could not find fee asset item in fees: {:?}",
				fees,
			);
			return fees
		}
		let (fee_index, total_fee) = maybe_total_supplied_fee.unwrap();
		let local_fee = FeeProvider::local_fee();
		let remote_fee = total_fee.saturating_sub(local_fee);
		if local_fee == Balance::zero() || remote_fee == Balance::zero() {
			log::error!(
				target: LOG_TARGET,
				"calculated refund incorrect with local_fee: {:?} and remote_fee: {:?}",
				local_fee,
				remote_fee,
			);
			return fees
		}
		// Refund remote component of fee to physical origin
		let result = AssetTransactor::deposit_asset(
			&Asset { id: AssetId(token_location.clone()), fun: Fungible(remote_fee.into()) },
			&Location::new(1, [Parachain(para_id)]),
			context,
		);
		if result.is_err() {
			log::error!(
				target: LOG_TARGET,
				"transact fee asset failed: {:?}",
				result.unwrap_err()
			);
			return fees
		}

		// Return remaining fee to the next fee handler in the chain.
		let mut modified_fees = fees.inner().clone();
		modified_fees.remove(fee_index);
		modified_fees.push(Asset { id: AssetId(token_location), fun: Fungible(local_fee.into()) });
		modified_fees.into()
	}
}
