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

//! Mock implementations to test XCM builder configuration types.

use crate::{
	barriers::{AllowSubscriptionsFrom, RespectSuspension, TrailingSetTopicAsId},
	test_utils::*,
	EnsureDecodableXcm,
};
pub use crate::{
	AliasForeignAccountId32, AllowExplicitUnpaidExecutionFrom, AllowKnownQueryResponses,
	AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, FixedRateOfFungible,
	FixedWeightBounds, TakeWeightCredit,
};
pub use alloc::collections::{btree_map::BTreeMap, btree_set::BTreeSet};
pub use codec::{Decode, Encode};
pub use core::{
	cell::{Cell, RefCell},
	fmt::Debug,
};
use frame_support::traits::{ContainsPair, Everything};
pub use frame_support::{
	dispatch::{DispatchInfo, DispatchResultWithPostInfo, GetDispatchInfo, PostDispatchInfo},
	ensure, parameter_types,
	sp_runtime::{traits::Dispatchable, DispatchError, DispatchErrorWithPostInfo},
	traits::{Contains, Get, IsInVec},
};
pub use xcm::latest::{prelude::*, QueryId, Weight};
use xcm_executor::traits::{Properties, QueryHandler, QueryResponseStatus};
pub use xcm_executor::{
	traits::{
		AssetExchange, AssetLock, CheckSuspension, ConvertOrigin, Enact, ExportXcm, FeeManager,
		FeeReason, LockError, OnResponse, TransactAsset,
	},
	AssetsInHolding, Config,
};

#[derive(Debug)]
pub enum TestOrigin {
	Root,
	Relay,
	Signed(u64),
	Parachain(u32),
}

/// A dummy call.
///
/// Each item contains the amount of weight that it *wants* to consume as the first item, and the
/// actual amount (if different from the former) in the second option.
#[derive(Debug, Encode, Decode, Eq, PartialEq, Clone, Copy, scale_info::TypeInfo)]
pub enum TestCall {
	OnlyRoot(Weight, Option<Weight>),
	OnlyParachain(Weight, Option<Weight>, Option<u32>),
	OnlySigned(Weight, Option<Weight>, Option<u64>),
	Any(Weight, Option<Weight>),
}
impl Dispatchable for TestCall {
	type RuntimeOrigin = TestOrigin;
	type Config = ();
	type Info = ();
	type PostInfo = PostDispatchInfo;
	fn dispatch(self, origin: Self::RuntimeOrigin) -> DispatchResultWithPostInfo {
		let mut post_info = PostDispatchInfo::default();
		let maybe_actual = match self {
			TestCall::OnlyRoot(_, maybe_actual) |
			TestCall::OnlySigned(_, maybe_actual, _) |
			TestCall::OnlyParachain(_, maybe_actual, _) |
			TestCall::Any(_, maybe_actual) => maybe_actual,
		};
		post_info.actual_weight = maybe_actual;
		if match (&origin, &self) {
			(TestOrigin::Parachain(i), TestCall::OnlyParachain(_, _, Some(j))) => i == j,
			(TestOrigin::Signed(i), TestCall::OnlySigned(_, _, Some(j))) => i == j,
			(TestOrigin::Root, TestCall::OnlyRoot(..)) |
			(TestOrigin::Parachain(_), TestCall::OnlyParachain(_, _, None)) |
			(TestOrigin::Signed(_), TestCall::OnlySigned(_, _, None)) |
			(_, TestCall::Any(..)) => true,
			_ => false,
		} {
			Ok(post_info)
		} else {
			Err(DispatchErrorWithPostInfo { error: DispatchError::BadOrigin, post_info })
		}
	}
}

impl GetDispatchInfo for TestCall {
	fn get_dispatch_info(&self) -> DispatchInfo {
		let call_weight = *match self {
			TestCall::OnlyRoot(estimate, ..) |
			TestCall::OnlyParachain(estimate, ..) |
			TestCall::OnlySigned(estimate, ..) |
			TestCall::Any(estimate, ..) => estimate,
		};
		DispatchInfo { call_weight, ..Default::default() }
	}
}

