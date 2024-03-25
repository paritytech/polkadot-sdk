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

use std::{fs::File, io::Write};

fn workspace_dir() -> String {
	let output = std::process::Command::new(env!("CARGO"))
		.arg("locate-project")
		.arg("--workspace")
		.arg("--message-format=plain")
		.output()
		.unwrap()
		.stdout;
	let cargo_path = std::path::Path::new(std::str::from_utf8(&output).unwrap().trim());
	format!("{}", cargo_path.parent().unwrap().display())
}

// Saves a given string to a file
pub fn save_to_file(path: &str, value: String) -> color_eyre::eyre::Result<()> {
	let mut path: Vec<&str> = path.split('/').collect();
	let filename = path.pop().expect("Should contain a file name");
	let dir = format!("{}/{}", workspace_dir(), path.join("/"));

	if !path.is_empty() {
		std::fs::create_dir_all(&dir)?;
	}
	let mut file = File::create(format!("{}/{}", dir, filename))?;
	file.write_all(value.as_bytes())?;

	Ok(())
}
