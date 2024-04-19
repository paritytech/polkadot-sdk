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
pub mod limits;
pub mod locations;
pub mod origin_converter;
pub mod reserve;
pub mod teleporter;

pub use asset_transactor::*;
pub use limits::*;
pub use locations::*;

use frame_support::traits::{Everything, Nothing};
use xcm_builder::{FixedRateOfFungible, FrameTransactionalProcessor};

use crate::parachain::{MsgQueue, PolkadotXcm, RuntimeCall};

// Generated from `decl_test_network!`
pub type XcmRouter = crate::ParachainXcmRouter<MsgQueue>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = asset_transactor::LocalAssetTransactor;
	type OriginConverter = origin_converter::XcmOriginToCallOrigin;
	type IsReserve = reserve::TrustedReserves;
	type IsTeleporter = teleporter::TrustedTeleporters;
	type UniversalLocation = UniversalLocation;
	type Barrier = barrier::Barrier;
	type Weigher = limits::Weigher;
	type Trader = FixedRateOfFungible<limits::KsmPerSecondPerByte, ()>;
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = PolkadotXcm;
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type FeeManager = ();
	type MaxAssetsIntoHolding = limits::MaxAssetsIntoHolding;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
}
