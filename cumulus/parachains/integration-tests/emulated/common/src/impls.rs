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

pub use codec::{Decode, Encode};
pub use paste;

pub use crate::{
	constants::{PROOF_SIZE_THRESHOLD, REF_TIME_THRESHOLD},
	xcm_helpers::xcm_transact_unpaid_execution,
	BridgeHubRococo, BridgeHubWococo,
};

// Substrate
pub use frame_support::{assert_ok, traits::fungibles::Inspect};
pub use pallet_assets;
pub use pallet_message_queue;
use sp_core::Get;

// Cumulus
use bp_messages::{
	target_chain::{DispatchMessage, DispatchMessageData, MessageDispatch},
	LaneId, MessageKey, OutboundLaneData,
};
use bridge_runtime_common::messages_xcm_extension::XcmBlobMessageDispatchResult;
pub use cumulus_pallet_dmp_queue;
pub use cumulus_pallet_parachain_system;
pub use cumulus_pallet_xcmp_queue;
pub use cumulus_primitives_core::{
	relay_chain::HrmpChannelId, DmpMessageHandler, ParaId, XcmpMessageHandler,
};
use pallet_bridge_messages::{Config, Instance1, Instance2, OutboundLanes, Pallet};
pub use parachains_common::{AccountId, Balance};
pub use xcm_emulator::{
	assert_expected_events, bx, helpers::weight_within_threshold, BridgeMessage,
	BridgeMessageDispatchError, BridgeMessageHandler, Chain, Parachain, RelayChain, TestExt,
};

// Polkadot
pub use pallet_xcm;
pub use polkadot_runtime_parachains::{
	dmp, hrmp,
	inclusion::{AggregateMessageOrigin, UmpQueueId},
};
pub use xcm::{
	prelude::{OriginKind, Outcome, VersionedXcm, Weight},
	v3::Error,
	DoubleEncoded,
};

pub struct BridgeHubMessageHandler<S, T, I> {
	_marker: std::marker::PhantomData<(S, T, I)>,
}

struct LaneIdWrapper(LaneId);

impl From<LaneIdWrapper> for u32 {
	fn from(lane_id: LaneIdWrapper) -> u32 {
		u32::from_be_bytes(lane_id.0 .0)
	}
}

impl From<u32> for LaneIdWrapper {
	fn from(id: u32) -> LaneIdWrapper {
		LaneIdWrapper(LaneId(id.to_be_bytes()))
	}
}

type BridgeHubRococoRuntime = <BridgeHubRococo as Chain>::Runtime;
type BridgeHubWococoRuntime = <BridgeHubWococo as Chain>::Runtime;

// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
// type BridgeHubPolkadotRuntime = <BridgeHubPolkadot as Chain>::Runtime;
// type BridgeHubKusamaRuntime = <BridgeHubKusama as Chain>::Runtime;

pub type RococoWococoMessageHandler =
	BridgeHubMessageHandler<BridgeHubRococoRuntime, BridgeHubWococoRuntime, Instance2>;
pub type WococoRococoMessageHandler =
	BridgeHubMessageHandler<BridgeHubWococoRuntime, BridgeHubRococoRuntime, Instance2>;

// TODO: uncomment when https://github.com/paritytech/cumulus/pull/2528 is merged
// pub type PolkadotKusamaMessageHandler
//	= BridgeHubMessageHandler<BridgeHubPolkadotRuntime, BridgeHubKusamaRuntime, Instance1>;
// pub type KusamaPolkadotMessageHandler
//	= BridgeHubMessageHandler<BridgeHubKusamaRuntime, BridgeHubPolkadoRuntime, Instance1>;

