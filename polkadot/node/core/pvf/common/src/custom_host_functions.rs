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

use sp_core::{LogLevel, LogLevelFilter};
use sp_externalities::{decl_extension, ExternalitiesExt};
use sp_runtime_interface::{
	pass_by::{PassAs, PassFatPointerAndRead, ReturnAs},
	runtime_interface,
};

decl_extension! {
	/// Enables logging when registered.
	pub(crate) struct EnableLogging;
}

#[runtime_interface]
pub(crate) trait Logging {
	/// Request to print a log message on the host.
	///
	/// Note that this will be only displayed if the host is enabled to display log messages with
	/// given level and target.
	///
	/// Instead of using directly, prefer setting up `RuntimeLogger` and using `log` macros.
	fn log(
		&mut self,
		level: PassAs<LogLevel, u8>,
		target: PassFatPointerAndRead<&str>,
		message: PassFatPointerAndRead<&[u8]>,
	) {
		if self.extension::<EnableLogging>().is_none() {
			return
		}

		if let Ok(message) = core::str::from_utf8(message) {
			log::log!(target: target, log::Level::from(level), "{}", message)
		}
	}

	/// Returns the max log level used by the host.
	fn max_level(&mut self) -> ReturnAs<LogLevelFilter, u8> {
		if self.extension::<EnableLogging>().is_none() {
			LogLevelFilter::Off
		} else {
			log::max_level().into()
		}
	}
}
