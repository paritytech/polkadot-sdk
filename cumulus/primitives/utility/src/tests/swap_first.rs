// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;
use frame_support::{parameter_types, traits::fungibles::Inspect};
use mock::{setup_pool, AccountId, AssetId, Balance, Fungibles};
use xcm::latest::AssetId as XcmAssetId;
use xcm_executor::AssetsInHolding;

fn create_holding_asset(asset_id: AssetId, amount: Balance) -> AssetsInHolding {
	create_asset(asset_id, amount).into()
}

fn create_asset(asset_id: AssetId, amount: Balance) -> Asset {
	Asset { id: create_asset_id(asset_id), fun: Fungible(amount) }
}

fn create_asset_id(asset_id: AssetId) -> XcmAssetId {
	AssetId(Location::new(0, [GeneralIndex(asset_id.into())]))
}

fn xcm_context() -> XcmContext {
	XcmContext { origin: None, message_id: [0u8; 32], topic: None }
}

fn weight_worth_of(fee: Balance) -> Weight {
	Weight::from_parts(fee.try_into().unwrap(), 0)
}

const TARGET_ASSET: AssetId = 1;
const CLIENT_ASSET: AssetId = 2;
const CLIENT_ASSET_2: AssetId = 3;

parameter_types! {
	pub const TargetAsset: AssetId = TARGET_ASSET;
}

pub type Trader = SwapFirstAssetTrader<
	TargetAsset,
	mock::Swap,
	mock::WeightToFee,
	mock::Fungibles,
	mock::FungiblesMatcher,
	(),
	AccountId,
>;

#[test]
fn holding_asset_swap_for_target() {
	let client_asset_total = 15;
	let fee = 5;

	setup_pool(CLIENT_ASSET, 1000, TARGET_ASSET, 1000);

	let holding_asset = create_holding_asset(CLIENT_ASSET, client_asset_total);
	let holding_change = create_holding_asset(CLIENT_ASSET, client_asset_total - fee);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader.buy_weight(weight_worth_of(fee), holding_asset, &xcm_context()).unwrap(),
		holding_change
	);

	assert_eq!(trader.total_fee.peek(), fee);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + fee);
}

#[test]
fn holding_asset_swap_for_target_twice() {
	let client_asset_total = 20;
	let fee1 = 5;
	let fee2 = 6;

	setup_pool(CLIENT_ASSET, 1000, TARGET_ASSET, 1000);

	let holding_asset = create_holding_asset(CLIENT_ASSET, client_asset_total);
	let holding_change1 = create_holding_asset(CLIENT_ASSET, client_asset_total - fee1);
	let holding_change2 = create_holding_asset(CLIENT_ASSET, client_asset_total - fee1 - fee2);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader.buy_weight(weight_worth_of(fee1), holding_asset, &xcm_context()).unwrap(),
		holding_change1
	);
	assert_eq!(
		trader
			.buy_weight(weight_worth_of(fee2), holding_change1, &xcm_context())
			.unwrap(),
		holding_change2
	);

	assert_eq!(trader.total_fee.peek(), fee1 + fee2);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + fee1 + fee2);
}

#[test]
fn buy_and_refund_twice_for_target() {
	let client_asset_total = 15;
	let fee = 5;
	let refund1 = 4;
	let refund2 = 2;

	setup_pool(CLIENT_ASSET, 1000, TARGET_ASSET, 1000);
	// create pool for refund swap.
	setup_pool(TARGET_ASSET, 1000, CLIENT_ASSET, 1000);

	let holding_asset = create_holding_asset(CLIENT_ASSET, client_asset_total);
	let holding_change = create_holding_asset(CLIENT_ASSET, client_asset_total - fee);
	let refund_asset = create_asset(CLIENT_ASSET, refund1);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader.buy_weight(weight_worth_of(fee), holding_asset, &xcm_context()).unwrap(),
		holding_change
	);

	assert_eq!(trader.total_fee.peek(), fee);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(trader.refund_weight(weight_worth_of(refund1), &xcm_context()), Some(refund_asset));

	assert_eq!(trader.total_fee.peek(), fee - refund1);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(trader.refund_weight(weight_worth_of(refund2), &xcm_context()), None);

	assert_eq!(trader.total_fee.peek(), fee - refund1);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + fee - refund1);
}

