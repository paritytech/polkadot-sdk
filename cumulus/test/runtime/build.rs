// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#[cfg(feature = "std")]
fn main() {
	use substrate_wasm_builder::WasmBuilder;

	WasmBuilder::build_using_defaults();

	WasmBuilder::init_with_defaults()
		.enable_feature("increment-spec-version")
		.set_file_name("wasm_binary_spec_version_incremented.rs")
		.build();
}

#[cfg(not(feature = "std"))]
fn main() {}
