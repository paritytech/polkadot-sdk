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

use crate::imports::*;

use codec::Encode;
use frame_support::{
	assert_err,
	dispatch::{DispatchErrorWithPostInfo, PostDispatchInfo},
	pallet_prelude::{DispatchError, Pays},
	sp_runtime::traits::Dispatchable,
};
use parachains_common::AccountId;
use people_rococo_runtime::people::IdentityInfo;
use rococo_runtime::governance::pallet_custom_origins::Origin::GeneralAdmin as GeneralAdminOrigin;
use rococo_system_emulated_network::people_rococo_emulated_chain::people_rococo_runtime;

use pallet_identity::Data;

use emulated_integration_tests_common::accounts::{ALICE, BOB};

#[test]
fn relay_commands_add_registrar() {
	let (origin_kind, origin) = (OriginKind::Superuser, <Rococo as Chain>::RuntimeOrigin::root());

	let registrar: AccountId = [1; 32].into();
	Rococo::execute_with(|| {
		type Runtime = <Rococo as Chain>::Runtime;
		type RuntimeCall = <Rococo as Chain>::RuntimeCall;
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
		type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
		type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

		let add_registrar_call =
			PeopleCall::Identity(pallet_identity::Call::<PeopleRuntime>::add_registrar {
				account: registrar.into(),
			});

		let xcm_message = RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
			dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
			message: bx!(VersionedXcm::from(Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				Transact { origin_kind, call: add_registrar_call.encode().into() }
			]))),
		});

		assert_ok!(xcm_message.dispatch(origin));

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::RegistrarAdded { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. }) => {},
			]
		);
	});
}

#[test]
fn relay_commands_add_registrar_wrong_origin() {
	let people_rococo_alice = PeopleRococo::account_id_of(ALICE);

	let origins = vec![
		(
			OriginKind::SovereignAccount,
			<Rococo as Chain>::RuntimeOrigin::signed(people_rococo_alice),
		),
		(OriginKind::Xcm, GeneralAdminOrigin.into()),
	];

	let mut signed_origin = true;

	for (origin_kind, origin) in origins {
		let registrar: AccountId = [1; 32].into();
		Rococo::execute_with(|| {
			type Runtime = <Rococo as Chain>::Runtime;
			type RuntimeCall = <Rococo as Chain>::RuntimeCall;
			type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
			type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
			type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

			let add_registrar_call =
				PeopleCall::Identity(pallet_identity::Call::<PeopleRuntime>::add_registrar {
					account: registrar.into(),
				});

			let xcm_message = RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
				dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
				message: bx!(VersionedXcm::from(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact { origin_kind, call: add_registrar_call.encode().into() }
				]))),
			});

			if signed_origin {
				assert_ok!(xcm_message.dispatch(origin));
				assert_expected_events!(
					Rococo,
					vec![
						RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
					]
				);
			} else {
				assert_err!(
					xcm_message.dispatch(origin),
					DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes },
						error: DispatchError::BadOrigin,
					},
				);
				assert_expected_events!(Rococo, vec![]);
			}
		});

		PeopleRococo::execute_with(|| {
			assert_expected_events!(PeopleRococo, vec![]);
		});

		signed_origin = false;
	}
}

#[test]
fn relay_commands_kill_identity() {
	// To kill an identity, first one must be set
	PeopleRococo::execute_with(|| {
		type PeopleRuntime = <PeopleRococo as Chain>::Runtime;
		type PeopleRuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		let people_rococo_alice =
			<PeopleRococo as Chain>::RuntimeOrigin::signed(PeopleRococo::account_id_of(ALICE));

		let identity_info = IdentityInfo {
			email: Data::Raw(b"test@test.io".to_vec().try_into().unwrap()),
			..Default::default()
		};
		let identity: Box<<PeopleRuntime as pallet_identity::Config>::IdentityInformation> =
			Box::new(identity_info);

		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
			people_rococo_alice,
			identity
		));

		assert_expected_events!(
			PeopleRococo,
			vec![
				PeopleRuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
			]
		);
	});

	let (origin_kind, origin) = (OriginKind::Superuser, <Rococo as Chain>::RuntimeOrigin::root());

	Rococo::execute_with(|| {
		type Runtime = <Rococo as Chain>::Runtime;
		type RuntimeCall = <Rococo as Chain>::RuntimeCall;
		type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
		type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

		let kill_identity_call =
			PeopleCall::Identity(pallet_identity::Call::<PeopleRuntime>::kill_identity {
				target: people_rococo_runtime::MultiAddress::Id(PeopleRococo::account_id_of(ALICE)),
			});

		let xcm_message = RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
			dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
			message: bx!(VersionedXcm::from(Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				Transact {
					origin_kind,
					// Making the weight's ref time any lower will prevent the XCM from triggering
					// execution of the intended extrinsic on the People chain - beware of spurious
					// test failure due to this.
					call: kill_identity_call.encode().into(),
				}
			]))),
		});

		assert_ok!(xcm_message.dispatch(origin));

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::IdentityKilled { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. }) => {},
			]
		);
	});
}

