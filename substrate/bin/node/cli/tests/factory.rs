// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use std::process::{Command, Stdio};
use tempfile::tempdir;

mod common;

#[test]
fn factory_works() {
	let base_path = tempdir().expect("could not create a temp dir");

	let status = Command::new(cargo_bin("substrate"))
		.stdout(Stdio::null())
		.args(&["factory", "--dev", "-d"])
		.arg(base_path.path())
		.status()
		.unwrap();
	assert!(status.success());

	// Make sure that the `dev` chain folder exists & `db`
	assert!(base_path.path().join("chains/dev/").exists());
	assert!(base_path.path().join("chains/dev/db").exists());
}
