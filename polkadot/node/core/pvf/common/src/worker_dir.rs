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

//! Shared functions for getting the known worker files.

use std::path::{Path, PathBuf};

const WORKER_EXECUTE_ARTIFACT_NAME: &str = "artifact";
const WORKER_PREPARE_TMP_ARTIFACT_NAME: &str = "tmp-artifact";

pub fn execute_artifact(worker_dir_path: &Path) -> PathBuf {
	worker_dir_path.join(WORKER_EXECUTE_ARTIFACT_NAME)
}

pub fn prepare_tmp_artifact(worker_dir_path: &Path) -> PathBuf {
	worker_dir_path.join(WORKER_PREPARE_TMP_ARTIFACT_NAME)
}