impl<S, T, I> BridgeMessageHandler for BridgeHubMessageHandler<S, T, I>
where
	S: Config<Instance1>,
	T: Config<I>,
	I: 'static,
	<T as Config<I>>::InboundPayload: From<Vec<u8>>,
	<T as Config<I>>::MessageDispatch:
		MessageDispatch<DispatchLevelResult = XcmBlobMessageDispatchResult>,
{
	fn get_source_outbound_messages() -> Vec<BridgeMessage> {
		// get the source active outbound lanes
		let active_lanes = S::ActiveOutboundLanes::get();

		let mut messages: Vec<BridgeMessage> = Default::default();

		// collect messages from `OutboundMessages` for each active outbound lane in the source
		for lane in active_lanes {
			let latest_generated_nonce =
				OutboundLanes::<S, Instance1>::get(lane).latest_generated_nonce;
			let latest_received_nonce =
				OutboundLanes::<S, Instance1>::get(lane).latest_received_nonce;

			(latest_received_nonce + 1..=latest_generated_nonce).for_each(|nonce| {
				let encoded_payload: Vec<u8> =
					Pallet::<S, Instance1>::outbound_message_data(*lane, nonce)
						.expect("Bridge message does not exist")
						.into();
				let payload = Vec::<u8>::decode(&mut &encoded_payload[..])
					.expect("Decodign XCM message failed");
				let id: u32 = LaneIdWrapper(*lane).into();
				let message = BridgeMessage { id, nonce, payload };

				messages.push(message);
			});
		}
		messages
	}

	fn dispatch_target_inbound_message(
		message: BridgeMessage,
	) -> Result<(), BridgeMessageDispatchError> {
		type TargetMessageDispatch<T, I> = <T as Config<I>>::MessageDispatch;
		type InboundPayload<T, I> = <T as Config<I>>::InboundPayload;

		let lane_id = LaneIdWrapper::from(message.id).0;
		let nonce = message.nonce;
		let payload = Ok(From::from(message.payload));

		// Directly dispatch outbound messages assuming everything is correct
		// and bypassing the `Relayers`  and `InboundLane` logic
		let dispatch_result = TargetMessageDispatch::<T, I>::dispatch(DispatchMessage {
			key: MessageKey { lane_id, nonce },
			data: DispatchMessageData::<InboundPayload<T, I>> { payload },
		});

		let result = match dispatch_result.dispatch_level_result {
			XcmBlobMessageDispatchResult::Dispatched => Ok(()),
			XcmBlobMessageDispatchResult::InvalidPayload => Err(BridgeMessageDispatchError(
				Box::new(XcmBlobMessageDispatchResult::InvalidPayload),
			)),
			XcmBlobMessageDispatchResult::NotDispatched(e) => Err(BridgeMessageDispatchError(
				Box::new(XcmBlobMessageDispatchResult::NotDispatched(e)),
			)),
		};
		result
	}

	fn notify_source_message_delivery(lane_id: u32) {
		let data = OutboundLanes::<S, Instance1>::get(LaneIdWrapper::from(lane_id).0);
		let new_data = OutboundLaneData {
			oldest_unpruned_nonce: data.oldest_unpruned_nonce + 1,
			latest_received_nonce: data.latest_received_nonce + 1,
			..data
		};

		OutboundLanes::<S, Instance1>::insert(LaneIdWrapper::from(lane_id).0, new_data);
	}
}

#[macro_export]
macro_rules! impl_accounts_helpers_for_relay_chain {
	( $chain:ident ) => {
		$crate::impls::paste::paste! {
			impl $chain {
				/// Fund a set of accounts with a balance
				pub fn fund_accounts(accounts: Vec<($crate::impls::AccountId, $crate::impls::Balance)>) {
					<Self as $crate::impls::TestExt>::execute_with(|| {
						for account in accounts {
							$crate::impls::assert_ok!(<Self as [<$chain Pallet>]>::Balances::force_set_balance(
								<Self as $crate::impls::Chain>::RuntimeOrigin::root(),
								account.0.into(),
								account.1,
							));
						}
					});
				}
				/// Fund a sovereign account based on its Parachain Id
				pub fn fund_para_sovereign(amount: $crate::impls::Balance, para_id: $crate::impls::ParaId) -> sp_runtime::AccountId32 {
					let sovereign_account = <Self as $crate::impls::RelayChain>::sovereign_account_id_of_child_para(para_id);
					Self::fund_accounts(vec![(sovereign_account.clone(), amount)]);
					sovereign_account
				}
			}
		}
	};
}

