// Copyright 2019 Parity Technologies (UK) Ltd.
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

use wasm_builder_runner::WasmBuilder;

fn main() {
	WasmBuilder::new()
		.with_current_project()
		.with_wasm_builder_from_git("https://github.com/paritytech/substrate.git", "0b30207969fdde85e0dad785750757399cd0e3e4")
		.export_heap_base()
		.import_memory()
		.build()
}
