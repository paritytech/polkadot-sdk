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

//! Unit tests related to the `fees` register and `PayFees` instruction.
//!
//! See [Fellowship RFC 105](https://github.com/polkadot-fellows/rfCs/pull/105)
//! and the [specification](https://github.com/polkadot-fellows/xcm-format) for more information.

use alloc::collections::btree_map::BTreeMap;
use codec::{Encode, Decode};
use core::cell::RefCell;
use frame_support::{parameter_types, traits::{Everything, Nothing}, weights::Weight, dispatch::{DispatchResultWithPostInfo, PostDispatchInfo, GetDispatchInfo, DispatchInfo}};
use sp_runtime::traits::Dispatchable;
use xcm::prelude::*;

use crate::{AssetsInHolding, traits::{TransactAsset, WeightBounds}, XcmExecutor};

#[test]
fn works_for_execution_fees() {
    let xcm = Xcm::<TestCall>::builder()
        .withdraw_asset((Here, 100u128))
        .pay_fees((Here, 10u128)) // 10% destined for fees, not more.
        .deposit_asset(All, [1; 32])
        .build();

    let who = Location::new(0, [AccountId32 { id: [0; 32], network: None }]);

    let result = XcmExecutor::<XcmConfig>::prepare_and_execute(who, xcm.clone(), &mut xcm.using_encoded(sp_io::hashing::blake2_256), Weight::MAX, Weight::zero());

    dbg!(&result);
}

parameter_types! {
    pub const MaxAssetsIntoHolding: u32 = 10;
    pub const BaseXcmWeight: Weight = Weight::from_parts(1, 1);
    pub const MaxInstructions: u32 = 10;
    pub UniversalLocation: InteriorLocation = GlobalConsensus(ByGenesis([0; 32])).into();
}

/// Dummy origin.
#[derive(Debug)]
pub struct TestOrigin;

/// Dummy call.
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

pub struct TestWeigher;
impl<C> WeightBounds<C> for TestWeigher {
    fn weight(_message: &mut Xcm<C>) -> Result<Weight, ()> {
        Ok(Weight::from_parts(1, 1))
    }

    fn instr_weight(_instruction: &Instruction<C>) -> Result<Weight, ()> {
        Ok(Weight::from_parts(1, 1))
    }
}

thread_local! {
	pub static ASSETS: RefCell<BTreeMap<Location, AssetsInHolding>> = RefCell::new(BTreeMap::new());
}

pub struct TestAssetTransactor;
impl TransactAsset for TestAssetTransactor {
    fn deposit_asset(
        what: &Asset,
        who: &Location,
        _context: Option<&XcmContext>,
    ) -> Result<(), XcmError> {
    	ASSETS.with(|a| {
    		a.borrow_mut()
    			.entry(who.clone().into())
    			.or_insert(AssetsInHolding::new())
    			.subsume(what.clone().into())
    	});
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

pub struct XcmConfig;
impl crate::Config for XcmConfig {
	type RuntimeCall = TestCall;
	type XcmSender = ();
	type AssetTransactor = TestAssetTransactor;
	type OriginConverter = ();
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = ();
	type Weigher = TestWeigher;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
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
