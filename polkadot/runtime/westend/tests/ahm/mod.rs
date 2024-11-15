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

//! Asset Hub Migrator tests.

mod account;

// Runtime specific imports
// Use general aliases for the imports to make it easier to copy&paste the tests for other runtimes.
use westend_runtime::{AhMigrator, Block, Runtime as T, System, *};

// General imports
use frame_support::sp_runtime::traits::Dispatchable;
use remote_externalities::{Builder, Mode, OfflineConfig, RemoteExternalities};

async fn remote_ext_test_setup() -> RemoteExternalities<Block> {
	sp_tracing::try_init_simple();

	let Some(snap) = std::env::var("SNAP").ok() else {
		panic!("SNAP environment variable is not set; use it to point to the snapshot");
	};
	let abs = std::path::absolute(snap.clone());

	Builder::<Block>::default()
		.mode(Mode::Offline(OfflineConfig { state_snapshot: snap.clone().into() }))
		.build()
		.await
		.map_err(|e| {
			eprintln!("Could not load from snapshot: {:?}: {:?}", abs, e);
		})
		.unwrap()
}
