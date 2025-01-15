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
use crate::{config, XcmExecutor};
use xcm::latest::instructions::*;
use xcm::latest::Error as XcmError;
use frame_support::{ensure, traits::PalletsInfoAccess};

impl<Config: config::Config> ExecuteInstruction<Config> for ExpectAsset {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExpectAsset(assets) = self;
		executor.holding.ensure_contains(&assets).map_err(|e| {
			tracing::error!(target: "xcm::process_instruction::expect_asset", ?e, ?assets, "assets not contained in holding");
			XcmError::ExpectationFalse
		})
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExpectOrigin {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExpectOrigin(origin) = self;
		ensure!(executor.context.origin == origin, XcmError::ExpectationFalse);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExpectError {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExpectError(error) = self;
		ensure!(executor.error == error, XcmError::ExpectationFalse);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExpectTransactStatus {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExpectTransactStatus(transact_status) = self;
		ensure!(executor.transact_status == transact_status, XcmError::ExpectationFalse);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExpectPallet {
	fn execute(self, _executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExpectPallet { index, name, module_name, crate_major, min_crate_minor } = self;
		let pallet = Config::PalletInstancesInfo::infos()
			.into_iter()
			.find(|x| x.index == index as usize)
			.ok_or(XcmError::PalletNotFound)?;
		ensure!(pallet.name.as_bytes() == &name[..], XcmError::NameMismatch);
		ensure!(pallet.module_name.as_bytes() == &module_name[..], XcmError::NameMismatch);
		let major = pallet.crate_version.major as u32;
		ensure!(major == crate_major, XcmError::VersionIncompatible);
		let minor = pallet.crate_version.minor as u32;
		ensure!(minor >= min_crate_minor, XcmError::VersionIncompatible);
		Ok(())
	}
}
