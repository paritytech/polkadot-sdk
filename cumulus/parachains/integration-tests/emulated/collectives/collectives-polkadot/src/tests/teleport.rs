// Copyright Parity Technologies (UK) Ltd.
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

use crate::*;

#[test]
fn teleport_to_other_system_parachains_works() {
	let amount = COLLECTIVES_POLKADOT_ED * 100;
	let native_asset: VersionedMultiAssets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		CollectivesPolkadot,                       // Origin
		vec![AssetHubPolkadot, BridgeHubPolkadot], // Destinations
		(native_asset, amount)
	);
}
