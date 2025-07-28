// Copyright (C) Polkadot Fellows.
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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

use frame::weights_prelude::*;

/// Weight functions needed for `pallet_remote_proxy`.
pub trait WeightInfo {
	fn remote_proxy_with_registered_proof() -> Weight;
	fn register_remote_proxy_proof() -> Weight;
	fn remote_proxy() -> Weight;
}

impl WeightInfo for () {
	fn remote_proxy_with_registered_proof() -> Weight {
		Weight::MAX
	}

	fn register_remote_proxy_proof() -> Weight {
		Weight::MAX
	}

	fn remote_proxy() -> Weight {
		Weight::MAX
	}
}
