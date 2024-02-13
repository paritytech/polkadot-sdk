// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Auxiliary struct/enums for parachain runtimes.
//! Taken from polkadot/runtime/common (at a21cd64) and adapted for parachains.

use frame_support::traits::{
	fungibles::{self, Balanced, Credit},
	Contains, ContainsPair, Currency, Get, Imbalance, OnUnbalanced, OriginTrait,
};
use pallet_asset_tx_payment::HandleCredit;
use sp_runtime::traits::Zero;
use sp_std::{marker::PhantomData, prelude::*};
use xcm::latest::{
	Asset, AssetId, Fungibility, Fungibility::Fungible, Junction, Junctions::Here, Location,
	Parent, WeightLimit,
};
use xcm_executor::traits::ConvertLocation;

/// Type alias to conveniently refer to the `Currency::NegativeImbalance` associated type.
pub type NegativeImbalance<T> = <pallet_balances::Pallet<T> as Currency<
	<T as frame_system::Config>::AccountId,
>>::NegativeImbalance;

/// Type alias to conveniently refer to `frame_system`'s `Config::AccountId`.
pub type AccountIdOf<R> = <R as frame_system::Config>::AccountId;

/// Implementation of `OnUnbalanced` that deposits the fees into a staking pot for later payout.
pub struct ToStakingPot<R>(PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for ToStakingPot<R>
where
	R: pallet_balances::Config + pallet_collator_selection::Config,
	AccountIdOf<R>: From<polkadot_primitives::AccountId> + Into<polkadot_primitives::AccountId>,
	<R as frame_system::Config>::RuntimeEvent: From<pallet_balances::Event<R>>,
{
	fn on_nonzero_unbalanced(amount: NegativeImbalance<R>) {
		let staking_pot = <pallet_collator_selection::Pallet<R>>::account_id();
		<pallet_balances::Pallet<R>>::resolve_creating(&staking_pot, amount);
	}
}

/// Implementation of `OnUnbalanced` that deals with the fees by combining tip and fee and passing
/// the result on to `ToStakingPot`.
pub struct DealWithFees<R>(PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
	R: pallet_balances::Config + pallet_collator_selection::Config,
	AccountIdOf<R>: From<polkadot_primitives::AccountId> + Into<polkadot_primitives::AccountId>,
	<R as frame_system::Config>::RuntimeEvent: From<pallet_balances::Event<R>>,
{
	fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
		if let Some(mut fees) = fees_then_tips.next() {
			if let Some(tips) = fees_then_tips.next() {
				tips.merge_into(&mut fees);
			}
			<ToStakingPot<R> as OnUnbalanced<_>>::on_unbalanced(fees);
		}
	}
}

/// A `HandleCredit` implementation that naively transfers the fees to the block author.
/// Will drop and burn the assets in case the transfer fails.
pub struct AssetsToBlockAuthor<R, I>(PhantomData<(R, I)>);
impl<R, I> HandleCredit<AccountIdOf<R>, pallet_assets::Pallet<R, I>> for AssetsToBlockAuthor<R, I>
where
	I: 'static,
	R: pallet_authorship::Config + pallet_assets::Config<I>,
	AccountIdOf<R>: From<polkadot_primitives::AccountId> + Into<polkadot_primitives::AccountId>,
{
	fn handle_credit(credit: Credit<AccountIdOf<R>, pallet_assets::Pallet<R, I>>) {
		if let Some(author) = pallet_authorship::Pallet::<R>::author() {
			// In case of error: Will drop the result triggering the `OnDrop` of the imbalance.
			let _ = pallet_assets::Pallet::<R, I>::resolve(&author, credit);
		}
	}
}

/// Allow checking in assets that have issuance > 0.
pub struct NonZeroIssuance<AccountId, Assets>(PhantomData<(AccountId, Assets)>);
impl<AccountId, Assets> Contains<<Assets as fungibles::Inspect<AccountId>>::AssetId>
	for NonZeroIssuance<AccountId, Assets>
where
	Assets: fungibles::Inspect<AccountId>,
{
	fn contains(id: &<Assets as fungibles::Inspect<AccountId>>::AssetId) -> bool {
		!Assets::total_issuance(id.clone()).is_zero()
	}
}

/// Allow checking in assets that exists.
pub struct AssetExists<AccountId, Assets>(PhantomData<(AccountId, Assets)>);
impl<AccountId, Assets> Contains<<Assets as fungibles::Inspect<AccountId>>::AssetId>
	for AssetExists<AccountId, Assets>
where
	Assets: fungibles::Inspect<AccountId>,
{
	fn contains(id: &<Assets as fungibles::Inspect<AccountId>>::AssetId) -> bool {
		Assets::asset_exists(id.clone())
	}
}

/// Asset filter that allows all assets from a certain location.
pub struct AssetsFrom<T>(PhantomData<T>);
impl<T: Get<Location>> ContainsPair<Asset, Location> for AssetsFrom<T> {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let loc = T::get();
		&loc == origin &&
			matches!(asset, Asset { id: AssetId(asset_loc), fun: Fungible(_a) }
			if asset_loc.match_and_split(&loc).is_some())
	}
}

