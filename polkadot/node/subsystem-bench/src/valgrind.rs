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

use color_eyre::eyre;

/// Show if the app is running under Valgrind
#[cfg(target_os = "linux")]
pub(crate) fn is_valgrind_running() -> bool {
	!matches!(crabgrind::run_mode(), crabgrind::RunMode::Native)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_valgrind_running() -> bool {
	false
}

/// Start collecting cache misses data
#[cfg(target_os = "linux")]
pub(crate) fn start_measuring() {
	crabgrind::cachegrind::start_instrumentation();
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn start_measuring() {}

/// Stop collecting cache misses data
#[cfg(target_os = "linux")]
pub(crate) fn stop_measuring() {
	crabgrind::cachegrind::stop_instrumentation();
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn stop_measuring() {}

/// Stop execution and relaunch the app under valgrind
#[cfg(target_os = "linux")]
pub(crate) fn relaunch_in_valgrind_mode() -> eyre::Result<()> {
	use std::os::unix::process::CommandExt;
	std::process::Command::new("valgrind")
		.arg("--tool=cachegrind")
		.arg("--cache-sim=yes")
		.arg("--instr-at-start=no")
		.arg("--log-file=cachegrind_report.txt")
		.args(std::env::args())
		.exec();

	return Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn relaunch_in_valgrind_mode() -> eyre::Result<()> {
	return Err(eyre::eyre!("Valgrind can be executed only on linux"));
}
