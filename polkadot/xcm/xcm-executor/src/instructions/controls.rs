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
use crate::{config, traits::WeightBounds, FeesMode, Hint::AssetClaimer, XcmExecutor};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;

impl<Config: config::Config> ExecuteInstruction<Config> for SetErrorHandler<Config::RuntimeCall> {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let mut handler = self.0;
		let handler_weight =
			Config::Weigher::weight(&mut handler).map_err(|()| XcmError::WeightNotComputable)?;
		executor.total_surplus.saturating_accrue(executor.error_handler_weight);
		executor.error_handler = handler;
		executor.error_handler_weight = handler_weight;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for SetAppendix<Config::RuntimeCall> {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let mut appendix = self.0;
		let appendix_weight =
			Config::Weigher::weight(&mut appendix).map_err(|()| XcmError::WeightNotComputable)?;
		executor.total_surplus.saturating_accrue(executor.appendix_weight);
		executor.appendix = appendix;
		executor.appendix_weight = appendix_weight;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ClearError {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		executor.error = None;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for SetHints {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let SetHints { hints } = self;
		for hint in hints.into_iter() {
			match hint {
				AssetClaimer { location } => executor.asset_claimer = Some(location),
			}
		}
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for Trap {
	fn execute(self, _executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let Trap(code) = self;
		Err(XcmError::Trap(code))
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ClearTransactStatus {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		executor.transact_status = Default::default();
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for SetFeesMode {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let SetFeesMode { jit_withdraw } = self;
		executor.fees_mode = FeesMode { jit_withdraw };
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for SetTopic {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let SetTopic(topic) = self;
		executor.context.topic = Some(topic);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ClearTopic {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		executor.context.topic = None;
		Ok(())
	}
}
