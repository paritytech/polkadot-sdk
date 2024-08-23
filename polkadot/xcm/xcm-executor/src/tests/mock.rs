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

//! Mock types and XcmConfig for all executor unit tests.

use alloc::collections::btree_map::BTreeMap;
use codec::{Decode, Encode};
use core::cell::RefCell;
use frame_support::{
	dispatch::{DispatchInfo, DispatchResultWithPostInfo, GetDispatchInfo, PostDispatchInfo},
	parameter_types,
	traits::{Everything, Nothing, ProcessMessageError},
	weights::Weight,
};
use sp_runtime::traits::Dispatchable;
use xcm::prelude::*;

use crate::{
	traits::{DropAssets, Properties, ShouldExecute, TransactAsset, WeightBounds, WeightTrader},
	AssetsInHolding, Config, XcmExecutor,
};

/// We create an XCVM instance instead of calling `XcmExecutor::<_>::prepare_and_execute` so we
/// can inspect its fields.
pub fn instantiate_executor(
	origin: impl Into<Location>,
	message: Xcm<<XcmConfig as Config>::RuntimeCall>,
) -> (XcmExecutor<XcmConfig>, Weight) {
	let mut vm =
		XcmExecutor::<XcmConfig>::new(origin, message.using_encoded(sp_io::hashing::blake2_256));
	let weight = XcmExecutor::<XcmConfig>::prepare(message.clone()).unwrap().weight_of();
	vm.message_weight = weight;
	(vm, weight)
}

parameter_types! {
	pub const MaxAssetsIntoHolding: u32 = 10;
	pub const BaseXcmWeight: Weight = Weight::from_parts(1, 1);
	pub const MaxInstructions: u32 = 10;
	pub UniversalLocation: InteriorLocation = GlobalConsensus(ByGenesis([0; 32])).into();
}

/// Test origin.
#[derive(Debug)]
pub struct TestOrigin;

/// Test call.
///
/// Doesn't dispatch anything, has an empty implementation of [`Dispatchable`] that
/// just returns `Ok` with an empty [`PostDispatchInfo`].
#[derive(Debug, Encode, Decode, Eq, PartialEq, Clone, Copy, scale_info::TypeInfo)]
pub struct TestCall;
impl Dispatchable for TestCall {
	type RuntimeOrigin = TestOrigin;
	type Config = ();
	type Info = ();
	type PostInfo = PostDispatchInfo;

	fn dispatch(self, _origin: Self::RuntimeOrigin) -> DispatchResultWithPostInfo {
		Ok(PostDispatchInfo::default())
	}
}
impl GetDispatchInfo for TestCall {
	fn get_dispatch_info(&self) -> DispatchInfo {
		DispatchInfo::default()
	}
}

/// Test weigher that just returns a fixed weight for every program.
pub struct TestWeigher;
impl<C> WeightBounds<C> for TestWeigher {
	fn weight(_message: &mut Xcm<C>) -> Result<Weight, ()> {
		Ok(Weight::from_parts(2, 2))
	}

	fn instr_weight(_instruction: &Instruction<C>) -> Result<Weight, ()> {
		Ok(Weight::from_parts(2, 2))
	}
}

thread_local! {
	pub static ASSETS: RefCell<BTreeMap<Location, AssetsInHolding>> = RefCell::new(BTreeMap::new());
	pub static SENT_XCM: RefCell<Vec<(Location, Xcm<()>)>> = RefCell::new(Vec::new());
}

pub fn add_asset(who: impl Into<Location>, what: impl Into<Asset>) {
	ASSETS.with(|a| {
		a.borrow_mut()
			.entry(who.into())
			.or_insert(AssetsInHolding::new())
			.subsume(what.into())
	});
}

pub fn asset_list(who: impl Into<Location>) -> Vec<Asset> {
	Assets::from(assets(who)).into_inner()
}

pub fn assets(who: impl Into<Location>) -> AssetsInHolding {
	ASSETS.with(|a| a.borrow().get(&who.into()).cloned()).unwrap_or_default()
}