#[test]
fn buy_with_various_assets_and_refund_for_target() {
	let client_asset_total = 10;
	let client_asset_2_total = 15;
	let fee1 = 5;
	let fee2 = 6;
	let refund1 = 6;
	let refund2 = 4;

	setup_pool(CLIENT_ASSET, 1000, TARGET_ASSET, 1000);
	setup_pool(CLIENT_ASSET_2, 1000, TARGET_ASSET, 1000);
	// create pool for refund swap.
	setup_pool(TARGET_ASSET, 1000, CLIENT_ASSET_2, 1000);

	let holding_asset = create_holding_asset(CLIENT_ASSET, client_asset_total);
	let holding_asset_2 = create_holding_asset(CLIENT_ASSET_2, client_asset_2_total);
	let holding_change = create_holding_asset(CLIENT_ASSET, client_asset_total - fee1);
	let holding_change_2 = create_holding_asset(CLIENT_ASSET_2, client_asset_2_total - fee2);
	// both refunds in the latest buy asset (`CLIENT_ASSET_2`).
	let refund_asset = create_asset(CLIENT_ASSET_2, refund1);
	let refund_asset_2 = create_asset(CLIENT_ASSET_2, refund2);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);
	let client_total_2 = Fungibles::total_issuance(CLIENT_ASSET_2);

	let mut trader = Trader::new();
	// first purchase with `CLIENT_ASSET`.
	assert_eq!(
		trader.buy_weight(weight_worth_of(fee1), holding_asset, &xcm_context()).unwrap(),
		holding_change
	);

	assert_eq!(trader.total_fee.peek(), fee1);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	// second purchase with `CLIENT_ASSET_2`.
	assert_eq!(
		trader
			.buy_weight(weight_worth_of(fee2), holding_asset_2, &xcm_context())
			.unwrap(),
		holding_change_2
	);

	assert_eq!(trader.total_fee.peek(), fee1 + fee2);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET_2)));

	// first refund in the last asset used with `buy_weight`.
	assert_eq!(trader.refund_weight(weight_worth_of(refund1), &xcm_context()), Some(refund_asset));

	assert_eq!(trader.total_fee.peek(), fee1 + fee2 - refund1);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET_2)));

	// second refund in the last asset used with `buy_weight`.
	assert_eq!(
		trader.refund_weight(weight_worth_of(refund2), &xcm_context()),
		Some(refund_asset_2)
	);

	assert_eq!(trader.total_fee.peek(), fee1 + fee2 - refund1 - refund2);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET_2)));

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + fee1);
	assert_eq!(
		Fungibles::total_issuance(CLIENT_ASSET_2),
		client_total_2 + fee2 - refund1 - refund2
	);
}

#[test]
fn not_enough_to_refund() {
	let client_asset_total = 15;
	let fee = 5;
	let refund = 6;

	setup_pool(CLIENT_ASSET, 1000, TARGET_ASSET, 1000);

	let holding_asset = create_holding_asset(CLIENT_ASSET, client_asset_total);
	let holding_change = create_holding_asset(CLIENT_ASSET, client_asset_total - fee);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader.buy_weight(weight_worth_of(fee), holding_asset, &xcm_context()).unwrap(),
		holding_change
	);

	assert_eq!(trader.total_fee.peek(), fee);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(trader.refund_weight(weight_worth_of(refund), &xcm_context()), None);

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + fee);
}

#[test]
fn not_exchangeable_to_refund() {
	let client_asset_total = 15;
	let fee = 5;
	let refund = 1;

	setup_pool(CLIENT_ASSET, 1000, TARGET_ASSET, 1000);

	let holding_asset = create_holding_asset(CLIENT_ASSET, client_asset_total);
	let holding_change = create_holding_asset(CLIENT_ASSET, client_asset_total - fee);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader.buy_weight(weight_worth_of(fee), holding_asset, &xcm_context()).unwrap(),
		holding_change
	);

	assert_eq!(trader.total_fee.peek(), fee);
	assert_eq!(trader.last_fee_asset, Some(create_asset_id(CLIENT_ASSET)));

	assert_eq!(trader.refund_weight(weight_worth_of(refund), &xcm_context()), None);

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + fee);
}

#[test]
fn nothing_to_refund() {
	let fee = 5;

	let mut trader = Trader::new();
	assert_eq!(trader.refund_weight(weight_worth_of(fee), &xcm_context()), None);
}

#[test]
fn holding_asset_not_exchangeable_for_target() {
	let holding_asset = create_holding_asset(CLIENT_ASSET, 10);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader
			.buy_weight(Weight::from_all(10), holding_asset, &xcm_context())
			.unwrap_err(),
		XcmError::FeesNotMet
	);

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total);
}

