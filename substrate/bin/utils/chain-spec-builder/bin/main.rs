// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use chain_spec_builder::ChainSpecBuilder;
use clap::Parser;
use staging_chain_spec_builder as chain_spec_builder;

//avoid error message escaping
fn main() {
	match inner_main() {
		Err(e) => eprintln!("{}", format!("{e}")),
		_ => {},
	}
}

fn inner_main() -> Result<(), String> {
	sp_tracing::try_init_simple();

	let builder = ChainSpecBuilder::parse();
	builder.run()
}
