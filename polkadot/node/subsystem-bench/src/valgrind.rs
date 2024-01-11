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

#[cfg(target_os = "linux")]
const LOG_FILE: &str = "cachegrind_logs";
#[cfg(target_os = "linux")]
const REPORT_START: &str = "I refs:";

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
		.arg(format!("--log-file={}", LOG_FILE))
		.args(std::env::args())
		.exec();

	return Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn relaunch_in_valgrind_mode() -> eyre::Result<()> {
	return Err(eyre::eyre!("Valgrind can be executed only on linux"));
}

#[cfg(target_os = "linux")]
fn prepare_report() -> eyre::Result<String> {
	let log_file = std::fs::read_to_string(LOG_FILE)?;
	let lines: Vec<&str> = log_file.lines().collect();
	let start = lines
		.iter()
		.position(|line| line.contains(REPORT_START))
		.ok_or(eyre::eyre!("Log file {} does not contain cache misses report", LOG_FILE))?;
	let lines: Vec<&str> = lines
		.iter()
		.skip(start)
		.map(|line| line.trim_start_matches(|c: char| !c.is_alphabetic()))
		.collect();

	Ok(format!("\nCache misses report:\n\n\t{}", lines.join("\n\t")))
}

#[cfg(target_os = "linux")]
pub(crate) fn dispay_report() -> eyre::Result<()> {
	gum::info!("{}", prepare_report()?);

	Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn dispay_report() -> eyre::Result<()> {
	Ok(())
}