thread_local! {
	pub static SENT_XCM: RefCell<Vec<(Location, Xcm<()>, XcmHash)>> = RefCell::new(Vec::new());
	pub static EXPORTED_XCM: RefCell<
		Vec<(NetworkId, u32, InteriorLocation, InteriorLocation, Xcm<()>, XcmHash)>
	> = RefCell::new(Vec::new());
	pub static EXPORTER_OVERRIDE: RefCell<Option<(
		fn(
			NetworkId,
			u32,
			&InteriorLocation,
			&InteriorLocation,
			&Xcm<()>,
		) -> Result<Assets, SendError>,
		fn(
			NetworkId,
			u32,
			InteriorLocation,
			InteriorLocation,
			Xcm<()>,
		) -> Result<XcmHash, SendError>,
	)>> = RefCell::new(None);
	pub static SEND_PRICE: RefCell<Assets> = RefCell::new(Assets::new());
	pub static SUSPENDED: Cell<bool> = Cell::new(false);
}
pub fn sent_xcm() -> Vec<(Location, opaque::Xcm, XcmHash)> {
	SENT_XCM.with(|q| (*q.borrow()).clone())
}
pub fn set_send_price(p: impl Into<Asset>) {
	SEND_PRICE.with(|l| l.replace(p.into().into()));
}
pub fn exported_xcm(
) -> Vec<(NetworkId, u32, InteriorLocation, InteriorLocation, opaque::Xcm, XcmHash)> {
	EXPORTED_XCM.with(|q| (*q.borrow()).clone())
}
pub fn set_exporter_override(
	price: fn(
		NetworkId,
		u32,
		&InteriorLocation,
		&InteriorLocation,
		&Xcm<()>,
	) -> Result<Assets, SendError>,
	deliver: fn(
		NetworkId,
		u32,
		InteriorLocation,
		InteriorLocation,
		Xcm<()>,
	) -> Result<XcmHash, SendError>,
) {
	EXPORTER_OVERRIDE.with(|x| x.replace(Some((price, deliver))));
}
#[allow(dead_code)]
pub fn clear_exporter_override() {
	EXPORTER_OVERRIDE.with(|x| x.replace(None));
}
pub struct TestMessageSenderImpl;
impl SendXcm for TestMessageSenderImpl {
	type Ticket = (Location, Xcm<()>, XcmHash);
	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<(Location, Xcm<()>, XcmHash)> {
		let msg = msg.take().unwrap();
		let hash = fake_message_hash(&msg);
		let triplet = (dest.take().unwrap(), msg, hash);
		Ok((triplet, SEND_PRICE.with(|l| l.borrow().clone())))
	}
	fn deliver(triplet: (Location, Xcm<()>, XcmHash)) -> Result<XcmHash, SendError> {
		let hash = triplet.2;
		SENT_XCM.with(|q| q.borrow_mut().push(triplet));
		Ok(hash)
	}
}
pub type TestMessageSender = EnsureDecodableXcm<TestMessageSenderImpl>;

