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

pub use paste;

// Substrate
pub use pallet_balances;
pub use pallet_message_queue;
pub use pallet_xcm;

// Polkadot
pub use xcm::prelude::{AccountId32, WeightLimit};

// Cumulus
pub use asset_test_utils;
pub use cumulus_pallet_xcmp_queue;
pub use xcm_emulator::Chain;

#[macro_export]
macro_rules! test_parachain_is_trusted_teleporter {
	( $sender_para:ty, $sender_xcm_config:ty, vec![$( $receiver_para:ty ),+], ($assets:expr, $amount:expr) ) => {
		$crate::macros::paste::paste! {
			// init Origin variables
			let sender = [<$sender_para Sender>]::get();
			let mut para_sender_balance_before =
				<$sender_para as $crate::macros::Chain>::account_data_of(sender.clone()).free;
			let origin = <$sender_para as $crate::macros::Chain>::RuntimeOrigin::signed(sender.clone());
			let fee_asset_item = 0;
			let weight_limit = $crate::macros::WeightLimit::Unlimited;

			$(
				{
					// init Destination variables
					let receiver = [<$receiver_para Receiver>]::get();
					let para_receiver_balance_before =
						<$receiver_para as $crate::macros::Chain>::account_data_of(receiver.clone()).free;
					let para_destination =
						<$sender_para>::sibling_location_of(<$receiver_para>::para_id());
					let beneficiary: Location =
						$crate::macros::AccountId32 { network: None, id: receiver.clone().into() }.into();

					// Send XCM message from Origin Parachain
					// We are only testing the limited teleport version, which should be ok since success will
					// depend only on a proper `XcmConfig` at destination.
					<$sender_para>::execute_with(|| {
						assert_ok!(<$sender_para as [<$sender_para Pallet>]>::PolkadotXcm::limited_teleport_assets(
							origin.clone(),
							bx!(para_destination.clone().into()),
							bx!(beneficiary.clone().into()),
							bx!($assets.clone().into()),
							fee_asset_item,
							weight_limit.clone(),
						));

						type RuntimeEvent = <$sender_para as $crate::macros::Chain>::RuntimeEvent;

						assert_expected_events!(
							$sender_para,
							vec![
								RuntimeEvent::PolkadotXcm(
									$crate::macros::pallet_xcm::Event::Attempted { outcome: Outcome::Complete { .. } }
								) => {},
								RuntimeEvent::XcmpQueue(
									$crate::macros::cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
								) => {},
								RuntimeEvent::Balances(
									$crate::macros::pallet_balances::Event::Burned { who: sender, amount }
								) => {},
							]
						);
					});

					// Receive XCM message in Destination Parachain
					<$receiver_para>::execute_with(|| {
						type RuntimeEvent = <$receiver_para as $crate::macros::Chain>::RuntimeEvent;

						assert_expected_events!(
							$receiver_para,
							vec![
								RuntimeEvent::Balances(
									$crate::macros::pallet_balances::Event::Minted { who: receiver, .. }
								) => {},
								RuntimeEvent::MessageQueue(
									$crate::macros::pallet_message_queue::Event::Processed { success: true, .. }
								) => {},
							]
						);
					});

					// Check if balances are updated accordingly in Origin and Destination Parachains
					let para_sender_balance_after =
						<$sender_para as $crate::macros::Chain>::account_data_of(sender.clone()).free;
					let para_receiver_balance_after =
						<$receiver_para as $crate::macros::Chain>::account_data_of(receiver.clone()).free;
					let delivery_fees = <$sender_para>::execute_with(|| {
						$crate::macros::asset_test_utils::xcm_helpers::transfer_assets_delivery_fees::<
							<$sender_xcm_config as xcm_executor::Config>::XcmSender,
						>($assets.clone(), fee_asset_item, weight_limit.clone(), beneficiary, para_destination)
					});

					assert_eq!(para_sender_balance_before - $amount - delivery_fees, para_sender_balance_after);
					assert!(para_receiver_balance_after > para_receiver_balance_before);

					// Update sender balance
					para_sender_balance_before = <$sender_para as $crate::macros::Chain>::account_data_of(sender.clone()).free;
				}
			)+
		}
	};
}

