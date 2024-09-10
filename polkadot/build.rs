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

#[cfg(not(feature = "std"))]
fn main() {
	substrate_build_script_utils::generate_cargo_keys();
	// For the node/worker version check, make sure we always rebuild the node and binary workers
	// when the version changes.
	substrate_build_script_utils::rerun_if_git_head_changed();
}

#[cfg(feature = "std")]
fn main() {
	substrate_build_script_utils::generate_cargo_keys();
	// For the node/worker version check, make sure we always rebuild the node and binary workers
	// when the version changes.
	substrate_build_script_utils::rerun_if_git_head_changed();

	substrate_wasm_builder::WasmBuilder::init_with_defaults()
		.append_to_rust_flags("-Clink-args=--initial-memory=127108864")
		.append_to_rust_flags("-Clink-args=--max-memory=127108864")
		.build();
}