pub fn get_first_fungible(assets: &AssetsInHolding) -> Option<Asset> {
	assets.fungible_assets_iter().next()
}

/// Test asset transactor that withdraws from and deposits to a thread local assets storage.
pub struct TestAssetTransactor;
impl TransactAsset for TestAssetTransactor {
	fn deposit_asset(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(), XcmError> {
		add_asset(who.clone(), what.clone());
		Ok(())
	}

	fn withdraw_asset(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		ASSETS.with(|a| {
			a.borrow_mut()
				.get_mut(who)
				.ok_or(XcmError::NotWithdrawable)?
				.try_take(what.clone().into())
				.map_err(|_| XcmError::NotWithdrawable)
		})
	}
}

/// Test barrier that just lets everything through.
pub struct TestBarrier;
impl ShouldExecute for TestBarrier {
	fn should_execute<Call>(
		_origin: &Location,
		_instructions: &mut [Instruction<Call>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		Ok(())
	}
}

/// Test weight to fee that just multiplies `Weight.ref_time` and `Weight.proof_size`.
pub struct WeightToFee;
impl WeightToFee {
	pub fn weight_to_fee(weight: &Weight) -> u128 {
		weight.ref_time() as u128 * weight.proof_size() as u128
	}
}

/// Test weight trader that just buys weight with the native asset (`Here`) and
/// uses the test `WeightToFee`.
pub struct TestTrader {
	weight_bought_so_far: Weight,
}
impl WeightTrader for TestTrader {
	fn new() -> Self {
		Self { weight_bought_so_far: Weight::zero() }
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		payment: AssetsInHolding,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		let amount = WeightToFee::weight_to_fee(&weight);
		let required: Asset = (Here, amount).into();
		let unused = payment.checked_sub(required).map_err(|_| XcmError::TooExpensive)?;
		self.weight_bought_so_far.saturating_add(weight);
		Ok(unused)
	}

	fn refund_weight(&mut self, weight: Weight, _context: &XcmContext) -> Option<Asset> {
		let weight = weight.min(self.weight_bought_so_far);
		let amount = WeightToFee::weight_to_fee(&weight);
		self.weight_bought_so_far -= weight;
		if amount > 0 {
			Some((Here, amount).into())
		} else {
			None
		}
	}
}

/// Account where all dropped assets are deposited.
pub const TRAPPED_ASSETS: [u8; 32] = [255; 32];

/// Test asset trap that moves all dropped assets to the `TRAPPED_ASSETS` account.
pub struct TestAssetTrap;
impl DropAssets for TestAssetTrap {
	fn drop_assets(_origin: &Location, assets: AssetsInHolding, _context: &XcmContext) -> Weight {
		ASSETS.with(|a| {
			a.borrow_mut()
				.entry(TRAPPED_ASSETS.into())
				.or_insert(AssetsInHolding::new())
				.subsume_assets(assets)
		});
		Weight::zero()
	}
}

/// Test sender that always succeeds and puts messages in a dummy queue.
///
/// It charges `1` for the delivery fee.
pub struct TestSender;
impl SendXcm for TestSender {
	type Ticket = (Location, Xcm<()>);

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let ticket = (destination.take().unwrap(), message.take().unwrap());
		let delivery_fee: Asset = (Here, 1u128).into();
		Ok((ticket, delivery_fee.into()))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		SENT_XCM.with(|q| q.borrow_mut().push(ticket));
		Ok([0; 32])
	}
}

/// Gets queued test messages.
pub fn sent_xcm() -> Vec<(Location, Xcm<()>)> {
	SENT_XCM.with(|q| (*q.borrow()).clone())
}

/// Test XcmConfig that uses all the test implementations in this file.
pub struct XcmConfig;
impl Config for XcmConfig {
	type RuntimeCall = TestCall;
	type XcmSender = TestSender;
	type AssetTransactor = TestAssetTransactor;
	type OriginConverter = ();
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = TestBarrier;
	type Weigher = TestWeigher;
	type Trader = TestTrader;
	type ResponseHandler = ();
	type AssetTrap = TestAssetTrap;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = Self::RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = ();
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}
