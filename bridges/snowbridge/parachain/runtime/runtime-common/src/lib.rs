// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! # Runtime Common
//!
//! Common traits and types shared by runtimes.
#![cfg_attr(not(feature = "std"), no_std)]

use core::marker::PhantomData;
use frame_support::traits::Get;
use snowbridge_core::{outbound::SendMessageFeeProvider, sibling_sovereign_account_raw};
use sp_arithmetic::traits::{BaseArithmetic, Unsigned};
use xcm::prelude::*;
use xcm_builder::{deposit_or_burn_fee, HandleFee};
use xcm_executor::traits::{FeeReason, TransactAsset};

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
	Balance: BaseArithmetic + Unsigned + Copy + From<u128> + Into<u128>,
	AccountId: Clone + Into<[u8; 32]> + From<[u8; 32]>,
	FeeAssetLocation: Get<MultiLocation>,
	EthereumNetwork: Get<NetworkId>,
	AssetTransactor: TransactAsset,
	FeeProvider: SendMessageFeeProvider<Balance = Balance>,
{
	fn handle_fee(
		fees: MultiAssets,
		context: Option<&XcmContext>,
		reason: FeeReason,
	) -> MultiAssets {
		let token_location = FeeAssetLocation::get();

		// Check the reason to see if this export is for snowbridge.
		if !matches!(
			reason,
			FeeReason::Export { network: bridged_network, destination }
				if bridged_network == EthereumNetwork::get() && destination == Here
		) {
			return fees
		}

		// Get the parachain sovereign from the `context`.
		let para_sovereign = if let Some(XcmContext {
			origin: Some(MultiLocation { parents: 1, interior }),
			..
		}) = context
		{
			if let Some(Parachain(sibling_para_id)) = interior.first() {
				let account: AccountId =
					sibling_sovereign_account_raw((*sibling_para_id).into()).into();
				account
			} else {
				return fees
			}
		} else {
			return fees
		};

		// Get the total fee offered by export message.
		let maybe_total_supplied_fee: Option<(usize, Balance)> = fees
			.inner()
			.iter()
			.enumerate()
			.filter_map(|(index, asset)| {
				if let MultiAsset { id: Concrete(location), fun: Fungible(amount) } = asset {
					if *location == token_location {
						return Some((index, (*amount).into()))
					}
				}
				None
			})
			.next();

		if let Some((fee_index, total_fee)) = maybe_total_supplied_fee {
			let remote_fee = total_fee.saturating_sub(FeeProvider::local_fee());
			if remote_fee > (0u128).into() {
				// Refund remote component of fee to physical origin
				deposit_or_burn_fee::<AssetTransactor, _>(
					MultiAsset { id: Concrete(token_location), fun: Fungible(remote_fee.into()) }
						.into(),
					context,
					para_sovereign,
				);
				// Return remaining fee to the next fee handler in the chain.
				let mut modified_fees = fees.inner().clone();
				modified_fees.remove(fee_index);
				modified_fees.push(MultiAsset {
					id: Concrete(token_location),
					fun: Fungible((total_fee - remote_fee).into()),
				});
				return modified_fees.into()
			}
		}

		log::info!(
			target: "xcm::fees",
			"XcmExportFeeToSibling skipped: {fees:?}, context: {context:?}, reason: {reason:?}",
		);
		fees
	}
}
