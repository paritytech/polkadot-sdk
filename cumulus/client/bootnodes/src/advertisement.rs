// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Parachain bootnodes advertisement.

use sc_network::service::traits::NetworkService;
use std::sync::Arc;

pub struct BootnodeAdvertisement {
	network_service: Arc<dyn NetworkService>,
}

impl BootnodeAdvertisement {
	pub fn new(network_service: Arc<dyn NetworkService>) -> Self {
		Self { network_service }
	}

	pub async fn run(self) {}
}
