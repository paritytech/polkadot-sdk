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

use std::{env, path::PathBuf};

use vergen::{generate_cargo_keys, ConstantsFlags};

const ERROR_MSG: &str = "Failed to generate metadata files";

fn main() {
	generate_cargo_keys(ConstantsFlags::SHA_SHORT).expect(ERROR_MSG);

	let mut manifest_dir = PathBuf::from(
		env::var("CARGO_MANIFEST_DIR").expect("`CARGO_MANIFEST_DIR` is always set by cargo."),
	);

	while manifest_dir.parent().is_some() {
		if manifest_dir.join(".git/HEAD").exists() {
			println!(
				"cargo:rerun-if-changed={}",
				manifest_dir.join(".git/HEAD").display()
			);
			return;
		}

		manifest_dir.pop();
	}

	println!("cargo:warning=Could not find `.git/HEAD` from manifest dir!");
}
