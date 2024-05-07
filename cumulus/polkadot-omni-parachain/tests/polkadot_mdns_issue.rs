// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.

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

use tempfile::tempdir;

mod common;

#[tokio::test]
#[cfg(unix)]
#[ignore]
async fn interrupt_polkadot_mdns_issue_test() {
	use nix::sys::signal::Signal::{SIGINT, SIGTERM};

	let base_dir = tempdir().expect("could not create a temp dir");

	let args = &["--", "--chain=rococo-local"];

	common::run_node_for_a_while(base_dir.path(), args, SIGINT).await;
	common::run_node_for_a_while(base_dir.path(), args, SIGTERM).await;
}