#[test]
fn relay_commands_kill_identity_wrong_origin() {
	let people_rococo_alice = PeopleRococo::account_id_of(BOB);

	let origins = vec![
		(
			OriginKind::SovereignAccount,
			<Rococo as Chain>::RuntimeOrigin::signed(people_rococo_alice),
		),
		(OriginKind::Xcm, GeneralAdminOrigin.into()),
	];

	let mut signed_origin: bool = true;

	for (origin_kind, origin) in origins {
		Rococo::execute_with(|| {
			type Runtime = <Rococo as Chain>::Runtime;
			type RuntimeCall = <Rococo as Chain>::RuntimeCall;
			type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
			type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
			type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

			let kill_identity_call =
				PeopleCall::Identity(pallet_identity::Call::<PeopleRuntime>::kill_identity {
					target: people_rococo_runtime::MultiAddress::Id(PeopleRococo::account_id_of(
						ALICE,
					)),
				});

			let xcm_message = RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
				dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
				message: bx!(VersionedXcm::from(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact { origin_kind, call: kill_identity_call.encode().into() }
				]))),
			});

			if signed_origin {
				assert_ok!(xcm_message.dispatch(origin));
				assert_expected_events!(
					Rococo,
					vec![
						RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
					]
				);
			} else {
				assert_err!(
					xcm_message.dispatch(origin),
					DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes },
						error: DispatchError::BadOrigin,
					},
				);
				assert_expected_events!(Rococo, vec![]);
			}
		});

		PeopleRococo::execute_with(|| {
			assert_expected_events!(PeopleRococo, vec![]);
		});

		signed_origin = false;
	}
}

#[test]
fn relay_commands_add_remove_username_authority() {
	let people_rococo_alice = PeopleRococo::account_id_of(ALICE);
	let people_rococo_bob = PeopleRococo::account_id_of(BOB);

	let origins = vec![
		//(OriginKind::Xcm, GeneralAdminOrigin.into(), "generaladmin"),
		(OriginKind::Superuser, <Rococo as Chain>::RuntimeOrigin::root(), "rootusername"),
	];
	for (origin_kind, origin, usr) in origins {
		// First, add a username authority.
		Rococo::execute_with(|| {
			type Runtime = <Rococo as Chain>::Runtime;
			type RuntimeCall = <Rococo as Chain>::RuntimeCall;
			type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
			type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
			type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

			let add_username_authority = PeopleCall::Identity(pallet_identity::Call::<
				PeopleRuntime,
			>::add_username_authority {
				authority: people_rococo_runtime::MultiAddress::Id(people_rococo_alice.clone()),
				suffix: b"suffix1".into(),
				allocation: 10,
			});

			let add_authority_xcm_msg = RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
				dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
				message: bx!(VersionedXcm::from(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact { origin_kind, call: add_username_authority.encode().into() }
				]))),
			});

			assert_ok!(add_authority_xcm_msg.dispatch(origin.clone()));

			assert_expected_events!(
				Rococo,
				vec![
					RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
				]
			);
		});

		// Check events system-parachain-side
		PeopleRococo::execute_with(|| {
			type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

			assert_expected_events!(
				PeopleRococo,
				vec![
					RuntimeEvent::Identity(pallet_identity::Event::AuthorityAdded { .. }) => {},
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. }) => {},
				]
			);
		});

		// Now, use the previously added username authority to concede a username to an account.
		PeopleRococo::execute_with(|| {
			type PeopleRuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
			let full_username = [usr.to_owned(), ".suffix1".to_owned()].concat().into_bytes();

			assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_username_for(
				<PeopleRococo as Chain>::RuntimeOrigin::signed(people_rococo_alice.clone()),
				people_rococo_runtime::MultiAddress::Id(people_rococo_bob.clone()),
				full_username,
				None,
				true
			));

			assert_expected_events!(
				PeopleRococo,
				vec![
					PeopleRuntimeEvent::Identity(pallet_identity::Event::UsernameQueued { .. }) => {},
				]
			);
		});

		// Accept the given username
		PeopleRococo::execute_with(|| {
			type PeopleRuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
			let full_username = [usr.to_owned(), ".suffix1".to_owned()].concat().into_bytes();

			assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::accept_username(
				<PeopleRococo as Chain>::RuntimeOrigin::signed(people_rococo_bob.clone()),
				full_username.try_into().unwrap(),
			));

			assert_expected_events!(
				PeopleRococo,
				vec![
					PeopleRuntimeEvent::Identity(pallet_identity::Event::UsernameSet { .. }) => {},
				]
			);
		});

		// Now, remove the username authority with another priviledged XCM call.
		Rococo::execute_with(|| {
			type Runtime = <Rococo as Chain>::Runtime;
			type RuntimeCall = <Rococo as Chain>::RuntimeCall;
			type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
			type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
			type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

			let remove_username_authority = PeopleCall::Identity(pallet_identity::Call::<
				PeopleRuntime,
			>::remove_username_authority {
				authority: people_rococo_runtime::MultiAddress::Id(people_rococo_alice.clone()),
				suffix: b"suffix1".into(),
			});

			let remove_authority_xcm_msg =
				RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
					dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
					message: bx!(VersionedXcm::from(Xcm(vec![
						UnpaidExecution { weight_limit: Unlimited, check_origin: None },
						Transact { origin_kind, call: remove_username_authority.encode().into() }
					]))),
				});

			assert_ok!(remove_authority_xcm_msg.dispatch(origin));

			assert_expected_events!(
				Rococo,
				vec![
					RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
				]
			);
		});

		// Final event check.
		PeopleRococo::execute_with(|| {
			type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

			assert_expected_events!(
				PeopleRococo,
				vec![
					RuntimeEvent::Identity(pallet_identity::Event::AuthorityRemoved { .. }) => {},
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. }) => {},
				]
			);
		});
	}
}

