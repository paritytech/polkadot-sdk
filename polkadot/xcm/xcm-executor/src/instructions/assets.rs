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

use super::ExecuteInstruction;
use crate::traits::{
	AssetExchange, AssetLock, ClaimAssets, Enact, FeeReason,
	ProcessTransaction, TransactAsset,
};
use crate::{config, validate_send, AssetTransferFilter, WeightLimit, XcmExecutor};
use frame_support::{
	ensure,
	traits::{ContainsPair, Get},
};
use xcm::{
	latest::{Instruction, instructions::*, Error as XcmError, Reanchorable, Xcm},
	traits::{IntoInstruction, SendXcm},
};

impl<Config: config::Config> ExecuteInstruction<Config> for WithdrawAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let assets = self.0;
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		executor.ensure_can_subsume_assets(assets.len())?;
		Config::TransactionalProcessor::process(|| {
			// Take `assets` from the origin account (on-chain)...
			for asset in assets.inner() {
				Config::AssetTransactor::withdraw_asset(asset, origin, Some(&executor.context))?;
			}
			Ok(())
		})
		.and_then(|_| {
			// ...and place into holding.
			executor.holding.subsume_assets(assets.into());
			Ok(())
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ReserveAssetDeposited {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let assets = self.0;
		// check whether we trust origin to be our reserve location for this asset.
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		executor.ensure_can_subsume_assets(assets.len())?;
		for asset in assets.inner() {
			// Must ensure that we recognise the asset as being managed by the origin.
			ensure!(Config::IsReserve::contains(asset, origin), XcmError::UntrustedReserveLocation);
		}
		executor.holding.subsume_assets(assets.into());
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for TransferAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let TransferAsset { assets, beneficiary } = self;
		Config::TransactionalProcessor::process(|| {
			// Take `assets` from the origin account (on-chain) and place into dest account.
			let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
			for asset in assets.inner() {
				Config::AssetTransactor::transfer_asset(
					&asset,
					origin,
					&beneficiary,
					&executor.context,
				)?;
			}
			Ok(())
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for TransferReserveAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let TransferReserveAsset { mut assets, dest, xcm } = self;
		Config::TransactionalProcessor::process(|| {
			let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
			// Take `assets` from the origin account (on-chain) and place into dest account.
			for asset in assets.inner() {
				Config::AssetTransactor::transfer_asset(asset, origin, &dest, &executor.context)?;
			}
			let reanchor_context = Config::UniversalLocation::get();
			assets.reanchor(&dest, &reanchor_context).map_err(|()| XcmError::LocationFull)?;
			let mut message = vec![
				Instruction::ReserveAssetDeposited(assets),
				Instruction::ClearOrigin,
			];
			message.extend(xcm.0.into_iter());
			executor.send(dest, Xcm::new(message), FeeReason::TransferReserveAsset)?;
			Ok(())
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ReceiveTeleportedAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let assets = self.0;
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		executor.ensure_can_subsume_assets(assets.len())?;
		Config::TransactionalProcessor::process(|| {
			// check whether we trust origin to teleport this asset to us via config trait.
			for asset in assets.inner() {
				// We only trust the origin to send us assets that they identify as their
				// sovereign assets.
				ensure!(
					Config::IsTeleporter::contains(asset, origin),
					XcmError::UntrustedTeleportLocation
				);
				// We should check that the asset can actually be teleported in (for this to
				// be in error, there would need to be an accounting violation by one of the
				// trusted chains, so it's unlikely, but we don't want to punish a possibly
				// innocent chain/user).
				Config::AssetTransactor::can_check_in(origin, asset, &executor.context)?;
				Config::AssetTransactor::check_in(origin, asset, &executor.context);
			}
			Ok(())
		})
		.and_then(|_| {
			executor.holding.subsume_assets(assets.into());
			Ok(())
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for DepositAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let DepositAsset { assets, beneficiary } = self;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			let deposited = executor.holding.saturating_take(assets);
			XcmExecutor::<Config>::deposit_assets_with_retry(
				&deposited,
				&beneficiary,
				Some(&executor.context),
			)
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for DepositReserveAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let DepositReserveAsset { assets, dest, xcm } = self;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			let mut assets = executor.holding.saturating_take(assets);
			// When not using `PayFees`, nor `JIT_WITHDRAW`, delivery fees are paid from
			// transferred assets.
			let maybe_delivery_fee_from_assets =
				if executor.fees.is_empty() && !executor.fees_mode.jit_withdraw {
					// Deduct and return the part of `assets` that shall be used for delivery fees.
					executor.take_delivery_fee_from_assets(
						&mut assets,
						&dest,
						FeeReason::DepositReserveAsset,
						&xcm,
					)?
				} else {
					None
				};
			let mut message = Vec::with_capacity(xcm.len() + 2);
			tracing::trace!(target: "xcm::DepositReserveAsset", ?assets, "Assets except delivery fee");
			XcmExecutor::<Config>::do_reserve_deposit_assets(
				assets,
				&dest,
				&mut message,
				Some(&executor.context),
			)?;
			// clear origin for subsequent custom instructions
			message.push(Instruction::from(ClearOrigin.into_instruction()));
			// append custom instructions
			message.extend(xcm.0.into_iter());
			if let Some(delivery_fee) = maybe_delivery_fee_from_assets {
				// Put back delivery_fee in holding register to be charged by XcmSender.
				executor.holding.subsume_assets(delivery_fee);
			}
			executor.send(dest, Xcm::new(message), FeeReason::DepositReserveAsset)?;
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for InitiateReserveWithdraw {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let InitiateReserveWithdraw { assets, reserve, xcm } = self;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			let mut assets = executor.holding.saturating_take(assets);
			// When not using `PayFees`, nor `JIT_WITHDRAW`, delivery fees are paid from
			// transferred assets.
			let maybe_delivery_fee_from_assets =
				if executor.fees.is_empty() && !executor.fees_mode.jit_withdraw {
					// Deduct and return the part of `assets` that shall be used for delivery fees.
					executor.take_delivery_fee_from_assets(
						&mut assets,
						&reserve,
						FeeReason::InitiateReserveWithdraw,
						&xcm,
					)?
				} else {
					None
				};
			let mut message = Vec::with_capacity(xcm.len() + 2);
			XcmExecutor::<Config>::do_reserve_withdraw_assets(
				assets,
				&mut executor.holding,
				&reserve,
				&mut message,
			)?;
			// clear origin for subsequent custom instructions
			message.push(Instruction::ClearOrigin);
			// append custom instructions
			message.extend(xcm.0.into_iter());
			if let Some(delivery_fee) = maybe_delivery_fee_from_assets {
				// Put back delivery_fee in holding register to be charged by XcmSender.
				executor.holding.subsume_assets(delivery_fee);
			}
			executor.send(reserve, Xcm::new(message), FeeReason::InitiateReserveWithdraw)?;
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for InitiateTeleport {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let InitiateTeleport { assets, dest, xcm } = self;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			let mut assets = executor.holding.saturating_take(assets);
			// When not using `PayFees`, nor `JIT_WITHDRAW`, delivery fees are paid from
			// transferred assets.
			let maybe_delivery_fee_from_assets =
				if executor.fees.is_empty() && !executor.fees_mode.jit_withdraw {
					// Deduct and return the part of `assets` that shall be used for delivery fees.
					executor.take_delivery_fee_from_assets(
						&mut assets,
						&dest,
						FeeReason::InitiateTeleport,
						&xcm,
					)?
				} else {
					None
				};
			let mut message = Vec::with_capacity(xcm.len() + 2);
			XcmExecutor::<Config>::do_teleport_assets(assets, &dest, &mut message, &executor.context)?;
			// clear origin for subsequent custom instructions
			message.push(Instruction::ClearOrigin);
			// append custom instructions
			message.extend(xcm.0.into_iter());
			if let Some(delivery_fee) = maybe_delivery_fee_from_assets {
				// Put back delivery_fee in holding register to be charged by XcmSender.
				executor.holding.subsume_assets(delivery_fee);
			}
			executor.send(dest.clone(), Xcm::new(message), FeeReason::InitiateTeleport)?;
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for InitiateTransfer {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let InitiateTransfer { destination, remote_fees, preserve_origin, assets, remote_xcm } =
			self;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			let mut message = Vec::with_capacity(assets.len() + remote_xcm.len() + 2);

			// We need to transfer the fees and buy execution on remote chain _BEFORE_
			// transferring the other assets. This is required to satisfy the
			// `MAX_ASSETS_FOR_BUY_EXECUTION` limit in the `AllowTopLevelPaidExecutionFrom`
			// barrier.
			if let Some(remote_fees) = remote_fees {
				let reanchored_fees = match remote_fees {
					AssetTransferFilter::Teleport(fees_filter) => {
						let teleport_fees = executor
							.holding
							.try_take(fees_filter)
							.map_err(|_| XcmError::NotHoldingFees)?;
						XcmExecutor::<Config>::do_teleport_assets(
							teleport_fees,
							&destination,
							&mut message,
							&executor.context,
						)?
					},
					AssetTransferFilter::ReserveDeposit(fees_filter) => {
						let reserve_deposit_fees = executor
							.holding
							.try_take(fees_filter)
							.map_err(|_| XcmError::NotHoldingFees)?;
						XcmExecutor::<Config>::do_reserve_deposit_assets(
							reserve_deposit_fees,
							&destination,
							&mut message,
							Some(&executor.context),
						)?
					},
					AssetTransferFilter::ReserveWithdraw(fees_filter) => {
						let reserve_withdraw_fees = executor
							.holding
							.try_take(fees_filter)
							.map_err(|_| XcmError::NotHoldingFees)?;
						XcmExecutor::<Config>::do_reserve_withdraw_assets(
							reserve_withdraw_fees,
							&mut executor.holding,
							&destination,
							&mut message,
						)?
					},
				};
				ensure!(reanchored_fees.len() == 1, XcmError::TooManyAssets);
				let fees = reanchored_fees.into_inner().pop().ok_or(XcmError::NotHoldingFees)?;
				// move these assets to the fees register for covering execution and paying
				// any subsequent fees
				message.push(Instruction::PayFees { asset: fees });
			} else {
				// unpaid execution
				message.push(
					Instruction::UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
				);
			}

			// add any extra asset transfers
			for asset_filter in assets {
				match asset_filter {
					AssetTransferFilter::Teleport(assets) => {
						XcmExecutor::<Config>::do_teleport_assets(
							executor.holding.saturating_take(assets),
							&destination,
							&mut message,
							&executor.context,
						)?
					},
					AssetTransferFilter::ReserveDeposit(assets) => {
						XcmExecutor::<Config>::do_reserve_deposit_assets(
							executor.holding.saturating_take(assets),
							&destination,
							&mut message,
							Some(&executor.context),
						)?
					},
					AssetTransferFilter::ReserveWithdraw(assets) => {
						XcmExecutor::<Config>::do_reserve_withdraw_assets(
							executor.holding.saturating_take(assets),
							&mut executor.holding,
							&destination,
							&mut message,
						)?
					},
				};
			}
			if preserve_origin {
				// preserve current origin for subsequent user-controlled instructions on
				// remote chain
				let original_origin = executor
					.origin_ref()
					.cloned()
					.and_then(|origin| {
						XcmExecutor::<Config>::try_reanchor(origin, &destination)
							.map(|(reanchored, _)| reanchored)
							.ok()
					})
					.ok_or(XcmError::BadOrigin)?;
				message.push(Instruction::AliasOrigin(original_origin));
			} else {
				// clear origin for subsequent user-controlled instructions on remote chain
				message.push(Instruction::ClearOrigin);
			}
			// append custom instructions
			message.extend(remote_xcm.0.into_iter());
			// send the onward XCM
			executor.send(destination, Xcm::new(message), FeeReason::InitiateTransfer)?;
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ClaimAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ClaimAsset { assets, ticket } = self;
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		executor.ensure_can_subsume_assets(assets.len())?;
		let ok = Config::AssetClaims::claim_assets(origin, &ticket, &assets, &executor.context);
		ensure!(ok, XcmError::UnknownClaim);
		executor.holding.subsume_assets(assets.into());
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for BurnAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let BurnAsset(assets) = self;
		executor.holding.saturating_take(assets.into());
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for LockAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let LockAsset { asset, unlocker } = self;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			let origin = executor.cloned_origin().ok_or(XcmError::BadOrigin)?;
			let (remote_asset, context) = XcmExecutor::<Config>::try_reanchor(asset.clone(), &unlocker)?;
			let lock_ticket =
				Config::AssetLocker::prepare_lock(unlocker.clone(), asset, origin.clone())?;
			let owner = origin.reanchored(&unlocker, &context).map_err(|e| {
						tracing::error!(target: "xcm::xcm_executor::process_instruction", ?e, ?unlocker, ?context, "Failed to re-anchor origin");
						XcmError::ReanchorFailed
					})?;
			let msg = Xcm::<()>::new(vec![
				Instruction::NoteUnlockable { asset: remote_asset, owner }
			]);
			let (ticket, price) = validate_send::<Config::XcmSender>(unlocker, msg)?;
			executor.take_fee(price, FeeReason::LockAsset)?;
			lock_ticket.enact()?;
			Config::XcmSender::deliver(ticket)?;
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for UnlockAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let UnlockAsset { asset, target } = self;
		let origin = executor.cloned_origin().ok_or(XcmError::BadOrigin)?;
		Config::AssetLocker::prepare_unlock(origin, asset, target)?.enact()?;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for NoteUnlockable {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let NoteUnlockable { asset, owner } = self;
		let origin = executor.cloned_origin().ok_or(XcmError::BadOrigin)?;
		Config::AssetLocker::note_unlockable(origin, asset, owner)?;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for RequestUnlock {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let RequestUnlock { asset, locker } = self;
		let origin = executor.cloned_origin().ok_or(XcmError::BadOrigin)?;
		let remote_asset = XcmExecutor::<Config>::try_reanchor(asset.clone(), &locker)?.0;
		let remote_target = XcmExecutor::<Config>::try_reanchor(origin.clone(), &locker)?.0;
		let reduce_ticket =
			Config::AssetLocker::prepare_reduce_unlockable(locker.clone(), asset, origin.clone())?;
		let msg = Xcm::<()>::new(vec![
			Instruction::UnlockAsset { asset: remote_asset, target: remote_target }
		]);
		let (ticket, price) = validate_send::<Config::XcmSender>(locker, msg)?;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			executor.take_fee(price, FeeReason::RequestUnlock)?;
			reduce_ticket.enact()?;
			Config::XcmSender::deliver(ticket)?;
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExchangeAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExchangeAsset { give, want, maximal } = self;
		let old_holding = executor.holding.clone();
		let give = executor.holding.saturating_take(give);
		let result = Config::TransactionalProcessor::process(|| {
			executor.ensure_can_subsume_assets(want.len())?;
			let exchange_result =
				Config::AssetExchanger::exchange_asset(executor.origin_ref(), give, &want, maximal);
			if let Ok(received) = exchange_result {
				executor.holding.subsume_assets(received.into());
				Ok(())
			} else {
				Err(XcmError::NoDeal)
			}
		});
		if result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}
