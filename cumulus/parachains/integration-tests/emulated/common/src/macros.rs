// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#[macro_export]
macro_rules! test_parachain_is_trusted_teleporter {
	( $sender_para:ty, vec![$( $receiver_para:ty ),+], ($assets:expr, $amount:expr) ) => {
		$crate::paste::paste! {
			// init Origin variables
			let sender = [<$sender_para Sender>]::get();
			let mut para_sender_balance_before =
				<$sender_para as $crate::Chain>::account_data_of(sender.clone()).free;
			let origin = <$sender_para as $crate::Chain>::RuntimeOrigin::signed(sender.clone());
			let fee_asset_item = 0;
			let weight_limit = $crate::WeightLimit::Unlimited;

			$(
				{
					// init Destination variables
					let receiver = [<$receiver_para Receiver>]::get();
					let para_receiver_balance_before =
						<$receiver_para as $crate::Chain>::account_data_of(receiver.clone()).free;
					let para_destination =
						<$sender_para>::sibling_location_of(<$receiver_para>::para_id()).into();
					let beneficiary =
						$crate::AccountId32 { network: None, id: receiver.clone().into() }.into();

					// Send XCM message from Origin Parachain
					<$sender_para>::execute_with(|| {
						assert_ok!(<$sender_para as [<$sender_para Pallet>]>::PolkadotXcm::limited_teleport_assets(
							origin.clone(),
							bx!(para_destination),
							bx!(beneficiary),
							bx!($assets.clone()),
							fee_asset_item,
							weight_limit.clone(),
						));

						type RuntimeEvent = <$sender_para as $crate::Chain>::RuntimeEvent;

						assert_expected_events!(
							$sender_para,
							vec![
								RuntimeEvent::PolkadotXcm(
									$crate::pallet_xcm::Event::Attempted { outcome: Outcome::Complete { .. } }
								) => {},
								RuntimeEvent::XcmpQueue(
									$crate::cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
								) => {},
								RuntimeEvent::Balances(
									$crate::pallet_balances::Event::Withdraw { who: sender, amount }
								) => {},
							]
						);
					});

					// Receive XCM message in Destination Parachain
					<$receiver_para>::execute_with(|| {
						type RuntimeEvent = <$receiver_para as $crate::Chain>::RuntimeEvent;

						assert_expected_events!(
							$receiver_para,
							vec![
								RuntimeEvent::Balances(
									$crate::pallet_balances::Event::Deposit { who: receiver, .. }
								) => {},
								RuntimeEvent::XcmpQueue(
									$crate::cumulus_pallet_xcmp_queue::Event::Success { .. }
								) => {},
							]
						);
					});

					// Check if balances are updated accordingly in Origin and Destination Parachains
					let para_sender_balance_after =
						<$sender_para as $crate::Chain>::account_data_of(sender.clone()).free;
					let para_receiver_balance_after =
						<$receiver_para as $crate::Chain>::account_data_of(receiver.clone()).free;

					assert_eq!(para_sender_balance_before - $amount, para_sender_balance_after);
					assert!(para_receiver_balance_after > para_receiver_balance_before);

					// Update sender balance
					para_sender_balance_before = <$sender_para as $crate::Chain>::account_data_of(sender.clone()).free;
				}
			)+
		}
	};
}
