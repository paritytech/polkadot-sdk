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

pub mod mock {
	use crate::*;
	use core::cell::RefCell;
	use frame_support::{
		ensure,
		traits::{
			fungibles::{Balanced, DecreaseIssuance, Dust, IncreaseIssuance, Inspect, Unbalanced},
			tokens::{
				DepositConsequence, Fortitude, Preservation, Provenance, WithdrawConsequence,
			},
		},
	};
	use sp_runtime::DispatchError;
	use std::collections::HashMap;
	use xcm::latest::Junction;

	pub type AccountId = u64;
	pub type AssetId = u32;
	pub type Balance = u128;
	pub type Credit = fungibles::Credit<AccountId, Fungibles>;
	pub type Debt = fungibles::Debt<AccountId, Fungibles>;

	thread_local! {
	   pub static TOTAL_ISSUANCE: RefCell<HashMap<AssetId, Balance>> = RefCell::new(HashMap::new());
	   pub static SWAP: RefCell<HashMap<(AssetId, AssetId), (Debt, Credit)>> = RefCell::new(HashMap::new());
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
			_amount_out_min: Option<Self::Balance>,
		) -> Result<Self::Credit, (Self::Credit, DispatchError)> {
			ensure!(2 == path.len(), (credit_in, DispatchError::Unavailable));
			let swap_res = SWAP.with(|b| b.borrow_mut().remove(&(path[0], path[1])));
			let (debt, credit_out) = match swap_res {
				Some((d, c)) => (d, c),
				None => return Err((credit_in, DispatchError::Unavailable)),
			};
			drop(debt);
			drop(credit_in);
			Ok(credit_out)
		}
		fn swap_tokens_for_exact_tokens(
			path: Vec<Self::AssetKind>,
			credit_in: Self::Credit,
			_amount_out: Self::Balance,
		) -> Result<(Self::Credit, Self::Credit), (Self::Credit, DispatchError)> {
			ensure!(2 == path.len(), (credit_in, DispatchError::Unavailable));
			let swap_res = SWAP.with(|b| b.borrow_mut().remove(&(path[0], path[1])));
			let (debt, credit_out) = match swap_res {
				Some((d, c)) => (d, c),
				None => return Err((credit_in, DispatchError::Unavailable)),
			};
			let (credit_debt, change) = credit_in.split(debt.peek());
			drop(debt);
			drop(credit_debt);
			Ok((credit_out, change))
		}
	}

	pub fn prepare_swap(
		asset_in: AssetId,
		max_amount_in: Balance,
		asset_out: AssetId,
		amount_out: Balance,
	) {
		let debt = Fungibles::rescind(asset_in, max_amount_in);
		let credit_out = Fungibles::issue(asset_out, amount_out);
		SWAP.with(|b| b.borrow_mut().insert((asset_in, asset_out), (debt, credit_out)));
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
			unimplemented!()
		}
		fn total_balance(_: Self::AssetId, _: &AccountId) -> Self::Balance {
			unimplemented!()
		}
		fn balance(_: Self::AssetId, _: &AccountId) -> Self::Balance {
			unimplemented!()
		}
		fn reducible_balance(
			_: Self::AssetId,
			_: &AccountId,
			_: Preservation,
			_: Fortitude,
		) -> Self::Balance {
			unimplemented!()
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
			_: Self::AssetId,
			_: &AccountId,
			_: Self::Balance,
		) -> Result<Option<Self::Balance>, DispatchError> {
			unimplemented!()
		}
	}

	impl Balanced<AccountId> for Fungibles {
		type OnDropCredit = DecreaseIssuance<AccountId, Self>;
		type OnDropDebt = IncreaseIssuance<AccountId, Self>;
	}

	pub struct FungiblesMatcher;
	impl MatchesFungibles<AssetId, Balance> for FungiblesMatcher {
		fn matches_fungibles(
			a: &MultiAsset,
		) -> core::result::Result<(AssetId, Balance), xcm_executor::traits::Error> {
			match a {
				MultiAsset {
					fun: Fungible(amount),
					id:
						Concrete(MultiLocation { parents: 0, interior: X1(Junction::GeneralIndex(id)) }),
				} => Ok(((*id).try_into().unwrap(), *amount)),
				_ => Err(xcm_executor::traits::Error::AssetNotHandled),
			}
		}
	}
}

use crate::*;
use frame_support::{
	parameter_types,
	traits::fungibles::{Inspect, Unbalanced},
};
use mock::{AccountId, AssetId, Balance, Fungibles};
use xcm_executor::Assets as HoldingAsset;

fn create_exchangeable_holding_asset(
	amount: Balance,
	exchange_amount: Balance,
	target_amount: Balance,
) -> (HoldingAsset, HoldingAsset) {
	assert!(amount >= exchange_amount);
	mock::prepare_swap(CLIENT_ASSET, exchange_amount, TARGET_ASSET, target_amount);
	(
		create_holding_asset(CLIENT_ASSET, amount),
		create_holding_asset(CLIENT_ASSET, amount - exchange_amount),
	)
}

fn create_holding_asset(asset_id: AssetId, amount: Balance) -> HoldingAsset {
	MultiAsset {
		id: Concrete(MultiLocation::new(0, X1(GeneralIndex(asset_id.into())))),
		fun: Fungible(amount),
	}
	.into()
}

fn xcm_context() -> XcmContext {
	XcmContext { origin: None, message_id: [0u8; 32], topic: None }
}

// const UNKNOWN_ASSET: AssetId = 0;
const TARGET_ASSET: AssetId = 1;
const CLIENT_ASSET: AssetId = 2;

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
	Fungibles::set_total_issuance(TARGET_ASSET, 10000);
	Fungibles::set_total_issuance(CLIENT_ASSET, 10000);

	let client_asset_total = 15;
	let client_asset_fee = 5;
	let target_asset_fee = 30;

	let (holding_asset, holding_change) =
		create_exchangeable_holding_asset(client_asset_total, client_asset_fee, target_asset_fee);

	let target_total = Fungibles::total_issuance(TARGET_ASSET);
	let client_total = Fungibles::total_issuance(CLIENT_ASSET);

	let mut trader = Trader::new();
	assert_eq!(
		trader.buy_weight(Weight::from_all(10), holding_asset, &xcm_context()).unwrap(),
		holding_change
	);

	assert_eq!(Fungibles::total_issuance(TARGET_ASSET), target_total);
	assert_eq!(Fungibles::total_issuance(CLIENT_ASSET), client_total + client_asset_fee);
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
			.buy_weight(Weight::from_all(10), HoldingAsset::new(), &xcm_context())
			.unwrap_err(),
		XcmError::AssetNotFound
	);
}

#[test]
fn fails_to_match_holding_asset() {
	let mut trader = Trader::new();
	let holding_asset =
		MultiAsset { id: Concrete(MultiLocation::new(1, X1(Parachain(1)))), fun: Fungible(10) };
	assert_eq!(
		trader
			.buy_weight(Weight::from_all(10), holding_asset.into(), &xcm_context())
			.unwrap_err(),
		XcmError::FeesNotMet
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
