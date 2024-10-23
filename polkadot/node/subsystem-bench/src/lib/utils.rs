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

//! Test utils

use std::{fs::File, io::Write};

// Saves a given string to a file
pub fn save_to_file(path: &str, value: String) -> color_eyre::eyre::Result<()> {
	let output = std::process::Command::new(env!("CARGO"))
		.arg("locate-project")
		.arg("--workspace")
		.arg("--message-format=plain")
		.output()
		.unwrap()
		.stdout;
	let workspace_dir = std::path::Path::new(std::str::from_utf8(&output).unwrap().trim())
		.parent()
		.unwrap();
	let path = workspace_dir.join(path);
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
	}
	let mut file = File::create(path)?;
	file.write_all(value.as_bytes())?;

	Ok(())
}