#[macro_export]
macro_rules! impl_assert_events_helpers_for_relay_chain {
	( $chain:ident ) => {
		$crate::impls::paste::paste! {
			type [<$chain RuntimeEvent>] = <$chain as $crate::impls::Chain>::RuntimeEvent;

			impl $chain {
				/// Asserts a dispatchable is completely executed and XCM sent
				pub fn assert_xcm_pallet_attempted_complete(expected_weight: Option<$crate::impls::Weight>) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::XcmPallet(
								$crate::impls::pallet_xcm::Event::Attempted { outcome: $crate::impls::Outcome::Complete(weight) }
							) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
							},
						]
					);
				}

				/// Asserts a dispatchable is incompletely executed and XCM sent
				pub fn assert_xcm_pallet_attempted_incomplete(
					expected_weight: Option<$crate::impls::Weight>,
					expected_error: Option<$crate::impls::Error>,
				) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							// Dispatchable is properly executed and XCM message sent
							[<$chain RuntimeEvent>]::XcmPallet(
								$crate::impls::pallet_xcm::Event::Attempted { outcome: $crate::impls::Outcome::Incomplete(weight, error) }
							) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
								error: *error == expected_error.unwrap_or(*error),
							},
						]
					);
				}

				/// Asserts a XCM message is sent
				pub fn assert_xcm_pallet_sent() {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::XcmPallet($crate::impls::pallet_xcm::Event::Sent { .. }) => {},
						]
					);
				}

				/// Asserts a XCM from System Parachain is succesfully received and proccessed
				pub fn assert_ump_queue_processed(
					expected_success: bool,
					expected_id: Option<$crate::impls::ParaId>,
					expected_weight: Option<$crate::impls::Weight>,
				) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							// XCM is succesfully received and proccessed
							[<$chain RuntimeEvent>]::MessageQueue($crate::impls::pallet_message_queue::Event::Processed {
								origin: $crate::impls::AggregateMessageOrigin::Ump($crate::impls::UmpQueueId::Para(id)),
								weight_used,
								success,
								..
							}) => {
								id: *id == expected_id.unwrap_or(*id),
								weight_used: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight_used),
									*weight_used
								),
								success: *success == expected_success,
							},
						]
					);
				}
			}
		}
	};
}

#[macro_export]
macro_rules! impl_hrmp_channels_helpers_for_relay_chain {
	( $chain:ident ) => {
		$crate::impls::paste::paste! {
			impl $chain {
				/// Init open channel request with another Parachain
				pub fn init_open_channel_call(
					recipient_para_id: $crate::impls::ParaId,
					max_capacity: u32,
					max_message_size: u32,
				) -> $crate::impls::DoubleEncoded<()> {
					use $crate::impls::Encode;

					<Self as $crate::impls::Chain>::RuntimeCall::Hrmp($crate::impls::hrmp::Call::<
						<Self as $crate::impls::Chain>::Runtime,
					>::hrmp_init_open_channel {
						recipient: recipient_para_id,
						proposed_max_capacity: max_capacity,
						proposed_max_message_size: max_message_size,
					})
					.encode()
					.into()
				}
				/// Recipient Parachain accept the open request from another Parachain
				pub fn accept_open_channel_call(sender_para_id: $crate::impls::ParaId) -> $crate::impls::DoubleEncoded<()> {
					use $crate::impls::Encode;

					<Self as $crate::impls::Chain>::RuntimeCall::Hrmp($crate::impls::hrmp::Call::<
						<Self as $crate::impls::Chain>::Runtime,
					>::hrmp_accept_open_channel {
						sender: sender_para_id,
					})
					.encode()
					.into()
				}

				/// A root origin force to open a channel between two Parachains
				pub fn force_process_hrmp_open(sender: $crate::impls::ParaId, recipient: $crate::impls::ParaId) {
					use $crate::impls::Chain;

					<Self as $crate::impls::TestExt>::execute_with(|| {
						let relay_root_origin = <Self as Chain>::RuntimeOrigin::root();

						// Force process HRMP open channel requests without waiting for the next session
						$crate::impls::assert_ok!(<Self as [<$chain Pallet>]>::Hrmp::force_process_hrmp_open(
							relay_root_origin,
							0
						));

						let channel_id = $crate::impls::HrmpChannelId { sender, recipient };

						let hrmp_channel_exist = $crate::impls::hrmp::HrmpChannels::<
							<Self as Chain>::Runtime,
						>::contains_key(&channel_id);

						// Check the HRMP channel has been successfully registrered
						assert!(hrmp_channel_exist)
					});
				}
			}
		}
	};
}