#[test]
fn relay_commands_add_remove_username_authority_wrong_origin() {
	let people_rococo_alice = PeopleRococo::account_id_of(ALICE);

	let origins = vec![
		(
			OriginKind::SovereignAccount,
			<Rococo as Chain>::RuntimeOrigin::signed(people_rococo_alice.clone()),
		),
		(OriginKind::Xcm, GeneralAdminOrigin.into()),
	];

	let mut signed_origin = true;

	for (origin_kind, origin) in origins {
		Rococo::execute_with(|| {
			type Runtime = <Rococo as Chain>::Runtime;
			type RuntimeCall = <Rococo as Chain>::RuntimeCall;
			type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
			type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
			type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

			let add_username_authority = PeopleCall::Identity(pallet_identity::Call::<
				PeopleRuntime,
			>::add_username_authority {
				authority: people_rococo_runtime::MultiAddress::Id(people_rococo_alice.clone()),
				suffix: b"suffix1".into(),
				allocation: 10,
			});

			let add_authority_xcm_msg = RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
				dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
				message: bx!(VersionedXcm::from(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact { origin_kind, call: add_username_authority.encode().into() }
				]))),
			});

			if signed_origin {
				assert_ok!(add_authority_xcm_msg.dispatch(origin.clone()));
				assert_expected_events!(
					Rococo,
					vec![
						RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
					]
				);
			} else {
				assert_err!(
					add_authority_xcm_msg.dispatch(origin.clone()),
					DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes },
						error: DispatchError::BadOrigin,
					},
				);
				assert_expected_events!(Rococo, vec![]);
			}
		});

		// Check events system-parachain-side
		PeopleRococo::execute_with(|| {
			assert_expected_events!(PeopleRococo, vec![]);
		});

		Rococo::execute_with(|| {
			type Runtime = <Rococo as Chain>::Runtime;
			type RuntimeCall = <Rococo as Chain>::RuntimeCall;
			type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
			type PeopleCall = <PeopleRococo as Chain>::RuntimeCall;
			type PeopleRuntime = <PeopleRococo as Chain>::Runtime;

			let remove_username_authority = PeopleCall::Identity(pallet_identity::Call::<
				PeopleRuntime,
			>::remove_username_authority {
				authority: people_rococo_runtime::MultiAddress::Id(people_rococo_alice.clone()),
				suffix: b"suffix1".into(),
			});

			let remove_authority_xcm_msg =
				RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
					dest: bx!(VersionedLocation::from(Location::new(0, [Parachain(1004)]))),
					message: bx!(VersionedXcm::from(Xcm(vec![
						UnpaidExecution { weight_limit: Unlimited, check_origin: None },
						Transact {
							origin_kind: OriginKind::SovereignAccount,
							call: remove_username_authority.encode().into(),
						}
					]))),
				});

			if signed_origin {
				assert_ok!(remove_authority_xcm_msg.dispatch(origin));
				assert_expected_events!(
					Rococo,
					vec![
						RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
					]
				);
			} else {
				assert_err!(
					remove_authority_xcm_msg.dispatch(origin),
					DispatchErrorWithPostInfo {
						post_info: PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes },
						error: DispatchError::BadOrigin,
					},
				);
				assert_expected_events!(Rococo, vec![]);
			}
		});

		PeopleRococo::execute_with(|| {
			assert_expected_events!(PeopleRococo, vec![]);
		});

		signed_origin = false;
	}
}
