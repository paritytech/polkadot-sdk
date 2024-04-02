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

//! UI tests for XCM procedural macros

#[cfg(not(feature = "disable-ui-tests"))]
#[test]
fn ui() {
	// Only run the ui tests when `RUN_UI_TESTS` is set.
	if std::env::var("RUN_UI_TESTS").is_err() {
		return
	}

	// As trybuild is using `cargo check`, we don't need the real WASM binaries.
	std::env::set_var("SKIP_WASM_BUILD", "1");

	let t = trybuild::TestCases::new();
	t.compile_fail("tests/ui/**/*.rs");
}