#[macro_export]
macro_rules! impl_accounts_helpers_for_parachain {
	( $chain:ident ) => {
		$crate::impls::paste::paste! {
			impl $chain {
				/// Fund a set of accounts with a balance
				pub fn fund_accounts(accounts: Vec<($crate::impls::AccountId, $crate::impls::Balance)>) {
					<Self as $crate::impls::TestExt>::execute_with(|| {
						for account in accounts {
							$crate::impls::assert_ok!(<Self as [<$chain Pallet>]>::Balances::force_set_balance(
								<Self as $crate::impls::Chain>::RuntimeOrigin::root(),
								account.0.into(),
								account.1,
							));
						}
					});
				}
			}
		}
	};
}

#[macro_export]
macro_rules! impl_assert_events_helpers_for_parachain {
	( $chain:ident ) => {
		$crate::impls::paste::paste! {
			type [<$chain RuntimeEvent>] = <$chain as $crate::impls::Chain>::RuntimeEvent;

			impl $chain {
				/// Asserts a dispatchable is completely executed and XCM sent
				pub fn assert_xcm_pallet_attempted_complete(expected_weight: Option<$crate::impls::Weight>) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::PolkadotXcm(
								$crate::impls::pallet_xcm::Event::Attempted { outcome: $crate::impls::Outcome::Complete(weight) }
							) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
							},
						]
					);
				}

				/// Asserts a dispatchable is incompletely executed and XCM sent
				pub fn assert_xcm_pallet_attempted_incomplete(
					expected_weight: Option<$crate::impls::Weight>,
					expected_error: Option<$crate::impls::Error>,
				) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							// Dispatchable is properly executed and XCM message sent
							[<$chain RuntimeEvent>]::PolkadotXcm(
								$crate::impls::pallet_xcm::Event::Attempted { outcome: $crate::impls::Outcome::Incomplete(weight, error) }
							) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
								error: *error == expected_error.unwrap_or(*error),
							},
						]
					);
				}

				/// Asserts a dispatchable throws and error when trying to be sent
				pub fn assert_xcm_pallet_attempted_error(expected_error: Option<$crate::impls::Error>) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							// Execution fails in the origin with `Barrier`
							[<$chain RuntimeEvent>]::PolkadotXcm(
								$crate::impls::pallet_xcm::Event::Attempted { outcome: $crate::impls::Outcome::Error(error) }
							) => {
								error: *error == expected_error.unwrap_or(*error),
							},
						]
					);
				}

				/// Asserts a XCM message is sent
				pub fn assert_xcm_pallet_sent() {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::PolkadotXcm($crate::impls::pallet_xcm::Event::Sent { .. }) => {},
						]
					);
				}

				/// Asserts a XCM message is sent to Relay Chain
				pub fn assert_parachain_system_ump_sent() {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::ParachainSystem(
								$crate::impls::cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }
							) => {},
						]
					);
				}

				/// Asserts a XCM from Relay Chain is completely executed
				pub fn assert_dmp_queue_complete(expected_weight: Option<$crate::impls::Weight>) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::DmpQueue($crate::impls::cumulus_pallet_dmp_queue::Event::ExecutedDownward {
								outcome: $crate::impls::Outcome::Complete(weight), ..
							}) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
							},
						]
					);
				}

				/// Asserts a XCM from Relay Chain is incompletely executed
				pub fn assert_dmp_queue_incomplete(
					expected_weight: Option<$crate::impls::Weight>,
					expected_error: Option<$crate::impls::Error>,
				) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::DmpQueue($crate::impls::cumulus_pallet_dmp_queue::Event::ExecutedDownward {
								outcome: $crate::impls::Outcome::Incomplete(weight, error), ..
							}) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
								error: *error == expected_error.unwrap_or(*error),
							},
						]
					);
				}

				/// Asserts a XCM from another Parachain is completely executed
				pub fn assert_xcmp_queue_success(expected_weight: Option<$crate::impls::Weight>) {
					$crate::impls::assert_expected_events!(
						Self,
						vec![
							[<$chain RuntimeEvent>]::XcmpQueue(
								$crate::impls::cumulus_pallet_xcmp_queue::Event::Success { weight, .. }
							) => {
								weight: $crate::impls::weight_within_threshold(
									($crate::impls::REF_TIME_THRESHOLD, $crate::impls::PROOF_SIZE_THRESHOLD),
									expected_weight.unwrap_or(*weight),
									*weight
								),
							},
						]
					);
				}
			}
		}
	};
}

