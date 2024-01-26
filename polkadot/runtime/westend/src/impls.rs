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

use crate::xcm_config;
use frame_support::pallet_prelude::DispatchResult;
use frame_system::RawOrigin;
use parity_scale_codec::{Decode, Encode};
use primitives::Balance;
use runtime_common::identity_migrator::{OnReapIdentity, WeightInfo};
use sp_std::{marker::PhantomData, prelude::*};
use westend_runtime_constants::currency::*;
use xcm::{latest::prelude::*, VersionedLocation, VersionedXcm};
use xcm_executor::traits::TransactAsset;

/// A type containing the encoding of the People Chain pallets in its runtime. Used to construct any
/// remote calls. The codec index must correspond to the index of `IdentityMigrator` in the
/// `construct_runtime` of the remote chain.
#[derive(Encode, Decode)]
enum PeopleRuntimePallets<AccountId: Encode> {
	#[codec(index = 248)]
	IdentityMigrator(IdentityMigratorCalls<AccountId>),
}

/// Call encoding for the calls needed from the Identity Migrator pallet.
#[derive(Encode, Decode)]
enum IdentityMigratorCalls<AccountId: Encode> {
	#[codec(index = 1)]
	PokeDeposit(AccountId),
}

/// Type that implements `OnReapIdentity` that will send the deposit needed to store the same
/// information on a parachain, sends the deposit there, and then updates it.
pub struct ToParachainIdentityReaper<Runtime, AccountId>(PhantomData<(Runtime, AccountId)>);
impl<Runtime, AccountId> ToParachainIdentityReaper<Runtime, AccountId> {
	/// Calculate the balance needed on the remote chain based on the `IdentityInfo` and `Subs` on
	/// this chain. The total includes:
	///
	/// - Identity basic deposit
	/// - `IdentityInfo` byte deposit
	/// - Sub accounts deposit
	/// - 2x existential deposit (1 for account existence, 1 such that the user can transact)
	fn calculate_remote_deposit(bytes: u32, subs: u32) -> Balance {
		// Remote deposit constants. Parachain uses `deposit / 100`
		// Source:
		// https://github.com/paritytech/polkadot-sdk/blob/a146918/cumulus/parachains/common/src/westend.rs#L28
		//
		// Parachain Deposit Configuration:
		//
		// pub const BasicDeposit: Balance = deposit(1, 17);
		// pub const ByteDeposit: Balance = deposit(0, 1);
		// pub const SubAccountDeposit: Balance = deposit(1, 53);
		// pub const EXISTENTIAL_DEPOSIT: Balance = constants::currency::EXISTENTIAL_DEPOSIT / 10;
		let para_basic_deposit = deposit(1, 17) / 100;
		let para_byte_deposit = deposit(0, 1) / 100;
		let para_sub_account_deposit = deposit(1, 53) / 100;
		let para_existential_deposit = EXISTENTIAL_DEPOSIT / 10;

		// pallet deposits
		let id_deposit =
			para_basic_deposit.saturating_add(para_byte_deposit.saturating_mul(bytes as Balance));
		let subs_deposit = para_sub_account_deposit.saturating_mul(subs as Balance);

		id_deposit
			.saturating_add(subs_deposit)
			.saturating_add(para_existential_deposit.saturating_mul(2))
	}
}

// Note / Warning: This implementation should only be used in a transactional context. If not, then
// an error could result in assets being burned.
impl<Runtime, AccountId> OnReapIdentity<AccountId> for ToParachainIdentityReaper<Runtime, AccountId>
where
	Runtime: frame_system::Config + pallet_xcm::Config,
	AccountId: Into<[u8; 32]> + Clone + Encode,
{
	fn on_reap_identity(who: &AccountId, fields: u32, subs: u32) -> DispatchResult {
		use crate::{
			impls::IdentityMigratorCalls::PokeDeposit,
			weights::runtime_common_identity_migrator::WeightInfo as MigratorWeights,
		};

		let total_to_send = Self::calculate_remote_deposit(fields, subs);

		// define asset / destination from relay perspective
		let wnd = Asset { id: AssetId(Here.into_location()), fun: Fungible(total_to_send) };
		// People Chain: ParaId 1004
		let destination: Location = Location::new(0, Parachain(1004));

		// Do `check_out` accounting since the XCM Executor's `InitiateTeleport` doesn't support
		// unpaid teleports.

		// withdraw the asset from `who`
		let who_origin =
			Junction::AccountId32 { network: None, id: who.clone().into() }.into_location();
		let _withdrawn = xcm_config::LocalAssetTransactor::withdraw_asset(&wnd, &who_origin, None)
			.map_err(|err| {
				log::error!(
					target: "runtime::on_reap_identity",
					"withdraw_asset(what: {:?}, who_origin: {:?}) error: {:?}",
					wnd, who_origin, err
				);
				pallet_xcm::Error::<Runtime>::LowBalance
			})?;

		// check out
		xcm_config::LocalAssetTransactor::can_check_out(
			&destination,
			&wnd,
			// not used in AssetTransactor
			&XcmContext { origin: None, message_id: [0; 32], topic: None },
		)
		.map_err(|err| {
			log::error!(
				target: "runtime::on_reap_identity",
				"can_check_out(destination: {:?}, asset: {:?}, _) error: {:?}",
				destination, wnd, err
			);
			pallet_xcm::Error::<Runtime>::CannotCheckOutTeleport
		})?;
		xcm_config::LocalAssetTransactor::check_out(
			&destination,
			&wnd,
			// not used in AssetTransactor
			&XcmContext { origin: None, message_id: [0; 32], topic: None },
		);

		// reanchor
		let wnd_reanchored: Assets =
			vec![Asset { id: AssetId(Location::new(1, Here)), fun: Fungible(total_to_send) }]
				.into();

		let poke = PeopleRuntimePallets::<AccountId>::IdentityMigrator(PokeDeposit(who.clone()));
		let remote_weight_limit = MigratorWeights::<Runtime>::poke_deposit().saturating_mul(2);

		// Actual program to execute on People Chain.
		let program: Xcm<()> = Xcm(vec![
			// Unpaid as this is constructed by the system, once per user. The user shouldn't have
			// their balance reduced by teleport fees for the favor of migrating.
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			// Receive the asset into holding.
			ReceiveTeleportedAsset(wnd_reanchored),
			// Deposit into the user's account.
			DepositAsset {
				assets: Wild(AllCounted(1)),
				beneficiary: Junction::AccountId32 { network: None, id: who.clone().into() }
					.into_location()
					.into(),
			},
			// Poke the deposit to reserve the appropriate amount on the parachain.
			Transact {
				origin_kind: OriginKind::Superuser,
				require_weight_at_most: remote_weight_limit,
				call: poke.encode().into(),
			},
		]);

		// send
		let _ = <pallet_xcm::Pallet<Runtime>>::send(
			RawOrigin::Root.into(),
			Box::new(VersionedLocation::V4(destination)),
			Box::new(VersionedXcm::V4(program)),
		)?;
		Ok(())
	}
}
