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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use frame_support::weights::Weight;
use pallet_xcm::WeightInfo;
pub struct ReviveTestWeightInfo;
impl WeightInfo for ReviveTestWeightInfo {
	fn send() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn teleport_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn reserve_transfer_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn transfer_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn execute() -> Weight {
		Weight::from_parts(100_000_000, 2_000)
	}

	fn force_xcm_version() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_default_xcm_version() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_subscribe_version_notify() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_unsubscribe_version_notify() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn force_suspension() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_supported_version() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_version_notifiers() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn already_notified_target() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn notify_current_targets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn notify_target_migration_fail() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_version_notify_targets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn migrate_and_notify_old_targets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn new_query() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn take_response() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}

	fn claim_assets() -> Weight {
		Weight::from_parts(100_000_000, 0)
	}
}