#[macro_export]
macro_rules! impl_assets_helpers_for_parachain {
	( $chain:ident, $relay_chain:ident ) => {
		$crate::impls::paste::paste! {
			impl $chain {
				/// Returns the encoded call for `force_create` from the assets pallet
				pub fn force_create_asset_call(
					asset_id: u32,
					owner: $crate::impls::AccountId,
					is_sufficient: bool,
					min_balance: $crate::impls::Balance,
				) -> $crate::impls::DoubleEncoded<()> {
					use $crate::impls::{Chain, Encode};

					<Self as Chain>::RuntimeCall::Assets($crate::impls::pallet_assets::Call::<
						<Self as Chain>::Runtime,
						$crate::impls::pallet_assets::Instance1,
					>::force_create {
						id: asset_id.into(),
						owner: owner.into(),
						is_sufficient,
						min_balance,
					})
					.encode()
					.into()
				}

				/// Returns a `VersionedXcm` for `force_create` from the assets pallet
				pub fn force_create_asset_xcm(
					origin_kind: $crate::impls::OriginKind,
					asset_id: u32,
					owner: $crate::impls::AccountId,
					is_sufficient: bool,
					min_balance: $crate::impls::Balance,
				) -> $crate::impls::VersionedXcm<()> {
					let call = Self::force_create_asset_call(asset_id, owner, is_sufficient, min_balance);
					$crate::impls::xcm_transact_unpaid_execution(call, origin_kind)
				}

				/// Mint assets making use of the assets pallet
				pub fn mint_asset(
					signed_origin: <Self as $crate::impls::Chain>::RuntimeOrigin,
					id: u32,
					beneficiary: $crate::impls::AccountId,
					amount_to_mint: u128,
				) {
					<Self as $crate::impls::TestExt>::execute_with(|| {
						$crate::impls::assert_ok!(<Self as [<$chain Pallet>]>::Assets::mint(
							signed_origin,
							id.into(),
							beneficiary.clone().into(),
							amount_to_mint
						));

						type RuntimeEvent = <$chain as $crate::impls::Chain>::RuntimeEvent;

						$crate::impls::assert_expected_events!(
							Self,
							vec![
								RuntimeEvent::Assets($crate::impls::pallet_assets::Event::Issued { asset_id, owner, amount }) => {
									asset_id: *asset_id == id,
									owner: *owner == beneficiary.clone().into(),
									amount: *amount == amount_to_mint,
								},
							]
						);
					});
				}

				/// Force create and mint assets making use of the assets pallet
				pub fn force_create_and_mint_asset(
					id: u32,
					min_balance: u128,
					is_sufficient: bool,
					asset_owner: $crate::impls::AccountId,
					amount_to_mint: u128,
				) {
					use $crate::impls::{bx, Chain, RelayChain, Parachain, Inspect, TestExt};
					// Init values for Relay Chain
					let root_origin = <$relay_chain as Chain>::RuntimeOrigin::root();
					let destination = <$relay_chain>::child_location_of(<$chain>::para_id());
					let xcm = Self::force_create_asset_xcm(
						$crate::impls::OriginKind::Superuser,
						id,
						asset_owner.clone(),
						is_sufficient,
						min_balance,
					);

					<$relay_chain>::execute_with(|| {
						$crate::impls::assert_ok!(<$relay_chain as [<$relay_chain Pallet>]>::XcmPallet::send(
							root_origin,
							bx!(destination.into()),
							bx!(xcm),
						));

						<$relay_chain>::assert_xcm_pallet_sent();
					});

					Self::execute_with(|| {
						Self::assert_dmp_queue_complete(Some($crate::impls::Weight::from_parts(1_019_445_000, 200_000)));

						type RuntimeEvent = <$chain as $crate::impls::Chain>::RuntimeEvent;

						$crate::impls::assert_expected_events!(
							Self,
							vec![
								// Asset has been created
								RuntimeEvent::Assets($crate::impls::pallet_assets::Event::ForceCreated { asset_id, owner }) => {
									asset_id: *asset_id == id,
									owner: *owner == asset_owner.clone(),
								},
							]
						);

						assert!(<Self as [<$chain Pallet>]>::Assets::asset_exists(id.into()));
					});

					let signed_origin = <Self as Chain>::RuntimeOrigin::signed(asset_owner.clone());

					// Mint asset for System Parachain's sender
					Self::mint_asset(signed_origin, id, asset_owner, amount_to_mint);
				}
			}
		}
	};
}