pub struct TestMessageExporter;
impl ExportXcm for TestMessageExporter {
	type Ticket = (NetworkId, u32, InteriorLocation, InteriorLocation, Xcm<()>, XcmHash);
	fn validate(
		network: NetworkId,
		channel: u32,
		uni_src: &mut Option<InteriorLocation>,
		dest: &mut Option<InteriorLocation>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<(NetworkId, u32, InteriorLocation, InteriorLocation, Xcm<()>, XcmHash)> {
		let (s, d, m) = (uni_src.take().unwrap(), dest.take().unwrap(), msg.take().unwrap());
		let r: Result<Assets, SendError> = EXPORTER_OVERRIDE.with(|e| {
			if let Some((ref f, _)) = &*e.borrow() {
				f(network, channel, &s, &d, &m)
			} else {
				Ok(Assets::new())
			}
		});
		let h = fake_message_hash(&m);
		match r {
			Ok(price) => Ok(((network, channel, s, d, m, h), price)),
			Err(e) => {
				*uni_src = Some(s);
				*dest = Some(d);
				*msg = Some(m);
				Err(e)
			},
		}
	}
	fn deliver(
		tuple: (NetworkId, u32, InteriorLocation, InteriorLocation, Xcm<()>, XcmHash),
	) -> Result<XcmHash, SendError> {
		EXPORTER_OVERRIDE.with(|e| {
			if let Some((_, ref f)) = &*e.borrow() {
				let (network, channel, uni_src, dest, msg, _hash) = tuple;
				f(network, channel, uni_src, dest, msg)
			} else {
				let hash = tuple.5;
				EXPORTED_XCM.with(|q| q.borrow_mut().push(tuple));
				Ok(hash)
			}
		})
	}
}

thread_local! {
	pub static ASSETS: RefCell<BTreeMap<Location, AssetsInHolding>> = RefCell::new(BTreeMap::new());
}
pub fn assets(who: impl Into<Location>) -> AssetsInHolding {
	ASSETS.with(|a| a.borrow().get(&who.into()).cloned()).unwrap_or_default()
}
pub fn asset_list(who: impl Into<Location>) -> Vec<Asset> {
	Assets::from(assets(who)).into_inner()
}
pub fn add_asset(who: impl Into<Location>, what: impl Into<Asset>) {
	ASSETS.with(|a| {
		a.borrow_mut()
			.entry(who.into())
			.or_insert(AssetsInHolding::new())
			.subsume(what.into())
	});
}
pub fn clear_assets(who: impl Into<Location>) {
	ASSETS.with(|a| a.borrow_mut().remove(&who.into()));
}

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
		_maybe_context: Option<&XcmContext>,
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

pub fn to_account(l: impl Into<Location>) -> Result<u64, Location> {
	let l = l.into();
	Ok(match l.unpack() {
		// Siblings at 2000+id
		(1, [Parachain(id)]) => 2000 + *id as u64,
		// Accounts are their number
		(0, [AccountIndex64 { index, .. }]) => *index,
		// Children at 1000+id
		(0, [Parachain(id)]) => 1000 + *id as u64,
		// Self at 3000
		(0, []) => 3000,
		// Parent at 3001
		(1, []) => 3001,
		_ => {
			// Is it a foreign-consensus?
			let uni = ExecutorUniversalLocation::get();
			if l.parents as usize != uni.len() {
				return Err(l)
			}
			match l.first_interior() {
				Some(GlobalConsensus(Kusama)) => 4000,
				Some(GlobalConsensus(Polkadot)) => 4001,
				_ => return Err(l),
			}
		},
	})
}

pub struct TestOriginConverter;
impl ConvertOrigin<TestOrigin> for TestOriginConverter {
	fn convert_origin(
		origin: impl Into<Location>,
		kind: OriginKind,
	) -> Result<TestOrigin, Location> {
		use OriginKind::*;
		let origin = origin.into();
		match (kind, origin.unpack()) {
			(Superuser, _) => Ok(TestOrigin::Root),
			(SovereignAccount, _) => Ok(TestOrigin::Signed(to_account(origin)?)),
			(Native, (0, [Parachain(id)])) => Ok(TestOrigin::Parachain(*id)),
			(Native, (1, [])) => Ok(TestOrigin::Relay),
			(Native, (0, [AccountIndex64 { index, .. }])) => Ok(TestOrigin::Signed(*index)),
			_ => Err(origin),
		}
	}
}

thread_local! {
	pub static IS_RESERVE: RefCell<BTreeMap<Location, Vec<AssetFilter>>> = RefCell::new(BTreeMap::new());
	pub static IS_TELEPORTER: RefCell<BTreeMap<Location, Vec<AssetFilter>>> = RefCell::new(BTreeMap::new());
	pub static UNIVERSAL_ALIASES: RefCell<BTreeSet<(Location, Junction)>> = RefCell::new(BTreeSet::new());
}
pub fn add_reserve(from: Location, asset: AssetFilter) {
	IS_RESERVE.with(|r| r.borrow_mut().entry(from).or_default().push(asset));
}
#[allow(dead_code)]
pub fn add_teleporter(from: Location, asset: AssetFilter) {
	IS_TELEPORTER.with(|r| r.borrow_mut().entry(from).or_default().push(asset));
}
pub fn add_universal_alias(bridge: impl Into<Location>, consensus: impl Into<Junction>) {
	UNIVERSAL_ALIASES.with(|r| r.borrow_mut().insert((bridge.into(), consensus.into())));
}
pub fn clear_universal_aliases() {
	UNIVERSAL_ALIASES.with(|r| r.replace(Default::default()));
}

pub struct TestIsReserve;
impl ContainsPair<Asset, Location> for TestIsReserve {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		IS_RESERVE
			.with(|r| r.borrow().get(origin).map_or(false, |v| v.iter().any(|a| a.matches(asset))))
	}
}
pub struct TestIsTeleporter;
impl ContainsPair<Asset, Location> for TestIsTeleporter {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		IS_TELEPORTER
			.with(|r| r.borrow().get(origin).map_or(false, |v| v.iter().any(|a| a.matches(asset))))
	}
}

