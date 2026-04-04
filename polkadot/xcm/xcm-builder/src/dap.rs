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

//! XCM adapter for [`pallet_dap_satellite::SendToDap`].
//!
//! Provides [`SendToDapViaTeleport`], a configurable adapter that implements
//! [`pallet_dap_satellite::SendToDap`] by teleporting native tokens to the
//! central DAP via XCM.

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

const LOG_TARGET: &str = "xcm::dap";

/// XCM adapter that implements [`pallet_dap_satellite::SendToDap`] by teleporting native tokens
/// to the central DAP buffer account on a destination chain. The execution is transactional:
/// if anything fails, all local state changes are rolled back.
///
/// # Type parameters
///
/// - `XcmConfig`: Implements [`xcm_executor::Config`]. Used to run local XCM execution.
/// - `Dest`: Implements [`Get<Location>`]. The location of the destination chain with the
///   central DAP.
/// - `NativeAsset`: Implements [`Get<Location>`]. The location of the native token being sent.
/// - `BufferLocation`: Implements [`Get<InteriorLocation>`]. The interior location of the
///   central DAP buffer account on `Dest`.
pub struct SendToDapViaTeleport<XcmConfig, Dest, NativeAsset, BufferLocation>(
	PhantomData<(XcmConfig, Dest, NativeAsset, BufferLocation)>,
);

impl<XcmConfig, Dest, NativeAsset, BufferLocation, AccountId, Balance>
	pallet_dap_satellite::SendToDap<AccountId, Balance>
	for SendToDapViaTeleport<XcmConfig, Dest, NativeAsset, BufferLocation>
where
	XcmConfig: xcm_executor::Config,
	Dest: Get<Location>,
	NativeAsset: Get<Location>,
	BufferLocation: Get<InteriorLocation>,
	AccountId: Into<[u8; 32]>,
	Balance: Into<u128> + Copy,
{
	fn send_native(source: AccountId, amount: Balance) -> Result<(), ()> {
		let dest = Dest::get();
		let asset = Asset { id: AssetId(NativeAsset::get()), fun: Fungible(amount.into()) };
		let beneficiary: Location = BufferLocation::get().into_location();

		let remote_xcm = Xcm(vec![DepositAsset { assets: Wild(AllCounted(1)), beneficiary }]);

		// The XCM flow is: `ReceiveTeleportedAsset → UnpaidExecution → DepositAsset`.
		// The receiving chain must allow the source account in `AllowExplicitUnpaidExecutionFrom`.
		let xcm: Xcm<XcmConfig::RuntimeCall> = Xcm(vec![
			UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
			DescendOrigin(Junction::AccountId32 { network: None, id: source.into() }.into()),
			WithdrawAsset(asset.into()),
			InitiateTransfer {
				destination: dest,
				remote_fees: None,
				preserve_origin: true,
				assets: BoundedVec::truncate_from(alloc::vec![
					AssetTransferFilter::Teleport(Wild(AllCounted(1))),
				]),
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
					tracing::warn!(
						target: LOG_TARGET,
						?exec_error,
						"DAP satellite: XCM execution failed"
					);

					TransactionOutcome::Rollback(Err(DispatchError::Other(
						"XCM execution failed",
					)))
				},
			}
		})
		.map_err(|_| ())
	}
}
