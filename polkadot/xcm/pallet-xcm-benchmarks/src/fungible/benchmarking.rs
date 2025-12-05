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

use super::*;
use crate::{account_and_location, new_executor, AssetTransactorOf, EnsureDelivery, XcmCallOf};
use alloc::{vec, vec::Vec};
use frame_benchmarking::{benchmarks_instance_pallet, BenchmarkError, BenchmarkResult};
use frame_support::{
	pallet_prelude::Get,
	traits::fungible::{Inspect, Mutate},
	weights::Weight,
	BoundedVec,
};
use sp_runtime::traits::Bounded;
use xcm::latest::{prelude::*, AssetTransferFilter, MAX_ITEMS_IN_ASSETS};
use xcm_executor::{
	traits::{ConvertLocation, FeeReason, TransactAsset},
	AssetsInHolding,
};

/// Helper function to convert Assets to AssetsInHolding by minting each asset.
/// This is used for benchmark setup where we need to create imbalances.
fn assets_to_holding<T: crate::Config>(assets: &Assets) -> Result<AssetsInHolding, XcmError> {
	let context = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
	let mut holding = AssetsInHolding::new();
	for asset in assets.inner() {
		let minted = <AssetTransactorOf<T>>::mint_asset(asset, &context)?;
		holding.subsume_assets(minted);
	}
	Ok(holding)
}