#[test]
fn empty_holding_asset() {
	let mut trader = Trader::new();
	assert_eq!(
		trader
			.buy_weight(Weight::from_all(10), AssetsInHolding::new(), &xcm_context())
			.unwrap_err(),
		XcmError::AssetNotFound
	);
}

#[test]
fn fails_to_match_holding_asset() {
	let mut trader = Trader::new();
	let holding_asset = Asset { id: AssetId(Location::new(1, [Parachain(1)])), fun: Fungible(10) };
	assert_eq!(
		trader
			.buy_weight(Weight::from_all(10), holding_asset.into(), &xcm_context())
			.unwrap_err(),
		XcmError::AssetNotFound
	);
}

#[test]
fn holding_asset_equal_to_target_asset() {
	let mut trader = Trader::new();
	let holding_asset = create_holding_asset(TargetAsset::get(), 10);
	assert_eq!(
		trader
			.buy_weight(Weight::from_all(10), holding_asset, &xcm_context())
			.unwrap_err(),
		XcmError::FeesNotMet
	);
}

pub mod mock {
	use crate::*;
	use core::cell::RefCell;
	use frame_support::{
		ensure,
		traits::{
			fungibles::{Balanced, DecreaseIssuance, Dust, IncreaseIssuance, Inspect, Unbalanced},
			tokens::{
				DepositConsequence, Fortitude, Fortitude::Polite, Precision::Exact, Preservation,
				Preservation::Preserve, Provenance, WithdrawConsequence,
			},
		},
	};
	use sp_runtime::{traits::One, DispatchError};
	use std::collections::HashMap;
	use xcm::latest::Junction;

	pub type AccountId = u64;
	pub type AssetId = u32;
	pub type Balance = u128;
	pub type Credit = fungibles::Credit<AccountId, Fungibles>;

	thread_local! {
	   pub static TOTAL_ISSUANCE: RefCell<HashMap<AssetId, Balance>> = RefCell::new(HashMap::new());
	   pub static ACCOUNT: RefCell<HashMap<(AssetId, AccountId), Balance>> = RefCell::new(HashMap::new());
	   pub static SWAP: RefCell<HashMap<(AssetId, AssetId), AccountId>> = RefCell::new(HashMap::new());
	}

	pub struct Swap {}
	impl SwapCreditT<AccountId> for Swap {
		type Balance = Balance;
		type AssetKind = AssetId;
		type Credit = Credit;
		fn max_path_len() -> u32 {
			2
		}
		fn swap_exact_tokens_for_tokens(
			path: Vec<Self::AssetKind>,
			credit_in: Self::Credit,
			amount_out_min: Option<Self::Balance>,
		) -> Result<Self::Credit, (Self::Credit, DispatchError)> {
			ensure!(2 == path.len(), (credit_in, DispatchError::Unavailable));
			ensure!(
				credit_in.peek() >= amount_out_min.unwrap_or(Self::Balance::zero()),
				(credit_in, DispatchError::Unavailable)
			);
			let swap_res = SWAP.with(|b| b.borrow().get(&(path[0], path[1])).map(|v| *v));
			let pool_account = match swap_res {
				Some(a) => a,
				None => return Err((credit_in, DispatchError::Unavailable)),
			};
			let credit_out = match Fungibles::withdraw(
				path[1],
				&pool_account,
				credit_in.peek(),
				Exact,
				Preserve,
				Polite,
			) {
				Ok(c) => c,
				Err(_) => return Err((credit_in, DispatchError::Unavailable)),
			};
			let _ = Fungibles::resolve(&pool_account, credit_in)
				.map_err(|c| (c, DispatchError::Unavailable))?;
			Ok(credit_out)
		}
		fn swap_tokens_for_exact_tokens(
			path: Vec<Self::AssetKind>,
			credit_in: Self::Credit,
			amount_out: Self::Balance,
		) -> Result<(Self::Credit, Self::Credit), (Self::Credit, DispatchError)> {
			ensure!(2 == path.len(), (credit_in, DispatchError::Unavailable));
			ensure!(credit_in.peek() >= amount_out, (credit_in, DispatchError::Unavailable));
			let swap_res = SWAP.with(|b| b.borrow().get(&(path[0], path[1])).map(|v| *v));
			let pool_account = match swap_res {
				Some(a) => a,
				None => return Err((credit_in, DispatchError::Unavailable)),
			};
			let credit_out = match Fungibles::withdraw(
				path[1],
				&pool_account,
				amount_out,
				Exact,
				Preserve,
				Polite,
			) {
				Ok(c) => c,
				Err(_) => return Err((credit_in, DispatchError::Unavailable)),
			};
			let (credit_in, change) = credit_in.split(amount_out);
			let _ = Fungibles::resolve(&pool_account, credit_in)
				.map_err(|c| (c, DispatchError::Unavailable))?;
			Ok((credit_out, change))
		}
	}

