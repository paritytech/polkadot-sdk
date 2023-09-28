// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

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

#[cfg(feature = "std")]
fn main() {
	// note: needs to be synced with rococo-runtime-constants::time hard-coded string literal
	const ROCOCO_EPOCH_DURATION_ENV: &str = "ROCOCO_EPOCH_DURATION";

	substrate_wasm_builder::WasmBuilder::new()
		.with_current_project()
		.import_memory()
		.export_heap_base()
		.build();

	println!("cargo:rerun-if-env-changed={}", ROCOCO_EPOCH_DURATION_ENV);
}

#[cfg(not(feature = "std"))]
fn main() {}