#[macro_export]
macro_rules! include_penpal_create_foreign_asset_on_asset_hub {
	( $penpal:ident, $asset_hub:ident, $relay_ed:expr, $weight_to_fee:expr) => {
		$crate::impls::paste::paste! {
			pub fn penpal_create_foreign_asset_on_asset_hub(
				asset_id_on_penpal: u32,
				foreign_asset_at_asset_hub: v3::Location,
				ah_as_seen_by_penpal: Location,
				is_sufficient: bool,
				asset_owner: AccountId,
				prefund_amount: u128,
			) {
				use frame_support::weights::WeightToFee;
				let ah_check_account = $asset_hub::execute_with(|| {
					<$asset_hub as [<$asset_hub Pallet>]>::PolkadotXcm::check_account()
				});
				let penpal_check_account =
					$penpal::execute_with(|| <$penpal as [<$penpal Pallet>]>::PolkadotXcm::check_account());
				let penpal_as_seen_by_ah = $asset_hub::sibling_location_of($penpal::para_id());

				// prefund SA of Penpal on AssetHub with enough native tokens to pay for creating
				// new foreign asset, also prefund CheckingAccount with ED, because teleported asset
				// itself might not be sufficient and CheckingAccount cannot be created otherwise
				let sov_penpal_on_ah = $asset_hub::sovereign_account_id_of(penpal_as_seen_by_ah.clone());
				$asset_hub::fund_accounts(vec![
					(sov_penpal_on_ah.clone().into(), $relay_ed * 100_000_000_000),
					(ah_check_account.clone().into(), $relay_ed * 1000),
				]);

				// prefund SA of AssetHub on Penpal with native asset
				let sov_ah_on_penpal = $penpal::sovereign_account_id_of(ah_as_seen_by_penpal.clone());
				$penpal::fund_accounts(vec![
					(sov_ah_on_penpal.into(), $relay_ed * 1_000_000_000),
					(penpal_check_account.clone().into(), $relay_ed * 1000),
				]);

				// Force create asset on $penpal and prefund [<$penpal Sender>]
				$penpal::force_create_and_mint_asset(
					asset_id_on_penpal,
					ASSET_MIN_BALANCE,
					is_sufficient,
					asset_owner,
					None,
					prefund_amount,
				);

				let require_weight_at_most = Weight::from_parts(1_100_000_000_000, 30_000);
				// `OriginKind::Xcm` required by ForeignCreators pallet-assets origin filter
				let origin_kind = OriginKind::Xcm;
				let call_create_foreign_assets =
					<$asset_hub as Chain>::RuntimeCall::ForeignAssets(pallet_assets::Call::<
						<$asset_hub as Chain>::Runtime,
						pallet_assets::Instance2,
					>::create {
						id: foreign_asset_at_asset_hub,
						min_balance: ASSET_MIN_BALANCE,
						admin: sov_penpal_on_ah.into(),
					})
					.encode();
				let buy_execution_fee_amount = $weight_to_fee::weight_to_fee(
					&Weight::from_parts(10_100_000_000_000, 300_000),
				);
				let buy_execution_fee = Asset {
					id: AssetId(Location { parents: 1, interior: Here }),
					fun: Fungible(buy_execution_fee_amount),
				};
				let xcm = VersionedXcm::from(Xcm(vec![
					WithdrawAsset { 0: vec![buy_execution_fee.clone()].into() },
					BuyExecution { fees: buy_execution_fee.clone(), weight_limit: Unlimited },
					Transact { require_weight_at_most, origin_kind, call: call_create_foreign_assets.into() },
					ExpectTransactStatus(MaybeErrorCode::Success),
					RefundSurplus,
					DepositAsset { assets: All.into(), beneficiary: penpal_as_seen_by_ah },
				]));
				// Send XCM message from penpal => asset_hub
				let sudo_penpal_origin = <$penpal as Chain>::RuntimeOrigin::root();
				$penpal::execute_with(|| {
					assert_ok!(<$penpal as [<$penpal Pallet>]>::PolkadotXcm::send(
						sudo_penpal_origin.clone(),
						bx!(ah_as_seen_by_penpal.into()),
						bx!(xcm),
					));
					type RuntimeEvent = <$penpal as Chain>::RuntimeEvent;
					assert_expected_events!(
						$penpal,
						vec![
							RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
						]
					);
				});
				$asset_hub::execute_with(|| {
					type ForeignAssets = <$asset_hub as [<$asset_hub Pallet>]>::ForeignAssets;
					assert!(ForeignAssets::asset_exists(foreign_asset_at_asset_hub));
				});
			}
		}
	};
}
