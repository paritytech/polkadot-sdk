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

//! XCM adapters for the Dynamic Allocation Pool (DAP).
//!
//! - [`SendToDapViaTeleport`]: satellite-side fund sending adapter.
//! - [`DapBufferAssetTransactor`]: AssetHub-side fund receiving adapter.

use alloc::vec;
use core::marker::PhantomData;
use frame_support::{
	storage::{with_transaction, TransactionOutcome},
	traits::{fungible::Unbalanced, Get},
	BoundedVec,
};
use sp_runtime::DispatchError;
use xcm::latest::{prelude::*, AssetTransferFilter};
use xcm_executor::{
	traits::{ConvertLocation, MatchesFungible, TransactAsset},
	AssetsInHolding, XcmExecutor,
};

const LOG_TARGET: &str = "xcm::dap";

/// XCM adapter that implements [`sp_dap::SendToDap`] by teleporting native tokens to
/// the central DAP buffer account on a destination chain. The execution is transactional:
/// if anything fails, all local state changes are rolled back.
pub struct SendToDapViaTeleport<XcmConfig, Dest, NativeAsset, BufferLocation>(
	PhantomData<(XcmConfig, Dest, NativeAsset, BufferLocation)>,
);

impl<XcmConfig, Dest, NativeAsset, BufferLocation, AccountId, Balance>
	sp_dap::SendToDap<AccountId, Balance>
	for SendToDapViaTeleport<XcmConfig, Dest, NativeAsset, BufferLocation>
where
	XcmConfig: xcm_executor::Config,
	Dest: Get<Location>,
	NativeAsset: Get<Location>,
	BufferLocation: Get<InteriorLocation>,
	AccountId: Into<[u8; 32]> + Clone,
	Balance: Into<u128> + Copy,
{
	fn send_native(source: AccountId, amount: Balance) -> Result<(), ()> {
		let dest = Dest::get();
		let asset = Asset { id: AssetId(NativeAsset::get()), fun: Fungible(amount.into()) };
		let beneficiary: Location = BufferLocation::get().into_location();

		let remote_xcm = Xcm(vec![DepositAsset { assets: Wild(AllCounted(1)), beneficiary }]);

		// The XCM flow: `ReceiveTeleportedAsset → AliasOrigin(satellite) → UnpaidExecution →
		// DepositAsset`. `preserve_origin: true` causes `InitiateTransfer` to prepend
		// `AliasOrigin(satellite_location)` to the remote XCM.
		let xcm: Xcm<XcmConfig::RuntimeCall> = Xcm(vec![
			UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
			DescendOrigin(Junction::AccountId32 { network: None, id: source.into() }.into()),
			WithdrawAsset(asset.into()),
			InitiateTransfer {
				destination: dest,
				remote_fees: None,
				preserve_origin: true,
				assets: BoundedVec::truncate_from(alloc::vec![AssetTransferFilter::Teleport(
					Wild(AllCounted(1))
				),]),
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
						"DAP satellite: XCM execution failed"
					);

					TransactionOutcome::Rollback(Err(DispatchError::Other("XCM execution failed")))
				},
			}
		})
		.map_err(|_| ())
	}
}

/// Fund-receiving adapter for the central DAP on AssetHub. It wraps an inner [`TransactAsset`]
/// implementation and additionally calls [`Unbalanced::deactivate`]. This ensures funds
/// sitting in the DAP buffer account are excluded from `active_issuance()`, thus mirroring the
/// behaviour of locally-received funds processed by the DAP's `OnUnbalanced` handler.
pub struct DapBufferAssetTransactor<
	Inner,
	Currency,
	Matcher,
	AccountIdConverter,
	AccountId,
	WatchedAccount,
>(PhantomData<(Inner, Currency, Matcher, AccountIdConverter, AccountId, WatchedAccount)>);

impl<Inner, Currency, Matcher, AccountIdConverter, AccountId, WatchedAccount> TransactAsset
	for DapBufferAssetTransactor<
		Inner,
		Currency,
		Matcher,
		AccountIdConverter,
		AccountId,
		WatchedAccount,
	>
where
	Inner: TransactAsset,
	Currency: Unbalanced<AccountId>,
	Matcher: MatchesFungible<Currency::Balance>,
	AccountIdConverter: ConvertLocation<AccountId>,
	AccountId: PartialEq,
	WatchedAccount: Get<AccountId>,
{
	fn deposit_asset(
		what: AssetsInHolding,
		who: &Location,
		context: Option<&XcmContext>,
	) -> Result<(), (AssetsInHolding, XcmError)> {
		// Check before consuming `what`: if we're depositing into the watched buffer account,
		// we capture the amount so we can deactivate it after the deposit succeeds.
		let maybe_deactivate = AccountIdConverter::convert_location(who)
			.filter(|account| account == &WatchedAccount::get())
			.and_then(|_| what.fungible_assets_iter().find_map(|a| Matcher::matches_fungible(&a)));

		Inner::deposit_asset(what, who, context)?;

		if let Some(amount) = maybe_deactivate {
			Currency::deactivate(amount);
		}

		Ok(())
	}

	fn can_check_in(origin: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		Inner::can_check_in(origin, what, context)
	}

	fn check_in(origin: &Location, what: &Asset, context: &XcmContext) {
		Inner::check_in(origin, what, context)
	}

	fn can_check_out(dest: &Location, what: &Asset, context: &XcmContext) -> XcmResult {
		Inner::can_check_out(dest, what, context)
	}

	fn check_out(dest: &Location, what: &Asset, context: &XcmContext) {
		Inner::check_out(dest, what, context)
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		maybe_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		Inner::withdraw_asset(what, who, maybe_context)
	}

	fn mint_asset(what: &Asset, context: &XcmContext) -> Result<AssetsInHolding, XcmError> {
		Inner::mint_asset(what, context)
	}
}