benchmarks_instance_pallet! {
	where_clause { where
		<
			<
				T::TransactAsset
				as
				Inspect<T::AccountId>
			>::Balance
			as
			TryInto<u128>
		>::Error: core::fmt::Debug,
	}

	withdraw_asset {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let worst_case_holding = T::worst_case_holding(0);
		let asset = T::get_asset();

		let context = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
		let holdings = <AssetTransactorOf<T>>::mint_asset(&asset, &context).unwrap();
		<AssetTransactorOf<T>>::deposit_asset(holdings, &sender_location, Some(&context)).unwrap();

		let mut executor = new_executor::<T>(sender_location);
		executor.set_holding(worst_case_holding);
		let instruction = Instruction::<XcmCallOf<T>>::WithdrawAsset(vec![asset.clone()].into());
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
		assert!(executor.holding().ensure_contains(&vec![asset].into()).is_ok());
	}

	transfer_asset {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let asset = T::get_asset();
		let assets: Assets = vec![asset.clone()].into();
		// this xcm doesn't use holding

		let dest_location = T::valid_destination()?;
		let dest_account = T::AccountIdConverter::convert_location(&dest_location).unwrap();

		let context = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
		let holdings = <AssetTransactorOf<T>>::mint_asset(&asset, &context).unwrap();
		<AssetTransactorOf<T>>::deposit_asset(holdings, &sender_location, Some(&context)).unwrap();
		// We deposit the asset twice so we have enough for ED after transferring
		let holdings = <AssetTransactorOf<T>>::mint_asset(&asset, &context).unwrap();
		<AssetTransactorOf<T>>::deposit_asset(holdings, &sender_location, Some(&context)).unwrap();

		let mut executor = new_executor::<T>(sender_location);
		let instruction = Instruction::TransferAsset { assets, beneficiary: dest_location };
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
	}

	transfer_reserve_asset {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let dest_location = T::valid_destination()?;
		let dest_account = T::AccountIdConverter::convert_location(&dest_location).unwrap();

		let (expected_fees_mode, expected_assets_in_holding) = T::DeliveryHelper::ensure_successful_delivery(
			&sender_location,
			&dest_location,
			FeeReason::TransferReserveAsset
		);

		let asset = T::get_asset();
		let context = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
		let holdings = <AssetTransactorOf<T>>::mint_asset(&asset, &context).unwrap();
		<AssetTransactorOf<T>>::deposit_asset(holdings, &sender_location, Some(&context)).unwrap();
		// We deposit the asset twice so we have enough for ED after transferring
		let holdings = <AssetTransactorOf<T>>::mint_asset(&asset, &context).unwrap();
		<AssetTransactorOf<T>>::deposit_asset(holdings, &sender_location, Some(&context)).unwrap();
		let assets: Assets = vec![asset].into();

		let mut executor = new_executor::<T>(sender_location);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			// Mint real assets for delivery fees and add to holding
			executor.set_holding(assets_to_holding::<T>(&expected_assets_in_holding).unwrap());
		}

		let instruction = Instruction::TransferReserveAsset {
			assets,
			dest: dest_location,
			xcm: Xcm::new()
		};
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
		// TODO: Check sender queue is not empty. #4426
	}

	reserve_asset_deposited {
		let (trusted_reserve, transferable_reserve_asset) = T::TrustedReserve::get().or_else(|| {
			Some((Default::default(), T::get_asset()))
		})
			.ok_or(BenchmarkError::Override(
				BenchmarkResult::from_weight(Weight::MAX)
			))?;

		let assets: Assets = vec![ transferable_reserve_asset ].into();

		let mut executor = new_executor::<T>(trusted_reserve);
		let instruction = Instruction::ReserveAssetDeposited(assets.clone());
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
		assert!(executor.holding().ensure_contains(&assets).is_ok());
	}

	initiate_reserve_withdraw {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let reserve = T::valid_destination().map_err(|_| BenchmarkError::Skip)?;

		let (expected_fees_mode, expected_assets_in_holding) = T::DeliveryHelper::ensure_successful_delivery(
			&sender_location,
			&reserve,
			FeeReason::InitiateReserveWithdraw,
		);
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		// generate holding and add possible required fees
		let holding = if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			let mut holding = T::worst_case_holding(1 + expected_assets_in_holding.len() as u32);
			// Mint real assets for delivery fees and merge into holding
			let real_assets = assets_to_holding::<T>(&expected_assets_in_holding).unwrap();
			holding.subsume_assets(real_assets);
			holding
		} else {
			T::worst_case_holding(1)
		};

		// Build Assets descriptor from AssetsInHolding for the instruction (before consuming holding)
		let withdraw_assets: Assets = {
			let mut assets = Vec::new();
			// Add fungible assets up to MAX_ITEMS_IN_ASSETS
			for (asset_id, imbalance) in holding.fungible.iter().take(MAX_ITEMS_IN_ASSETS) {
				assets.push(Asset {
					id: asset_id.clone(),
					fun: Fungible(imbalance.amount()),
				});
			}
			// Add non-fungible assets if we haven't hit the limit
			let remaining = MAX_ITEMS_IN_ASSETS.saturating_sub(assets.len());
			for (asset_id, instance) in holding.non_fungible.iter().take(remaining) {
				assets.push(Asset {
					id: asset_id.clone(),
					fun: NonFungible(instance.clone()),
				});
			}
			assets.into()
		};

		let mut executor = new_executor::<T>(sender_location);
		executor.set_holding(holding);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}

		let instruction = Instruction::InitiateReserveWithdraw {
			// Worst case is looking through all holdings for every asset explicitly - respecting the limit `MAX_ITEMS_IN_ASSETS`.
			assets: Definite(withdraw_assets),
			reserve,
			xcm: Xcm(vec![])
		};
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);
		// The execute completing successfully is as good as we can check.
		// TODO: Potentially add new trait to XcmSender to detect a queued outgoing message. #4426
	}

	receive_teleported_asset {
		// If there is no trusted teleporter, then we skip this benchmark.
		let (trusted_teleporter, teleportable_asset) = T::TrustedTeleporter::get()
			.ok_or(BenchmarkError::Skip)?;

		if let Some((checked_account, _)) = T::CheckedAccount::get() {
			T::TransactAsset::mint_into(
				&checked_account,
				<
					T::TransactAsset
					as
					Inspect<T::AccountId>
				>::Balance::max_value() / 2u32.into(),
			)?;
		}

		let assets: Assets = vec![ teleportable_asset ].into();

		let mut executor = new_executor::<T>(trusted_teleporter);
		let instruction = Instruction::ReceiveTeleportedAsset(assets.clone());
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm).map_err(|_| {
			BenchmarkError::Override(
				BenchmarkResult::from_weight(Weight::MAX)
			)
		})?;
	} verify {
		assert!(executor.holding().ensure_contains(&assets).is_ok());
	}

	deposit_asset {
		let asset = T::get_asset();
		let mut holding = T::worst_case_holding(1);

		// Add our asset to the holding.
		let real_asset = assets_to_holding::<T>(&vec![asset.clone()].into()).unwrap();
		holding.subsume_assets(real_asset);

		// our dest must have no balance initially.
		let dest_location = T::valid_destination()?;
		let dest_account = T::AccountIdConverter::convert_location(&dest_location).unwrap();

		// Ensure that origin can send to destination (e.g. setup delivery fees, ensure router setup, ...)
		let (_, _) = T::DeliveryHelper::ensure_successful_delivery(
			&Default::default(),
			&dest_location,
			FeeReason::ChargeFees,
		);

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding);
		let instruction = Instruction::<XcmCallOf<T>>::DepositAsset {
			assets: asset.into(),
			beneficiary: dest_location,
		};
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
	}

	deposit_reserve_asset {
		let asset = T::get_asset();
		let mut holding = T::worst_case_holding(1);

		// Add our asset to the holding.
		let real_asset = assets_to_holding::<T>(&vec![asset.clone()].into()).unwrap();
		holding.subsume_assets(real_asset);

		// our dest must have no balance initially.
		let dest_location = T::valid_destination()?;
		let dest_account = T::AccountIdConverter::convert_location(&dest_location).unwrap();

		// Ensure that origin can send to destination (e.g. setup delivery fees, ensure router setup, ...)
		let (_, _) = T::DeliveryHelper::ensure_successful_delivery(
			&Default::default(),
			&dest_location,
			FeeReason::ChargeFees,
		);

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding);
		let instruction = Instruction::<XcmCallOf<T>>::DepositReserveAsset {
			assets: asset.into(),
			dest: dest_location,
			xcm: Xcm::new(),
		};
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
	}

	initiate_teleport {
		let asset = T::get_asset();
		let mut holding = T::worst_case_holding(0);

		// Add our asset to the holding.
		let real_asset = assets_to_holding::<T>(&vec![asset.clone()].into()).unwrap();
		holding.subsume_assets(real_asset);

		let dest_location =  T::valid_destination()?;

		// Ensure that origin can send to destination (e.g. setup delivery fees, ensure router setup, ...)
		let (_, _) = T::DeliveryHelper::ensure_successful_delivery(
			&Default::default(),
			&dest_location,
			FeeReason::ChargeFees,
		);

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding);
		let instruction = Instruction::<XcmCallOf<T>>::InitiateTeleport {
			assets: asset.into(),
			dest: dest_location,
			xcm: Xcm::new(),
		};
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
	}

	initiate_transfer {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let asset = T::get_asset();
		let mut holding = T::worst_case_holding(1);
		let dest_location =  T::valid_destination()?;

		// Ensure that origin can send to destination (e.g. setup delivery fees, ensure router setup, ...)
		let (_, _) = T::DeliveryHelper::ensure_successful_delivery(
			&sender_location,
			&dest_location,
			FeeReason::ChargeFees,
		);

		// Add our asset to the holding.
		let real_asset = assets_to_holding::<T>(&vec![asset.clone()].into()).unwrap();
		holding.subsume_assets(real_asset);

		let mut executor = new_executor::<T>(sender_location);
		executor.set_holding(holding);
		let instruction = Instruction::<XcmCallOf<T>>::InitiateTransfer {
			destination: dest_location,
			// ReserveDeposit is the most expensive filter.
			remote_fees: Some(AssetTransferFilter::ReserveDeposit(asset.clone().into())),
			// It's more expensive if we reanchor the origin.
			preserve_origin: true,
			assets: BoundedVec::truncate_from(vec![AssetTransferFilter::ReserveDeposit(asset.into())]),
			remote_xcm: Xcm::new(),
		};
		let xcm = Xcm(vec![instruction]);
	}: {
		executor.bench_process(xcm)?;
	} verify {
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::fungible::mock::new_test_ext(),
		crate::fungible::mock::Test
	);
}
