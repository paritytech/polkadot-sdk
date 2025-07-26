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
#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{account_and_location, new_executor, EnsureDelivery, XcmCallOf};
use alloc::{vec, vec::Vec};
use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::{traits::fungible::Inspect, BoundedVec};
use xcm::{
	latest::{prelude::*, MaxDispatchErrorLen, MaybeErrorCode, Weight, MAX_ITEMS_IN_ASSETS},
	DoubleEncoded,
};
use xcm_executor::{
	traits::{ConvertLocation, FeeReason},
	ExecutorError, FeesMode,
};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn report_holding() -> Result<(), BenchmarkError> {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let destination = T::valid_destination().map_err(|_| BenchmarkError::Skip)?;

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(
				&sender_location,
				&destination,
				FeeReason::Report,
			);
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		// generate holding and add possible required fees
		let holding = if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			let mut holding = T::worst_case_holding(expected_assets_in_holding.len() as u32);
			for a in expected_assets_in_holding.into_inner() {
				holding.push(a);
			}
			holding
		} else {
			T::worst_case_holding(0)
		};

		let mut executor = new_executor::<T>(sender_location);
		executor.set_holding(holding.clone().into());
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}

		let instruction = Instruction::<XcmCallOf<T>>::ReportHolding {
			response_info: QueryResponseInfo {
				destination,
				query_id: Default::default(),
				max_weight: Weight::MAX,
			},
			// Worst case is looking through all holdings for every asset explicitly - respecting
			// the limit `MAX_ITEMS_IN_ASSETS`.
			assets: Definite(
				holding
					.into_inner()
					.into_iter()
					.take(MAX_ITEMS_IN_ASSETS)
					.collect::<Vec<_>>()
					.into(),
			),
		};

		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);

		Ok(())
	}

	// This benchmark does not use any additional orders or instructions. This should be managed
	// by the `deep` and `shallow` implementation.
	#[benchmark]
	fn buy_execution() -> Result<(), BenchmarkError> {
		let holding = T::worst_case_holding(0).into();

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding);

		// The worst case we want for buy execution in terms of
		// fee asset and weight
		let (fee_asset, weight_limit) = T::worst_case_for_trader()?;

		let instruction = Instruction::<XcmCallOf<T>>::BuyExecution {
			fees: fee_asset,
			weight_limit: weight_limit.into(),
		};

		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}

		Ok(())
	}

	#[benchmark]
	fn pay_fees() -> Result<(), BenchmarkError> {
		let holding = T::worst_case_holding(0).into();

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding);
		// Set some weight to be paid for.
		executor.set_message_weight(Weight::from_parts(100_000_000, 100_000));

		let (fee_asset, _): (Asset, WeightLimit) = T::worst_case_for_trader().unwrap();

		let instruction = Instruction::<XcmCallOf<T>>::PayFees { asset: fee_asset };

		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		Ok(())
	}

	#[benchmark]
	fn asset_claimer() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let (_, sender_location) = account_and_location::<T>(1);

		let instruction = Instruction::SetHints {
			hints: BoundedVec::<Hint, HintNumVariants>::truncate_from(vec![AssetClaimer {
				location: sender_location.clone(),
			}]),
		};

		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.asset_claimer(), Some(sender_location.clone()));

		Ok(())
	}

	#[benchmark]
	fn query_response() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let (query_id, response) = T::worst_case_response();
		let max_weight = Weight::MAX;
		let querier: Option<Location> = Some(Here.into());
		let instruction = Instruction::QueryResponse { query_id, response, max_weight, querier };
		let xcm = Xcm(vec![instruction]);

		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// The assert above is enough to show this XCM succeeded

		Ok(())
	}

	// We don't care about the call itself, since that is accounted for in the weight parameter
	// and included in the final weight calculation. So this is just the overhead of submitting
	// a noop call.
	#[benchmark]
	fn transact() -> Result<(), BenchmarkError> {
		let (origin, noop_call) = T::transact_origin_and_runtime_call()?;
		let mut executor = new_executor::<T>(origin);
		let double_encoded_noop_call: DoubleEncoded<_> = noop_call.encode().into();

		let instruction = Instruction::Transact {
			origin_kind: OriginKind::SovereignAccount,
			call: double_encoded_noop_call,
			fallback_max_weight: None,
		};
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// TODO Make the assertion configurable?

		Ok(())
	}

	#[benchmark]
	fn refund_surplus() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let holding_assets = T::worst_case_holding(1);
		// We can already buy execution since we'll load the holding register manually
		let (asset_for_fees, _): (Asset, WeightLimit) = T::worst_case_for_trader().unwrap();

		let previous_xcm = Xcm(vec![BuyExecution {
			fees: asset_for_fees,
			weight_limit: Limited(Weight::from_parts(1337, 1337)),
		}]);
		executor.set_holding(holding_assets.into());
		executor.set_total_surplus(Weight::from_parts(1337, 1337));
		executor.set_total_refunded(Weight::zero());
		executor
			.bench_process(previous_xcm)
			.expect("Holding has been loaded, so we can buy execution here");

		let instruction = Instruction::<XcmCallOf<T>>::RefundSurplus;
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			let _result = executor.bench_process(xcm)?;
		}
		assert_eq!(executor.total_surplus(), &Weight::from_parts(1337, 1337));
		assert_eq!(executor.total_refunded(), &Weight::from_parts(1337, 1337));

		Ok(())
	}

	#[benchmark]
	fn set_error_handler() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let instruction = Instruction::<XcmCallOf<T>>::SetErrorHandler(Xcm(vec![]));
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.error_handler(), &Xcm(vec![]));

		Ok(())
	}

	#[benchmark]
	fn set_appendix() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let appendix = Xcm(vec![]);
		let instruction = Instruction::<XcmCallOf<T>>::SetAppendix(appendix);
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.appendix(), &Xcm(vec![]));
		Ok(())
	}

	#[benchmark]
	fn clear_error() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		executor.set_error(Some((5u32, XcmError::Overflow)));
		let instruction = Instruction::<XcmCallOf<T>>::ClearError;
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert!(executor.error().is_none());
		Ok(())
	}

	#[benchmark]
	fn descend_origin() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let who = Junctions::from([OnlyChild, OnlyChild]);
		let instruction = Instruction::DescendOrigin(who.clone());
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.origin(), &Some(Location { parents: 0, interior: who }),);

		Ok(())
	}

	#[benchmark]
	fn execute_with_origin() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let who: Junctions = Junctions::from([AccountId32 { id: [0u8; 32], network: None }]);
		let instruction = Instruction::ExecuteWithOrigin {
			descendant_origin: Some(who.clone()),
			xcm: Xcm(vec![]),
		};
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor
				.bench_process(xcm)
				.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		}
		assert_eq!(executor.origin(), &Some(Location { parents: 0, interior: Here }),);

		Ok(())
	}

	#[benchmark]
	fn clear_origin() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let instruction = Instruction::ClearOrigin;
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.origin(), &None);
		Ok(())
	}

	#[benchmark]
	fn report_error() -> Result<(), BenchmarkError> {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let query_id = Default::default();
		let max_weight = Default::default();
		let destination = T::valid_destination().map_err(|_| BenchmarkError::Skip)?;

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(
				&sender_location,
				&destination,
				FeeReason::Report,
			);
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		let mut executor = new_executor::<T>(sender_location);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			executor.set_holding(expected_assets_in_holding.into());
		}
		executor.set_error(Some((0u32, XcmError::Unimplemented)));

		let instruction =
			Instruction::ReportError(QueryResponseInfo { query_id, destination, max_weight });
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);

		Ok(())
	}

	#[benchmark]
	fn claim_asset() -> Result<(), BenchmarkError> {
		use xcm_executor::traits::DropAssets;

		let (origin, ticket, assets) = T::claimable_asset()?;

		// We place some items into the asset trap to claim.
		<T::XcmConfig as xcm_executor::Config>::AssetTrap::drop_assets(
			&origin,
			assets.clone().into(),
			&XcmContext { origin: Some(origin.clone()), message_id: [0; 32], topic: None },
		);

		// Assets should be in the trap now.

		let mut executor = new_executor::<T>(origin);
		let instruction = Instruction::ClaimAsset { assets: assets.clone(), ticket };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert!(executor.holding().ensure_contains(&assets).is_ok());
		Ok(())
	}

	#[benchmark]
	fn trap() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let instruction = Instruction::Trap(10);
		let xcm = Xcm(vec![instruction]);
		// In order to access result in the verification below, it needs to be defined here.
		let result;
		#[block]
		{
			result = executor.bench_process(xcm);
		}
		assert!(matches!(result, Err(ExecutorError { xcm_error: XcmError::Trap(10), .. })));

		Ok(())
	}

	#[benchmark]
	fn subscribe_version() -> Result<(), BenchmarkError> {
		use xcm_executor::traits::VersionChangeNotifier;
		let origin = T::subscribe_origin()?;
		let query_id = Default::default();
		let max_response_weight = Default::default();
		let mut executor = new_executor::<T>(origin.clone());
		let instruction = Instruction::SubscribeVersion { query_id, max_response_weight };
		let xcm = Xcm(vec![instruction]);

		T::DeliveryHelper::ensure_successful_delivery(&origin, &origin, FeeReason::QueryPallet);

		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert!(<T::XcmConfig as xcm_executor::Config>::SubscriptionService::is_subscribed(
			&origin
		));
		Ok(())
	}

	#[benchmark]
	fn unsubscribe_version() -> Result<(), BenchmarkError> {
		use xcm_executor::traits::VersionChangeNotifier;
		// First we need to subscribe to notifications.
		let (origin, _) = T::transact_origin_and_runtime_call()?;

		T::DeliveryHelper::ensure_successful_delivery(&origin, &origin, FeeReason::QueryPallet);

		let query_id = Default::default();
		let max_response_weight = Default::default();
		<T::XcmConfig as xcm_executor::Config>::SubscriptionService::start(
			&origin,
			query_id,
			max_response_weight,
			&XcmContext { origin: Some(origin.clone()), message_id: [0; 32], topic: None },
		)
		.map_err(|_| "Could not start subscription")?;
		assert!(<T::XcmConfig as xcm_executor::Config>::SubscriptionService::is_subscribed(
			&origin
		));

		let mut executor = new_executor::<T>(origin.clone());
		let instruction = Instruction::UnsubscribeVersion;
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert!(!<T::XcmConfig as xcm_executor::Config>::SubscriptionService::is_subscribed(
			&origin
		));
		Ok(())
	}

	#[benchmark]
	fn burn_asset() -> Result<(), BenchmarkError> {
		let holding = T::worst_case_holding(0);
		let assets = holding.clone();

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding.into());

		let instruction = Instruction::BurnAsset(assets.into());
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert!(executor.holding().is_empty());
		Ok(())
	}

	#[benchmark]
	fn expect_asset() -> Result<(), BenchmarkError> {
		let holding = T::worst_case_holding(0);
		let assets = holding.clone();

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(holding.into());

		let instruction = Instruction::ExpectAsset(assets.into());
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// `execute` completing successfully is as good as we can check.

		Ok(())
	}

	#[benchmark]
	fn expect_origin() -> Result<(), BenchmarkError> {
		let expected_origin = Parent.into();
		let mut executor = new_executor::<T>(Default::default());

		let instruction = Instruction::ExpectOrigin(Some(expected_origin));
		let xcm = Xcm(vec![instruction]);
		let mut _result = Ok(());
		#[block]
		{
			_result = executor.bench_process(xcm);
		}
		assert!(matches!(
			_result,
			Err(ExecutorError { xcm_error: XcmError::ExpectationFalse, .. })
		));

		Ok(())
	}

	#[benchmark]
	fn expect_error() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		executor.set_error(Some((3u32, XcmError::Overflow)));

		let instruction = Instruction::ExpectError(None);
		let xcm = Xcm(vec![instruction]);
		let mut _result = Ok(());
		#[block]
		{
			_result = executor.bench_process(xcm);
		}
		assert!(matches!(
			_result,
			Err(ExecutorError { xcm_error: XcmError::ExpectationFalse, .. })
		));

		Ok(())
	}

	#[benchmark]
	fn expect_transact_status() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let worst_error =
			|| -> MaybeErrorCode { vec![0; MaxDispatchErrorLen::get() as usize].into() };
		executor.set_transact_status(worst_error());

		let instruction = Instruction::ExpectTransactStatus(worst_error());
		let xcm = Xcm(vec![instruction]);
		let mut _result = Ok(());
		#[block]
		{
			_result = executor.bench_process(xcm);
		}
		assert!(matches!(_result, Ok(..)));
		Ok(())
	}

	#[benchmark]
	fn query_pallet() -> Result<(), BenchmarkError> {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let query_id = Default::default();
		let destination = T::valid_destination().map_err(|_| BenchmarkError::Skip)?;
		let max_weight = Default::default();

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(
				&sender_location,
				&destination,
				FeeReason::QueryPallet,
			);
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);
		let mut executor = new_executor::<T>(sender_location);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			executor.set_holding(expected_assets_in_holding.into());
		}

		let valid_pallet = T::valid_pallet();
		let instruction = Instruction::QueryPallet {
			module_name: valid_pallet.module_name.as_bytes().to_vec(),
			response_info: QueryResponseInfo { destination, query_id, max_weight },
		};
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);
		// TODO: Potentially add new trait to XcmSender to detect a queued outgoing message. #4426

		Ok(())
	}

	#[benchmark]
	fn expect_pallet() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		let valid_pallet = T::valid_pallet();
		let instruction = Instruction::ExpectPallet {
			index: valid_pallet.index as u32,
			name: valid_pallet.name.as_bytes().to_vec(),
			module_name: valid_pallet.module_name.as_bytes().to_vec(),
			crate_major: valid_pallet.crate_version.major.into(),
			min_crate_minor: valid_pallet.crate_version.minor.into(),
		};
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// the execution succeeding is all we need to verify this xcm was successful
		Ok(())
	}

	#[benchmark]
	fn report_transact_status() -> Result<(), BenchmarkError> {
		let (sender_account, sender_location) = account_and_location::<T>(1);
		let query_id = Default::default();
		let destination = T::valid_destination().map_err(|_| BenchmarkError::Skip)?;
		let max_weight = Default::default();

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(
				&sender_location,
				&destination,
				FeeReason::Report,
			);
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		let mut executor = new_executor::<T>(sender_location);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			executor.set_holding(expected_assets_in_holding.into());
		}
		executor.set_transact_status(b"MyError".to_vec().into());

		let instruction = Instruction::ReportTransactStatus(QueryResponseInfo {
			query_id,
			destination,
			max_weight,
		});
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);
		// TODO: Potentially add new trait to XcmSender to detect a queued outgoing message. #4426
		Ok(())
	}

	#[benchmark]
	fn clear_transact_status() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		executor.set_transact_status(b"MyError".to_vec().into());

		let instruction = Instruction::ClearTransactStatus;
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.transact_status(), &MaybeErrorCode::Success);
		Ok(())
	}

	#[benchmark]
	fn set_topic() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());

		let instruction = Instruction::SetTopic([1; 32]);
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.topic(), &Some([1; 32]));
		Ok(())
	}

	#[benchmark]
	fn clear_topic() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		executor.set_topic(Some([2; 32]));

		let instruction = Instruction::ClearTopic;
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.topic(), &None);
		Ok(())
	}

	#[benchmark]
	fn exchange_asset() -> Result<(), BenchmarkError> {
		let (give, want) = T::worst_case_asset_exchange().map_err(|_| BenchmarkError::Skip)?;
		let assets = give.clone();

		let mut executor = new_executor::<T>(Default::default());
		executor.set_holding(give.into());
		let instruction =
			Instruction::ExchangeAsset { give: assets.into(), want: want.clone(), maximal: true };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert!(executor.holding().contains(&want.into()));
		Ok(())
	}

	#[benchmark]
	fn universal_origin() -> Result<(), BenchmarkError> {
		let (origin, alias) = T::universal_alias().map_err(|_| BenchmarkError::Skip)?;

		let mut executor = new_executor::<T>(origin);

		let instruction = Instruction::UniversalOrigin(alias);
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		use frame_support::traits::Get;
		let universal_location = <T::XcmConfig as xcm_executor::Config>::UniversalLocation::get();
		assert_eq!(
			executor.origin(),
			&Some(Junctions::from([alias]).relative_to(&universal_location))
		);

		Ok(())
	}

	#[benchmark]
	fn export_message(x: Linear<1, 1000>) -> Result<(), BenchmarkError> {
		// The `inner_xcm` influences `ExportMessage` total weight based on
		// `inner_xcm.encoded_size()`, so for this benchmark use smallest encoded instruction
		// to approximate weight per "unit" of encoded size; then actual weight can be estimated
		// to be `inner_xcm.encoded_size() * benchmarked_unit`.
		// Use `ClearOrigin` as the small encoded instruction.
		let inner_xcm = Xcm(vec![ClearOrigin; x as usize]);
		// Get `origin`, `network` and `destination` from configured runtime.
		let (origin, network, destination) = T::export_message_origin_and_destination()?;

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(
				&origin,
				&destination.clone().into(),
				FeeReason::Export { network, destination: destination.clone() },
			);
		let sender_account = T::AccountIdConverter::convert_location(&origin).unwrap();
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		let mut executor = new_executor::<T>(origin);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			executor.set_holding(expected_assets_in_holding.into());
		}
		let xcm =
			Xcm(vec![ExportMessage { network, destination: destination.clone(), xcm: inner_xcm }]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);
		// TODO: Potentially add new trait to XcmSender to detect a queued outgoing message. #4426
		Ok(())
	}

	#[benchmark]
	fn set_fees_mode() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		executor.set_fees_mode(FeesMode { jit_withdraw: false });

		let instruction = Instruction::SetFeesMode { jit_withdraw: true };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.fees_mode(), &FeesMode { jit_withdraw: true });
		Ok(())
	}

	#[benchmark]
	fn lock_asset() -> Result<(), BenchmarkError> {
		let (unlocker, owner, asset) = T::unlockable_asset()?;

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(&owner, &unlocker, FeeReason::LockAsset);
		let sender_account = T::AccountIdConverter::convert_location(&owner).unwrap();
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		// generate holding and add possible required fees
		let mut holding: Assets = asset.clone().into();
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			for a in expected_assets_in_holding.into_inner() {
				holding.push(a);
			}
		};

		let mut executor = new_executor::<T>(owner);
		executor.set_holding(holding.into());
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}

		let instruction = Instruction::LockAsset { asset, unlocker };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);
		// TODO: Potentially add new trait to XcmSender to detect a queued outgoing message. #4426
		Ok(())
	}

	#[benchmark]
	fn unlock_asset() -> Result<(), BenchmarkError> {
		use xcm_executor::traits::{AssetLock, Enact};

		let (unlocker, owner, asset) = T::unlockable_asset()?;

		let mut executor = new_executor::<T>(unlocker.clone());

		// We first place the asset in lock first...
		<T::XcmConfig as xcm_executor::Config>::AssetLocker::prepare_lock(
			unlocker,
			asset.clone(),
			owner.clone(),
		)
		.map_err(|_| BenchmarkError::Skip)?
		.enact()
		.map_err(|_| BenchmarkError::Skip)?;

		// ... then unlock them with the UnlockAsset instruction.
		let instruction = Instruction::UnlockAsset { asset, target: owner };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		Ok(())
	}

	#[benchmark]
	fn note_unlockable() -> Result<(), BenchmarkError> {
		use xcm_executor::traits::{AssetLock, Enact};

		let (unlocker, owner, asset) = T::unlockable_asset()?;

		let mut executor = new_executor::<T>(unlocker.clone());

		// We first place the asset in lock first...
		<T::XcmConfig as xcm_executor::Config>::AssetLocker::prepare_lock(
			unlocker,
			asset.clone(),
			owner.clone(),
		)
		.map_err(|_| BenchmarkError::Skip)?
		.enact()
		.map_err(|_| BenchmarkError::Skip)?;

		// ... then note them as unlockable with the NoteUnlockable instruction.
		let instruction = Instruction::NoteUnlockable { asset, owner };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		Ok(())
	}

	#[benchmark]
	fn request_unlock() -> Result<(), BenchmarkError> {
		use xcm_executor::traits::{AssetLock, Enact};

		let (locker, owner, asset) = T::unlockable_asset()?;

		// We first place the asset in lock first...
		<T::XcmConfig as xcm_executor::Config>::AssetLocker::prepare_lock(
			locker.clone(),
			asset.clone(),
			owner.clone(),
		)
		.map_err(|_| BenchmarkError::Skip)?
		.enact()
		.map_err(|_| BenchmarkError::Skip)?;

		let (expected_fees_mode, expected_assets_in_holding) =
			T::DeliveryHelper::ensure_successful_delivery(
				&owner,
				&locker,
				FeeReason::RequestUnlock,
			);
		let sender_account = T::AccountIdConverter::convert_location(&owner).unwrap();
		let sender_account_balance_before = T::TransactAsset::balance(&sender_account);

		// ... then request for an unlock with the RequestUnlock instruction.
		let mut executor = new_executor::<T>(owner);
		if let Some(expected_fees_mode) = expected_fees_mode {
			executor.set_fees_mode(expected_fees_mode);
		}
		if let Some(expected_assets_in_holding) = expected_assets_in_holding {
			executor.set_holding(expected_assets_in_holding.into());
		}
		let instruction = Instruction::RequestUnlock { asset, locker };
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		// Check we charged the delivery fees
		assert!(T::TransactAsset::balance(&sender_account) <= sender_account_balance_before);
		// TODO: Potentially add new trait to XcmSender to detect a queued outgoing message. #4426
		Ok(())
	}

	#[benchmark]
	fn unpaid_execution() -> Result<(), BenchmarkError> {
		let mut executor = new_executor::<T>(Default::default());
		executor.set_origin(Some(Here.into()));

		let instruction = Instruction::<XcmCallOf<T>>::UnpaidExecution {
			weight_limit: WeightLimit::Unlimited,
			check_origin: Some(Here.into()),
		};

		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		Ok(())
	}

	#[benchmark]
	fn alias_origin() -> Result<(), BenchmarkError> {
		let (origin, target) = T::alias_origin().map_err(|_| BenchmarkError::Skip)?;

		let mut executor = new_executor::<T>(origin);

		let instruction = Instruction::AliasOrigin(target.clone());
		let xcm = Xcm(vec![instruction]);
		#[block]
		{
			executor.bench_process(xcm)?;
		}
		assert_eq!(executor.origin(), &Some(target));
		Ok(())
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::generic::mock::new_test_ext(),
		crate::generic::mock::Test
	);
}
