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

use crate::{xcm_config, Hrmp, HrmpRelay, Runtime};
use alloc::{boxed::Box, vec, vec::Vec};
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::{DispatchResult, ValueQuery},
	weights::Weight,
};
use frame_system::RawOrigin;
use hrmp_primitives::MessageToPara;
use polkadot_primitives::Balance;
use polkadot_runtime_common::identity_migrator::{OnReapIdentity, WeightInfo};
use polkadot_runtime_parachains::hrmp::deposits::{
	ChannelDepositProvider, DepositChangeRequest, DepositChangeResponse,
};
use sp_runtime::DispatchError;
use westend_runtime_constants::{
	currency::*, hrmp_deposits::CORETIME_PALLET_INDEX, system_parachain::BROKER_ID,
};
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
			weights::polkadot_runtime_common_identity_migrator::WeightInfo as MigratorWeights,
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
				call: poke.encode().into(),
				fallback_max_weight: Some(remote_weight_limit),
			},
		]);

		// send
		<pallet_xcm::Pallet<Runtime>>::send(
			RawOrigin::Root.into(),
			Box::new(VersionedLocation::from(destination)),
			Box::new(VersionedXcm::from(program)),
		)?;
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful_identity_reaping(_: &AccountId, _: u32, _: u32) {
		crate::Dmp::make_parachain_reachable(1004);
	}
}

/// Sends `call`, encoded for the Coretime chain, to be dispatched there as this chain
/// (`Superuser`, which the Coretime chain turns into Root).
pub fn send_to_coretime(call: Vec<u8>) -> Result<(), SendError> {
	let message = Xcm(vec![
		Instruction::UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
		Instruction::Transact {
			origin_kind: OriginKind::Superuser,
			fallback_max_weight: None,
			call: call.into(),
		},
	]);
	send_xcm::<xcm_config::XcmRouter>(Location::new(0, [Parachain(BROKER_ID)]), message).map(|_| ())
}

/// Sends `pallet-hrmp-relay`'s messages to `pallet-hrmp-para` on the Coretime chain.
pub struct HrmpToCoretime;
impl pallet_hrmp_relay::SendToPara for HrmpToCoretime {
	fn send(message: MessageToPara) -> Result<(), ()> {
		// `HrmpPara::receive` on the Coretime chain.
		let call = (CORETIME_PALLET_INDEX, pallet_hrmp_para::RECEIVE_CALL_INDEX, message).encode();
		send_to_coretime(call).map_err(|e| {
			log::error!(target: "runtime::hrmp", "sending to the Coretime chain failed: {e:?}");
		})
	}
}

/// Where `hrmp` holds its channel deposits: reserved here until [`HrmpDepositsOnCoretime`] is
/// set, and held on the Coretime chain through `HrmpRelay` from then on.
pub struct HrmpDeposits;

/// Whether `hrmp`'s channel deposits are held on the Coretime chain.
///
/// TODO: no migration moves the deposits reserved here to the Coretime chain yet. It should
/// move each para's reserve there, hand every deposit to `HrmpPara` through
/// `ReceiveMigratedDeposits`, and set this once done, the way the Polkadot and Kusama
/// migration keys its switch on its own stage. Until then, root sets it with
/// `System::set_storage`, and a channel opened before is left with its deposits reserved here.
#[frame_support::storage_alias]
pub type HrmpDepositsOnCoretime = StorageValue<HrmpDepositsSwitch, bool, ValueQuery>;

impl ChannelDepositProvider for HrmpDeposits {
	type OnDepositSet = Hrmp;

	fn try_set_deposit(
		request: DepositChangeRequest,
	) -> Result<DepositChangeResponse, DispatchError> {
		if HrmpDepositsOnCoretime::get() {
			HrmpRelay::try_set_deposit(request)
		} else {
			Hrmp::try_set_deposit(request)
		}
	}

	fn weight_of_try_set_deposit() -> Weight {
		<Runtime as frame_system::Config>::DbWeight::get().reads(1).saturating_add(
			HrmpRelay::weight_of_try_set_deposit().max(Hrmp::weight_of_try_set_deposit()),
		)
	}
}

/// Makes the Coretime chain reachable for the benchmarks of `pallet-hrmp-relay`.
#[cfg(feature = "runtime-benchmarks")]
pub struct HrmpBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_hrmp_relay::BenchmarkHelper for HrmpBenchmarkHelper {
	fn ensure_send_to_para() {
		crate::Dmp::make_parachain_reachable(BROKER_ID);
	}
}
