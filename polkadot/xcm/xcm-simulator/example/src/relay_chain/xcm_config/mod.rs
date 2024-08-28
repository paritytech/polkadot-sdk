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

pub mod asset_transactor;
pub mod barrier;
pub mod constants;
pub mod location_converter;
pub mod origin_converter;
pub mod weigher;

use crate::relay_chain::{RuntimeCall, XcmPallet};
use frame_support::traits::{Everything, Nothing};
use xcm_builder::{EnsureDecodableXcm, FixedRateOfFungible, FrameTransactionalProcessor};
use xcm_executor::Config;

// Generated from `decl_test_network!`
pub type XcmRouter = EnsureDecodableXcm<crate::RelayChainXcmRouter>;

pub struct XcmConfig;
impl Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = asset_transactor::AssetTransactor;
	type OriginConverter = origin_converter::OriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = constants::UniversalLocation;
	type Barrier = barrier::Barrier;
	type Weigher = weigher::Weigher;
	type Trader = FixedRateOfFungible<constants::TokensPerSecondPerByte, ()>;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = XcmPallet;
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type FeeManager = ();
	type MaxAssetsIntoHolding = constants::MaxAssetsIntoHolding;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = XcmPallet;
}