	pub fn pool_account(asset1: AssetId, asset2: AssetId) -> AccountId {
		(1000 + asset1 * 10 + asset2 * 100).into()
	}

	pub fn setup_pool(asset1: AssetId, liquidity1: Balance, asset2: AssetId, liquidity2: Balance) {
		let account = pool_account(asset1, asset2);
		SWAP.with(|b| b.borrow_mut().insert((asset1, asset2), account));
		let debt1 = Fungibles::deposit(asset1, &account, liquidity1, Exact);
		let debt2 = Fungibles::deposit(asset2, &account, liquidity2, Exact);
		drop(debt1);
		drop(debt2);
	}

	pub struct WeightToFee;
	impl WeightToFeeT for WeightToFee {
		type Balance = Balance;
		fn weight_to_fee(weight: &Weight) -> Self::Balance {
			(weight.ref_time() + weight.proof_size()).into()
		}
	}

	pub struct Fungibles {}
	impl Inspect<AccountId> for Fungibles {
		type AssetId = AssetId;
		type Balance = Balance;
		fn total_issuance(asset: Self::AssetId) -> Self::Balance {
			TOTAL_ISSUANCE.with(|b| b.borrow().get(&asset).map_or(Self::Balance::zero(), |b| *b))
		}
		fn minimum_balance(_: Self::AssetId) -> Self::Balance {
			Self::Balance::one()
		}
		fn total_balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
			ACCOUNT.with(|b| b.borrow().get(&(asset, *who)).map_or(Self::Balance::zero(), |b| *b))
		}
		fn balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
			ACCOUNT.with(|b| b.borrow().get(&(asset, *who)).map_or(Self::Balance::zero(), |b| *b))
		}
		fn reducible_balance(
			asset: Self::AssetId,
			who: &AccountId,
			_: Preservation,
			_: Fortitude,
		) -> Self::Balance {
			ACCOUNT.with(|b| b.borrow().get(&(asset, *who)).map_or(Self::Balance::zero(), |b| *b))
		}
		fn can_deposit(
			_: Self::AssetId,
			_: &AccountId,
			_: Self::Balance,
			_: Provenance,
		) -> DepositConsequence {
			unimplemented!()
		}
		fn can_withdraw(
			_: Self::AssetId,
			_: &AccountId,
			_: Self::Balance,
		) -> WithdrawConsequence<Self::Balance> {
			unimplemented!()
		}
		fn asset_exists(_: Self::AssetId) -> bool {
			unimplemented!()
		}
	}

	impl Unbalanced<AccountId> for Fungibles {
		fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) {
			TOTAL_ISSUANCE.with(|b| b.borrow_mut().insert(asset, amount));
		}
		fn handle_dust(_: Dust<AccountId, Self>) {
			unimplemented!()
		}
		fn write_balance(
			asset: Self::AssetId,
			who: &AccountId,
			amount: Self::Balance,
		) -> Result<Option<Self::Balance>, DispatchError> {
			let _ = ACCOUNT.with(|b| b.borrow_mut().insert((asset, *who), amount));
			Ok(None)
		}
	}

	impl Balanced<AccountId> for Fungibles {
		type OnDropCredit = DecreaseIssuance<AccountId, Self>;
		type OnDropDebt = IncreaseIssuance<AccountId, Self>;
	}

	pub struct FungiblesMatcher;
	impl MatchesFungibles<AssetId, Balance> for FungiblesMatcher {
		fn matches_fungibles(
			a: &Asset,
		) -> core::result::Result<(AssetId, Balance), xcm_executor::traits::Error> {
			match a {
				Asset { fun: Fungible(amount), id: AssetId(inner_location) } =>
					match inner_location.unpack() {
						(0, [Junction::GeneralIndex(id)]) =>
							Ok(((*id).try_into().unwrap(), *amount)),
						_ => Err(xcm_executor::traits::Error::AssetNotHandled),
					},
				_ => Err(xcm_executor::traits::Error::AssetNotHandled),
			}
		}
	}
}
