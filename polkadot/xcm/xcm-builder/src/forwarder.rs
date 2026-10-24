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

//! XCM adapters for the accumulate-and-forward pallet.
//!
//! - [`TeleportForwarderForAccountId32`]: teleports to a destination and deposits there.
//! - [`TeleportAndBurnForwarderForAccountId32`]: teleports to a destination and burns there.

use alloc::vec;
use core::marker::PhantomData;
use frame_support::{
	storage::{with_transaction, TransactionOutcome},
	traits::Get,
	BoundedVec,
};
use sp_runtime::DispatchError;
use xcm::latest::{prelude::*, AssetTransferFilter};
use xcm_executor::XcmExecutor;

const LOG_TARGET: &str = "xcm::accumulate-forward";

/// XCM adapter that implements [`pallet_accumulate_and_forward::Forwarder`] for AccountId32-type
/// source accounts by teleporting native tokens to a target account on a destination chain.
/// Local-execution failures roll back all local state changes. Once the local executor reports
/// success, the message is queued and any destination-side rejection results in trapped assets
/// at the destination with no automatic recovery path.
///
/// NOTE: This adapter passes `Weight::MAX` to the XCM executor, relying on the call site to
/// enforce a weight budget before invoking it. It is designed to be called only from rate-limited
/// internal hooks such as `on_idle` and should never be wired to user-callable extrinsics.
pub struct TeleportForwarderForAccountId32<XcmConfig, Dest, NativeAsset, StagingLocation>(
	PhantomData<(XcmConfig, Dest, NativeAsset, StagingLocation)>,
);

impl<XcmConfig, Dest, NativeAsset, StagingLocation, AccountId, Balance>
	pallet_accumulate_and_forward::Forwarder<AccountId, Balance>
	for TeleportForwarderForAccountId32<XcmConfig, Dest, NativeAsset, StagingLocation>
where
	XcmConfig: xcm_executor::Config,
	Dest: Get<Location>,
	NativeAsset: Get<Location>,
	StagingLocation: Get<InteriorLocation>,
	AccountId: Into<[u8; 32]> + Clone,
	Balance: Into<u128> + Copy,
{
	fn forward(source: AccountId, amount: Balance) -> Result<(), ()> {
		let dest = Dest::get();
		let asset = Asset { id: AssetId(NativeAsset::get()), fun: Fungible(amount.into()) };
		let beneficiary: Location = StagingLocation::get().into_location();

		let remote_xcm = Xcm(vec![DepositAsset { assets: Wild(AllCounted(1)), beneficiary }]);

		teleport_from_account::<XcmConfig>(source.into(), asset, dest, remote_xcm)
	}
}

/// Like [`TeleportForwarderForAccountId32`], but burns the teleported tokens at the destination
/// instead of depositing them. For networks without a DAP.
pub struct TeleportAndBurnForwarderForAccountId32<XcmConfig, Dest, NativeAsset>(
	PhantomData<(XcmConfig, Dest, NativeAsset)>,
);

impl<XcmConfig, Dest, NativeAsset, AccountId, Balance>
	pallet_accumulate_and_forward::Forwarder<AccountId, Balance>
	for TeleportAndBurnForwarderForAccountId32<XcmConfig, Dest, NativeAsset>
where
	XcmConfig: xcm_executor::Config,
	Dest: Get<Location>,
	NativeAsset: Get<Location>,
	AccountId: Into<[u8; 32]> + Clone,
	Balance: Into<u128> + Copy,
{
	fn forward(source: AccountId, amount: Balance) -> Result<(), ()> {
		let dest = Dest::get();
		let asset = Asset { id: AssetId(NativeAsset::get()), fun: Fungible(amount.into()) };

		// `BurnAsset` takes concrete assets, so name them as the destination sees them.
		let remote_asset = asset
			.clone()
			.reanchored(&dest, &XcmConfig::UniversalLocation::get())
			.map_err(|asset| {
				tracing::debug!(
					target: LOG_TARGET,
					?asset,
					?dest,
					"accumulate-forward: failed to reanchor asset to destination"
				);
			})?;

		let remote_xcm = Xcm(vec![BurnAsset(remote_asset.into())]);

		teleport_from_account::<XcmConfig>(source.into(), asset, dest, remote_xcm)
	}
}

/// Teleports `asset` from `source` to `dest` with `preserve_origin`, followed by `remote_xcm`.
/// Rolls back local changes on failure.
fn teleport_from_account<XcmConfig: xcm_executor::Config>(
	source: [u8; 32],
	asset: Asset,
	dest: Location,
	remote_xcm: Xcm<()>,
) -> Result<(), ()> {
	let xcm: Xcm<XcmConfig::RuntimeCall> = Xcm(vec![
		UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
		DescendOrigin(Junction::AccountId32 { network: None, id: source }.into()),
		WithdrawAsset(asset.into()),
		InitiateTransfer {
			destination: dest,
			remote_fees: None,
			preserve_origin: true,
			assets: BoundedVec::truncate_from(vec![AssetTransferFilter::Teleport(Wild(
				AllCounted(1),
			))]),
			remote_xcm,
		},
	]);

	with_transaction(|| -> TransactionOutcome<Result<(), DispatchError>> {
		let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
			Location::here(),
			xcm,
			&mut [0u8; 32],
			Weight::MAX,
			Weight::MAX,
		);

		match outcome {
			Outcome::Complete { .. } => TransactionOutcome::Commit(Ok(())),
			exec_error => {
				tracing::debug!(
					target: LOG_TARGET,
					?exec_error,
					"accumulate-forward: XCM execution failed"
				);

				TransactionOutcome::Rollback(Err(DispatchError::Other("XCM execution failed")))
			},
		}
	})
	.map_err(|_| ())
}
