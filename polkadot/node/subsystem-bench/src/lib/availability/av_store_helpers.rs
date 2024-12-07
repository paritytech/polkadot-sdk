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

use crate::{environment::TestEnvironmentDependencies, mock::TestSyncOracle};
use polkadot_node_core_av_store::{AvailabilityStoreSubsystem, Config};
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_subsystem_util::database::Database;
use std::sync::Arc;

mod columns {
	pub const DATA: u32 = 0;
	pub const META: u32 = 1;
	pub const NUM_COLUMNS: u32 = 2;
}

const TEST_CONFIG: Config = Config { col_data: columns::DATA, col_meta: columns::META };

pub fn new_av_store(dependencies: &TestEnvironmentDependencies) -> AvailabilityStoreSubsystem {
	let metrics = Metrics::try_register(&dependencies.registry).unwrap();

	AvailabilityStoreSubsystem::new(test_store(), TEST_CONFIG, Box::new(TestSyncOracle {}), metrics)
}

fn test_store() -> Arc<dyn Database> {
	let db = kvdb_memorydb::create(columns::NUM_COLUMNS);
	let db =
		polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[columns::META]);
	Arc::new(db)
}
