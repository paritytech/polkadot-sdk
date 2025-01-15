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
use crate::{config, XcmExecutor, FeeReason, Response};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;

impl<Config: config::Config> ExecuteInstruction<Config> for ReportError {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let response_info = self.0;
		// Report the given result by sending a QueryResponse XCM to a previously given
		// outcome destination if one was registered.
		executor.respond(
			executor.cloned_origin(),
			Response::ExecutionResult(executor.error),
			response_info,
			FeeReason::Report,
		)?;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ReportHolding {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ReportHolding { response_info, assets } = self;
		// Note that we pass `None` as `maybe_failed_bin` since no assets were ever removed
		// from Holding.
		let assets =
		XcmExecutor::<Config>::reanchored(executor.holding.min(&assets), &response_info.destination, None);
		executor.respond(
			executor.cloned_origin(),
			Response::Assets(assets),
			response_info,
			FeeReason::Report,
		)?;
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ReportTransactStatus {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ReportTransactStatus(response_info) = self;
		executor.respond(
			executor.cloned_origin(),
			Response::DispatchResult(executor.transact_status.clone()),
			response_info,
			FeeReason::Report,
		)?;
		Ok(())
	}
}