/// Type alias to conveniently refer to the `Currency::Balance` associated type.
pub type BalanceOf<T> =
	<pallet_balances::Pallet<T> as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Implements `OnUnbalanced::on_unbalanced` to teleport slashed assets to relay chain treasury
/// account.
pub struct ToParentTreasury<TreasuryAccount, AccountIdConverter, T>(
	PhantomData<(TreasuryAccount, AccountIdConverter, T)>,
);

impl<TreasuryAccount, AccountIdConverter, T> OnUnbalanced<NegativeImbalance<T>>
	for ToParentTreasury<TreasuryAccount, AccountIdConverter, T>
where
	T: pallet_balances::Config + pallet_xcm::Config + frame_system::Config,
	<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId: From<AccountIdOf<T>>,
	[u8; 32]: From<<T as frame_system::Config>::AccountId>,
	TreasuryAccount: Get<AccountIdOf<T>>,
	AccountIdConverter: ConvertLocation<AccountIdOf<T>>,
	BalanceOf<T>: Into<Fungibility>,
{
	fn on_unbalanced(amount: NegativeImbalance<T>) {
		let amount = match amount.drop_zero() {
			Ok(..) => return,
			Err(amount) => amount,
		};
		let imbalance = amount.peek();
		let root_location: Location = Here.into();
		let root_account: AccountIdOf<T> =
			match AccountIdConverter::convert_location(&root_location) {
				Some(a) => a,
				None => {
					log::warn!("Failed to convert root origin into account id");
					return
				},
			};
		let treasury_account: AccountIdOf<T> = TreasuryAccount::get();

		<pallet_balances::Pallet<T>>::resolve_creating(&root_account, amount);

		let result = <pallet_xcm::Pallet<T>>::limited_teleport_assets(
			<<T as frame_system::Config>::RuntimeOrigin>::root(),
			Box::new(Parent.into()),
			Box::new(
				Junction::AccountId32 { network: None, id: treasury_account.into() }
					.into_location()
					.into(),
			),
			Box::new((Parent, imbalance).into()),
			0,
			WeightLimit::Unlimited,
		);

		if let Err(err) = result {
			log::warn!("Failed to teleport slashed assets: {:?}", err);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		derive_impl, parameter_types,
		traits::{ConstU32, FindAuthor, ValidatorRegistration},
		PalletId,
	};
	use frame_system::{limits, EnsureRoot};
	use pallet_collator_selection::IdentityCollator;
	use polkadot_primitives::AccountId;
	use sp_core::{ConstU64, H256};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentityLookup},
		BuildStorage, Perbill,
	};
	use xcm::prelude::*;

	type Block = frame_system::mocking::MockBlock<Test>;
	const TEST_ACCOUNT: AccountId = AccountId::new([1; 32]);

	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
			Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
			CollatorSelection: pallet_collator_selection::{Pallet, Call, Storage, Event<T>},
		}
	);

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub BlockLength: limits::BlockLength = limits::BlockLength::max(2 * 1024);
		pub const AvailableBlockRatio: Perbill = Perbill::one();
		pub const MaxReserves: u32 = 50;
	}

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for Test {
		type BaseCallFilter = frame_support::traits::Everything;
		type RuntimeOrigin = RuntimeOrigin;
		type Nonce = u64;
		type RuntimeCall = RuntimeCall;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Block = Block;
		type RuntimeEvent = RuntimeEvent;
		type BlockHashCount = BlockHashCount;
		type BlockLength = BlockLength;
		type BlockWeights = ();
		type DbWeight = ();
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = pallet_balances::AccountData<u64>;
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ();
		type OnSetCode = ();
		type MaxConsumers = frame_support::traits::ConstU32<16>;
	}

	impl pallet_balances::Config for Test {
		type Balance = u64;
		type RuntimeEvent = RuntimeEvent;
		type DustRemoval = ();
		type ExistentialDeposit = ConstU64<1>;
		type AccountStore = System;
		type MaxLocks = ();
		type WeightInfo = ();
		type MaxReserves = MaxReserves;
		type ReserveIdentifier = [u8; 8];
		type RuntimeHoldReason = RuntimeHoldReason;
		type RuntimeFreezeReason = RuntimeFreezeReason;
		type FreezeIdentifier = ();
		type MaxFreezes = ConstU32<1>;
	}

	pub struct OneAuthor;
	impl FindAuthor<AccountId> for OneAuthor {
		fn find_author<'a, I>(_: I) -> Option<AccountId>
		where
			I: 'a,
		{
			Some(TEST_ACCOUNT)
		}
	}

	pub struct IsRegistered;
	impl ValidatorRegistration<AccountId> for IsRegistered {
		fn is_registered(_id: &AccountId) -> bool {
			true
		}
	}

	parameter_types! {
		pub const PotId: PalletId = PalletId(*b"PotStake");
	}

	impl pallet_collator_selection::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type Currency = Balances;
		type UpdateOrigin = EnsureRoot<AccountId>;
		type PotId = PotId;
		type MaxCandidates = ConstU32<20>;
		type MinEligibleCollators = ConstU32<1>;
		type MaxInvulnerables = ConstU32<20>;
		type ValidatorId = <Self as frame_system::Config>::AccountId;
		type ValidatorIdOf = IdentityCollator;
		type ValidatorRegistration = IsRegistered;
		type KickThreshold = ();
		type WeightInfo = ();
	}

	impl pallet_authorship::Config for Test {
		type FindAuthor = OneAuthor;
		type EventHandler = ();
	}

	pub fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		// We use default for brevity, but you can configure as desired if needed.
		pallet_balances::GenesisConfig::<Test>::default()
			.assimilate_storage(&mut t)
			.unwrap();
		t.into()
	}

	#[test]
	fn test_fees_and_tip_split() {
		new_test_ext().execute_with(|| {
			let fee = Balances::issue(10);
			let tip = Balances::issue(20);

			assert_eq!(Balances::free_balance(TEST_ACCOUNT), 0);

			DealWithFees::on_unbalanceds(vec![fee, tip].into_iter());

			// Author gets 100% of tip and 100% of fee = 30
			assert_eq!(Balances::free_balance(CollatorSelection::account_id()), 30);
		});
	}

	#[test]
	fn assets_from_filters_correctly() {
		parameter_types! {
			pub SomeSiblingParachain: Location = (Parent, Parachain(1234)).into();
		}

		let asset_location = SomeSiblingParachain::get()
			.pushed_with_interior(GeneralIndex(42))
			.expect("location will only have 2 junctions; qed");
		let asset = Asset { id: AssetId(asset_location), fun: 1_000_000u128.into() };
		assert!(
			AssetsFrom::<SomeSiblingParachain>::contains(&asset, &SomeSiblingParachain::get()),
			"AssetsFrom should allow assets from any of its interior locations"
		);
	}
}
