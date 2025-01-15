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
use crate::{config, Junctions, XcmExecutor};
use frame_support::{
	ensure,
	traits::{Contains, ContainsPair, Get},
};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;

impl<Config: config::Config> ExecuteInstruction<Config> for DescendOrigin {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		executor.do_descend_origin(self.0)
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ClearOrigin {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		executor.do_clear_origin()
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExecuteWithOrigin<Config::RuntimeCall> {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExecuteWithOrigin { descendant_origin, xcm } = self;
		let previous_origin = executor.context.origin.clone();

		// Set new temporary origin.
		if let Some(who) = descendant_origin {
			executor.do_descend_origin(who)?;
		} else {
			executor.do_clear_origin()?;
		}
		// Process instructions.
		let result = executor.process(xcm).map_err(|error| {
			tracing::error!(target: "xcm::execute", ?error, actual_origin = ?executor.context.origin, original_origin = ?previous_origin, "ExecuteWithOrigin inner xcm failure");
			error.xcm_error
		});
		// Reset origin to previous one.
		executor.context.origin = previous_origin;
		result
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for UniversalOrigin {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let UniversalOrigin(new_global) = self;
		let universal_location = Config::UniversalLocation::get();
		ensure!(universal_location.first() != Some(&new_global), XcmError::InvalidLocation);
		let origin = executor.cloned_origin().ok_or(XcmError::BadOrigin)?;
		let origin_xform = (origin, new_global);
		let ok = Config::UniversalAliases::contains(&origin_xform);
		ensure!(ok, XcmError::InvalidLocation);
		let (_, new_global) = origin_xform;
		let new_origin = Junctions::from([new_global]).relative_to(&universal_location);
		executor.context.origin = Some(new_origin);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for AliasOrigin {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let AliasOrigin(target) = self;
		let origin = executor.origin_ref().ok_or(XcmError::BadOrigin)?;
		if Config::Aliasers::contains(origin, &target) {
			executor.context.origin = Some(target);
			Ok(())
		} else {
			Err(XcmError::NoPermission)
		}
	}
}
