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

use crate::{xcm_config::AssetTransactors, Runtime, RuntimeEvent};
use frame_support::{parameter_types, traits::Everything};
use hex_literal::hex;
use pallet_xcm::EnsureXcm;
use xcm::prelude::{AccountKey20, Asset, Ethereum, GlobalConsensus, Location, SendXcm};

#[cfg(not(feature = "runtime-benchmarks"))]
use crate::xcm_config::XcmRouter;
#[cfg(feature = "runtime-benchmarks")]
use benchmark_helpers::DoNothingRouter;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmark_helpers {
	use crate::RuntimeOrigin;
	use codec::Encode;

	pub struct DoNothingRouter;
	impl SendXcm for DoNothingRouter {
		type Ticket = Xcm<()>;

		fn validate(
			_dest: &mut Option<Location>,
			xcm: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			Ok((xcm.clone().unwrap(), Assets::new()))
		}
		fn deliver(xcm: Xcm<()>) -> Result<XcmHash, SendError> {
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			Ok(hash)
		}
	}

	impl snowbridge_system_frontend::BenchmarkHelper<RuntimeOrigin> for () {
		fn make_xcm_origin(location: Location) -> RuntimeOrigin {
			RuntimeOrigin::from(pallet_xcm::Origin::Xcm(location))
		}
	}
}

parameter_types! {
	pub storage WETH: Location = Location::new(
			2,
			[
				GlobalConsensus(Ethereum { chain_id: 11155111 }),
				AccountKey20 {
					network: None,
					key: hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
				},
			],
	);
	pub storage DeliveryFee: Asset = (Location::parent(), 80_000_000_000u128).into();
}

impl snowbridge_system_frontend::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
	type CreateAgentOrigin = EnsureXcm<Everything>;
	type RegisterTokenOrigin = EnsureXcm<Everything>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type XcmSender = XcmRouter;
	#[cfg(feature = "runtime-benchmarks")]
	type XcmSender = DoNothingRouter;
	type AssetTransactor = AssetTransactors;
	type WETH = WETH;
	type DeliveryFee = DeliveryFee;
}