pub struct TestUniversalAliases;
impl Contains<(Location, Junction)> for TestUniversalAliases {
	fn contains(t: &(Location, Junction)) -> bool {
		UNIVERSAL_ALIASES.with(|r| r.borrow().contains(t))
	}
}

pub enum ResponseSlot {
	Expecting(Location),
	Received(Response),
}
thread_local! {
	pub static QUERIES: RefCell<BTreeMap<u64, ResponseSlot>> = RefCell::new(BTreeMap::new());
}
pub struct TestResponseHandler;
impl OnResponse for TestResponseHandler {
	fn expecting_response(origin: &Location, query_id: u64, _querier: Option<&Location>) -> bool {
		QUERIES.with(|q| match q.borrow().get(&query_id) {
			Some(ResponseSlot::Expecting(ref l)) => l == origin,
			_ => false,
		})
	}
	fn on_response(
		_origin: &Location,
		query_id: u64,
		_querier: Option<&Location>,
		response: xcm::latest::Response,
		_max_weight: Weight,
		_context: &XcmContext,
	) -> Weight {
		QUERIES.with(|q| {
			q.borrow_mut().entry(query_id).and_modify(|v| {
				if matches!(*v, ResponseSlot::Expecting(..)) {
					*v = ResponseSlot::Received(response);
				}
			});
		});
		Weight::from_parts(10, 10)
	}
}
pub fn expect_response(query_id: u64, from: Location) {
	QUERIES.with(|q| q.borrow_mut().insert(query_id, ResponseSlot::Expecting(from)));
}
pub fn response(query_id: u64) -> Option<Response> {
	QUERIES.with(|q| {
		q.borrow().get(&query_id).and_then(|v| match v {
			ResponseSlot::Received(r) => Some(r.clone()),
			_ => None,
		})
	})
}

