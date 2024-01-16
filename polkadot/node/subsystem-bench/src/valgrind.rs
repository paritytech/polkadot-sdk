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
pub(crate) fn is_valgrind_running() -> bool {
	match std::env::var("LD_PRELOAD") {
		Ok(v) => v.contains("valgrind"),
		Err(_) => false,
	}
}

/// Stop execution and relaunch the app under valgrind
/// Cache configuration used to emulate Intel Ice Lake (size, associativity, line size):
///     L1 instruction: 32,768 B, 8-way, 64 B lines
///     L1 data: 49,152 B, 12-way, 64 B lines
///     Last-level: 2,097,152 B, 16-way, 64 B lines
pub(crate) fn relaunch_in_valgrind_mode() -> eyre::Result<()> {
	use std::os::unix::process::CommandExt;
	let err = std::process::Command::new("valgrind")
		.arg("--tool=cachegrind")
		.arg("--cache-sim=yes")
		.arg("--log-file=cachegrind_report.txt")
		.arg("--I1=32768,8,64")
		.arg("--D1=49152,12,64")
		.arg("--LL=2097152,16,64")
		.arg("--verbose")
		.args(std::env::args())
		.exec();

	Err(eyre::eyre!(
		"Сannot run Valgrind, check that it is installed and available in the PATH\n{}",
		err
	))
}
