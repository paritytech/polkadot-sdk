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
use crate::{config, XcmExecutor, Weight, traits::{ProcessTransaction, WeightTrader}};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;
use frame_support::ensure;

impl<Config: config::Config> ExecuteInstruction<Config> for BuyExecution {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let BuyExecution { fees, weight_limit } = self;
		// There is no need to buy any weight if `weight_limit` is `Unlimited` since it
		// would indicate that `AllowTopLevelPaidExecutionFrom` was unused for execution
		// and thus there is some other reason why it has been determined that this XCM
		// should be executed.
		let Some(weight) = Option::<Weight>::from(weight_limit) else { return Ok(()) };
		let old_holding = executor.holding.clone();
		// Save the asset being used for execution fees, so we later know what should be
		// used for delivery fees.
		executor.asset_used_in_buy_execution = Some(fees.id.clone());
		tracing::trace!(
			target: "xcm::executor::BuyExecution",
			asset_used_in_buy_execution = ?executor.asset_used_in_buy_execution
		);
		// pay for `weight` using up to `fees` of the holding register.
		let max_fee =
			executor.holding.try_take(fees.clone().into()).map_err(|e| {
				tracing::error!(target: "xcm::process_instruction::buy_execution", ?e, ?fees,
					"Failed to take fees from holding");
				XcmError::NotHoldingFees
			})?;
		let result = Config::TransactionalProcessor::process(|| {
			let unspent = executor.trader.buy_weight(weight, max_fee, &executor.context)?;
			executor.holding.subsume_assets(unspent);
			Ok(())
		});
		if result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for PayFees {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let PayFees { asset } = self;
		// Message was not weighed, there is nothing to pay.
		if executor.message_weight == Weight::zero() {
			tracing::warn!(
				target: "xcm::executor::PayFees",
				"Message was not weighed or weight was 0. Nothing will be charged.",
			);
			return Ok(());
		}
		// Record old holding in case we need to rollback.
		let old_holding = executor.holding.clone();
		// The max we're willing to pay for fees is decided by the `asset` operand.
		tracing::trace!(
			target: "xcm::executor::PayFees",
			asset_for_fees = ?asset,
			message_weight = ?executor.message_weight,
		);
		let max_fee =
			executor.holding.try_take(asset.into()).map_err(|_| XcmError::NotHoldingFees)?;
		// Pay for execution fees.
		let result = Config::TransactionalProcessor::process(|| {
			let unspent =
				executor.trader.buy_weight(executor.message_weight, max_fee, &executor.context)?;
			// Move unspent to the `fees` register.
			executor.fees.subsume_assets(unspent);
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			// Rollback.
			executor.holding = old_holding;
		}
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for RefundSurplus {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		executor.refund_surplus()
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for UnpaidExecution {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let UnpaidExecution { check_origin, .. } = self;
		ensure!(
			check_origin.is_none() || executor.context.origin == check_origin,
			XcmError::BadOrigin
		);
		Ok(())
	}
}
