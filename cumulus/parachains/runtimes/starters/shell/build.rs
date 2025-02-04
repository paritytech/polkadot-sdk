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
	substrate_wasm_builder::WasmBuilder::new()
		.with_current_project()
		.export_heap_base()
		.import_memory()
		.build()
}

<<<<<<< HEAD:cumulus/parachains/runtimes/starters/shell/build.rs
#[cfg(not(feature = "std"))]
fn main() {}
=======
pub mod cli;
mod command;
mod common;
mod fake_runtime_api;
mod nodes;

pub use cli::CliConfig;
pub use command::{run, RunConfig};
pub use common::{chain_spec, runtime};
pub use nodes::NODE_VERSION;
>>>>>>> 3fb7c8c6 (Align omni-node and polkadot-parachain versions (#7367)):cumulus/polkadot-omni-node/lib/src/lib.rs