/// Mock implementation of the [`QueryHandler`] trait for creating XCM success queries and expecting
/// responses.
pub struct TestQueryHandler<T, BlockNumber>(core::marker::PhantomData<(T, BlockNumber)>);
impl<T: Config, BlockNumber: sp_runtime::traits::Zero + Encode> QueryHandler
	for TestQueryHandler<T, BlockNumber>
{
	type BlockNumber = BlockNumber;
	type Error = XcmError;
	type UniversalLocation = T::UniversalLocation;

	fn new_query(
		responder: impl Into<Location>,
		_timeout: Self::BlockNumber,
		_match_querier: impl Into<Location>,
	) -> QueryId {
		let query_id = 1;
		expect_response(query_id, responder.into());
		query_id
	}

	fn report_outcome(
		message: &mut Xcm<()>,
		responder: impl Into<Location>,
		timeout: Self::BlockNumber,
	) -> Result<QueryId, Self::Error> {
		let responder = responder.into();
		let destination = Self::UniversalLocation::get()
			.invert_target(&responder)
			.map_err(|()| XcmError::LocationNotInvertible)?;
		let query_id = Self::new_query(responder, timeout, Here);
		let response_info = QueryResponseInfo { destination, query_id, max_weight: Weight::zero() };
		let report_error = Xcm(vec![ReportError(response_info)]);
		message.0.insert(0, SetAppendix(report_error));
		Ok(query_id)
	}

	fn take_response(query_id: QueryId) -> QueryResponseStatus<Self::BlockNumber> {
		QUERIES
			.with(|q| {
				q.borrow().get(&query_id).and_then(|v| match v {
					ResponseSlot::Received(r) => Some(QueryResponseStatus::Ready {
						response: r.clone(),
						at: Self::BlockNumber::zero(),
					}),
					_ => Some(QueryResponseStatus::NotFound),
				})
			})
			.unwrap_or(QueryResponseStatus::NotFound)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn expect_response(_id: QueryId, _response: xcm::latest::Response) {
		// Unnecessary since it's only a test implementation
	}
}

parameter_types! {
	pub static ExecutorUniversalLocation: InteriorLocation
		= (ByGenesis([0; 32]), Parachain(42)).into();
	pub UnitWeightCost: Weight = Weight::from_parts(10, 10);
}
parameter_types! {
	// Nothing is allowed to be paid/unpaid by default.
	pub static AllowExplicitUnpaidFrom: Vec<Location> = vec![];
	pub static AllowUnpaidFrom: Vec<Location> = vec![];
	pub static AllowPaidFrom: Vec<Location> = vec![];
	pub static AllowSubsFrom: Vec<Location> = vec![];
	// 1_000_000_000_000 => 1 unit of asset for 1 unit of ref time weight.
	// 1024 * 1024 => 1 unit of asset for 1 unit of proof size weight.
	pub static WeightPrice: (AssetId, u128, u128) =
		(From::from(Here), 1_000_000_000_000, 1024 * 1024);
	pub static MaxInstructions: u32 = 100;
}

pub struct TestSuspender;
impl CheckSuspension for TestSuspender {
	fn is_suspended<Call>(
		_origin: &Location,
		_instructions: &mut [Instruction<Call>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> bool {
		SUSPENDED.with(|s| s.get())
	}
}

impl TestSuspender {
	pub fn set_suspended(suspended: bool) {
		SUSPENDED.with(|s| s.set(suspended));
	}
}

pub type TestBarrier = (
	TakeWeightCredit,
	AllowKnownQueryResponses<TestResponseHandler>,
	AllowTopLevelPaidExecutionFrom<IsInVec<AllowPaidFrom>>,
	AllowExplicitUnpaidExecutionFrom<IsInVec<AllowExplicitUnpaidFrom>>,
	AllowUnpaidExecutionFrom<IsInVec<AllowUnpaidFrom>>,
	AllowSubscriptionsFrom<IsInVec<AllowSubsFrom>>,
);

thread_local! {
	pub static IS_WAIVED: RefCell<Vec<FeeReason>> = RefCell::new(vec![]);
}
#[allow(dead_code)]
pub fn set_fee_waiver(waived: Vec<FeeReason>) {
	IS_WAIVED.with(|l| l.replace(waived));
}

pub struct TestFeeManager;
impl FeeManager for TestFeeManager {
	fn is_waived(_: Option<&Location>, r: FeeReason) -> bool {
		IS_WAIVED.with(|l| l.borrow().contains(&r))
	}

	fn handle_fee(_: Assets, _: Option<&XcmContext>, _: FeeReason) {}
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum LockTraceItem {
	Lock { unlocker: Location, asset: Asset, owner: Location },
	Unlock { unlocker: Location, asset: Asset, owner: Location },
	Note { locker: Location, asset: Asset, owner: Location },
	Reduce { locker: Location, asset: Asset, owner: Location },
}
thread_local! {
	pub static NEXT_INDEX: RefCell<u32> = RefCell::new(0);
	pub static LOCK_TRACE: RefCell<Vec<LockTraceItem>> = RefCell::new(Vec::new());
	pub static ALLOWED_UNLOCKS: RefCell<BTreeMap<(Location, Location), AssetsInHolding>> = RefCell::new(BTreeMap::new());
	pub static ALLOWED_REQUEST_UNLOCKS: RefCell<BTreeMap<(Location, Location), AssetsInHolding>> = RefCell::new(BTreeMap::new());
}

pub fn take_lock_trace() -> Vec<LockTraceItem> {
	LOCK_TRACE.with(|l| l.replace(Vec::new()))
}
pub fn allow_unlock(
	unlocker: impl Into<Location>,
	asset: impl Into<Asset>,
	owner: impl Into<Location>,
) {
	ALLOWED_UNLOCKS.with(|l| {
		l.borrow_mut()
			.entry((owner.into(), unlocker.into()))
			.or_default()
			.subsume(asset.into())
	});
}
pub fn disallow_unlock(
	unlocker: impl Into<Location>,
	asset: impl Into<Asset>,
	owner: impl Into<Location>,
) {
	ALLOWED_UNLOCKS.with(|l| {
		l.borrow_mut()
			.entry((owner.into(), unlocker.into()))
			.or_default()
			.saturating_take(asset.into().into())
	});
}
pub fn unlock_allowed(unlocker: &Location, asset: &Asset, owner: &Location) -> bool {
	ALLOWED_UNLOCKS.with(|l| {
		l.borrow_mut()
			.get(&(owner.clone(), unlocker.clone()))
			.map_or(false, |x| x.contains_asset(asset))
	})
}
pub fn allow_request_unlock(
	locker: impl Into<Location>,
	asset: impl Into<Asset>,
	owner: impl Into<Location>,
) {
	ALLOWED_REQUEST_UNLOCKS.with(|l| {
		l.borrow_mut()
			.entry((owner.into(), locker.into()))
			.or_default()
			.subsume(asset.into())
	});
}
pub fn disallow_request_unlock(
	locker: impl Into<Location>,
	asset: impl Into<Asset>,
	owner: impl Into<Location>,
) {
	ALLOWED_REQUEST_UNLOCKS.with(|l| {
		l.borrow_mut()
			.entry((owner.into(), locker.into()))
			.or_default()
			.saturating_take(asset.into().into())
	});
}
pub fn request_unlock_allowed(locker: &Location, asset: &Asset, owner: &Location) -> bool {
	ALLOWED_REQUEST_UNLOCKS.with(|l| {
		l.borrow_mut()
			.get(&(owner.clone(), locker.clone()))
			.map_or(false, |x| x.contains_asset(asset))
	})
}

pub struct TestTicket(LockTraceItem);
impl Enact for TestTicket {
	fn enact(self) -> Result<(), LockError> {
		match &self.0 {
			LockTraceItem::Lock { unlocker, asset, owner } =>
				allow_unlock(unlocker.clone(), asset.clone(), owner.clone()),
			LockTraceItem::Unlock { unlocker, asset, owner } =>
				disallow_unlock(unlocker.clone(), asset.clone(), owner.clone()),
			LockTraceItem::Reduce { locker, asset, owner } =>
				disallow_request_unlock(locker.clone(), asset.clone(), owner.clone()),
			_ => {},
		}
		LOCK_TRACE.with(move |l| l.borrow_mut().push(self.0));
		Ok(())
	}
}

pub struct TestAssetLock;
impl AssetLock for TestAssetLock {
	type LockTicket = TestTicket;
	type UnlockTicket = TestTicket;
	type ReduceTicket = TestTicket;

	fn prepare_lock(
		unlocker: Location,
		asset: Asset,
		owner: Location,
	) -> Result<Self::LockTicket, LockError> {
		ensure!(assets(owner.clone()).contains_asset(&asset), LockError::AssetNotOwned);
		Ok(TestTicket(LockTraceItem::Lock { unlocker, asset, owner }))
	}

	fn prepare_unlock(
		unlocker: Location,
		asset: Asset,
		owner: Location,
	) -> Result<Self::UnlockTicket, LockError> {
		ensure!(unlock_allowed(&unlocker, &asset, &owner), LockError::NotLocked);
		Ok(TestTicket(LockTraceItem::Unlock { unlocker, asset, owner }))
	}

	fn note_unlockable(locker: Location, asset: Asset, owner: Location) -> Result<(), LockError> {
		allow_request_unlock(locker.clone(), asset.clone(), owner.clone());
		let item = LockTraceItem::Note { locker, asset, owner };
		LOCK_TRACE.with(move |l| l.borrow_mut().push(item));
		Ok(())
	}

	fn prepare_reduce_unlockable(
		locker: Location,
		asset: Asset,
		owner: Location,
	) -> Result<Self::ReduceTicket, xcm_executor::traits::LockError> {
		ensure!(request_unlock_allowed(&locker, &asset, &owner), LockError::NotLocked);
		Ok(TestTicket(LockTraceItem::Reduce { locker, asset, owner }))
	}
}

thread_local! {
	pub static EXCHANGE_ASSETS: RefCell<AssetsInHolding> = RefCell::new(AssetsInHolding::new());
}
pub fn set_exchange_assets(assets: impl Into<Assets>) {
	EXCHANGE_ASSETS.with(|a| a.replace(assets.into().into()));
}
pub fn exchange_assets() -> Assets {
	EXCHANGE_ASSETS.with(|a| a.borrow().clone().into())
}
pub struct TestAssetExchange;
impl AssetExchange for TestAssetExchange {
	fn exchange_asset(
		_origin: Option<&Location>,
		give: AssetsInHolding,
		want: &Assets,
		maximal: bool,
	) -> Result<AssetsInHolding, AssetsInHolding> {
		let mut have = EXCHANGE_ASSETS.with(|l| l.borrow().clone());
		ensure!(have.contains_assets(want), give);
		let get = if maximal {
			std::mem::replace(&mut have, AssetsInHolding::new())
		} else {
			have.saturating_take(want.clone().into())
		};
		have.subsume_assets(give);
		EXCHANGE_ASSETS.with(|l| l.replace(have));
		Ok(get)
	}

	fn quote_exchange_price(give: &Assets, want: &Assets, maximal: bool) -> Option<Assets> {
		let mut have = EXCHANGE_ASSETS.with(|l| l.borrow().clone());
		if !have.contains_assets(want) {
			return None;
		}
		let get = if maximal {
			have.saturating_take(give.clone().into())
		} else {
			have.saturating_take(want.clone().into())
		};
		let result: Vec<Asset> = get.fungible_assets_iter().collect();
		Some(result.into())
	}
}

pub struct SiblingPrefix;
impl Contains<Location> for SiblingPrefix {
	fn contains(loc: &Location) -> bool {
		matches!(loc.unpack(), (1, [Parachain(_)]))
	}
}

pub struct ChildPrefix;
impl Contains<Location> for ChildPrefix {
	fn contains(loc: &Location) -> bool {
		matches!(loc.unpack(), (0, [Parachain(_)]))
	}
}

pub struct ParentPrefix;
impl Contains<Location> for ParentPrefix {
	fn contains(loc: &Location) -> bool {
		matches!(loc.unpack(), (1, []))
	}
}

pub struct TestConfig;
impl Config for TestConfig {
	type RuntimeCall = TestCall;
	type XcmSender = TestMessageSender;
	type AssetTransactor = TestAssetTransactor;
	type OriginConverter = TestOriginConverter;
	type IsReserve = TestIsReserve;
	type IsTeleporter = TestIsTeleporter;
	type UniversalLocation = ExecutorUniversalLocation;
	type Barrier = TrailingSetTopicAsId<RespectSuspension<TestBarrier, TestSuspender>>;
	type Weigher = FixedWeightBounds<UnitWeightCost, TestCall, MaxInstructions>;
	type Trader = FixedRateOfFungible<WeightPrice, ()>;
	type ResponseHandler = TestResponseHandler;
	type AssetTrap = TestAssetTrap;
	type AssetLocker = TestAssetLock;
	type AssetExchanger = TestAssetExchange;
	type AssetClaims = TestAssetTrap;
	type SubscriptionService = TestSubscriptionService;
	type PalletInstancesInfo = TestPalletsInfo;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = TestFeeManager;
	type UniversalAliases = TestUniversalAliases;
	type MessageExporter = TestMessageExporter;
	type CallDispatcher = TestCall;
	type SafeCallFilter = Everything;
	type Aliasers = AliasForeignAccountId32<SiblingPrefix>;
	type TransactionalProcessor = ();
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

pub fn fungible_multi_asset(location: Location, amount: u128) -> Asset {
	(AssetId::from(location), Fungibility::Fungible(amount)).into()
}

pub fn fake_message_hash<T>(message: &Xcm<T>) -> XcmHash {
	message.using_encoded(sp_io::hashing::blake2_256)
}
