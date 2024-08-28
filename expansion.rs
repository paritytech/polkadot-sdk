#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
#[cfg(test)]
mod imports {
    pub use codec::Encode;
    pub use frame_support::{
        assert_err, assert_ok, pallet_prelude::Weight,
        sp_runtime::{DispatchError, DispatchResult, ModuleError},
        traits::fungibles::Inspect,
    };
    pub use xcm::prelude::{AccountId32 as AccountId32Junction, *};
    pub use xcm_executor::traits::TransferType;
    pub use asset_test_utils::xcm_helpers;
    pub use emulated_integration_tests_common::{
        accounts::DUMMY_EMPTY, get_account_id_from_seed,
        test_parachain_is_trusted_teleporter,
        test_parachain_is_trusted_teleporter_for_relay, test_relay_is_trusted_teleporter,
        xcm_emulator::{
            assert_expected_events, bx, Chain, Parachain as Para, RelayChain as Relay,
            Test, TestArgs, TestContext, TestExt,
        },
        xcm_helpers::{
            get_amount_from_versioned_assets, non_fee_asset, xcm_transact_paid_execution,
        },
        ASSETS_PALLET_ID, RESERVABLE_ASSET_ID, XCM_V3,
    };
    pub use parachains_common::{AccountId, Balance};
    pub use westend_system_emulated_network::{
        asset_hub_westend_emulated_chain::{
            asset_hub_westend_runtime::{
                xcm_config::{
                    self as ahw_xcm_config, WestendLocation as RelayLocation,
                    XcmConfig as AssetHubWestendXcmConfig,
                },
                AssetConversionOrigin as AssetHubWestendAssetConversionOrigin,
                ExistentialDeposit as AssetHubWestendExistentialDeposit,
            },
            genesis::{AssetHubWestendAssetOwner, ED as ASSET_HUB_WESTEND_ED},
            AssetHubWestendParaPallet as AssetHubWestendPallet,
        },
        collectives_westend_emulated_chain::CollectivesWestendParaPallet as CollectivesWestendPallet,
        penpal_emulated_chain::{
            penpal_runtime::xcm_config::{
                CustomizableAssetFromSystemAssetHub as PenpalCustomizableAssetFromSystemAssetHub,
                LocalReservableFromAssetHub as PenpalLocalReservableFromAssetHub,
                LocalTeleportableToAssetHub as PenpalLocalTeleportableToAssetHub,
            },
            PenpalAParaPallet as PenpalAPallet, PenpalAssetOwner,
            PenpalBParaPallet as PenpalBPallet,
        },
        westend_emulated_chain::{
            genesis::ED as WESTEND_ED,
            westend_runtime::xcm_config::{
                UniversalLocation as WestendUniversalLocation,
                XcmConfig as WestendXcmConfig,
            },
            WestendRelayPallet as WestendPallet,
        },
        AssetHubWestendPara as AssetHubWestend,
        AssetHubWestendParaReceiver as AssetHubWestendReceiver,
        AssetHubWestendParaSender as AssetHubWestendSender,
        BridgeHubWestendPara as BridgeHubWestend,
        BridgeHubWestendParaReceiver as BridgeHubWestendReceiver,
        CollectivesWestendPara as CollectivesWestend, PenpalAPara as PenpalA,
        PenpalAParaReceiver as PenpalAReceiver, PenpalAParaSender as PenpalASender,
        PenpalBPara as PenpalB, PenpalBParaReceiver as PenpalBReceiver,
        WestendRelay as Westend, WestendRelayReceiver as WestendReceiver,
        WestendRelaySender as WestendSender,
    };
    pub const ASSET_ID: u32 = 3;
    pub const ASSET_MIN_BALANCE: u128 = 1000;
    pub type RelayToParaTest = Test<Westend, PenpalA>;
    pub type ParaToRelayTest = Test<PenpalA, Westend>;
    pub type SystemParaToRelayTest = Test<AssetHubWestend, Westend>;
    pub type SystemParaToParaTest = Test<AssetHubWestend, PenpalA>;
    pub type ParaToSystemParaTest = Test<PenpalA, AssetHubWestend>;
    pub type ParaToParaThroughRelayTest = Test<PenpalA, PenpalB, Westend>;
    pub type ParaToParaThroughAHTest = Test<PenpalA, PenpalB, AssetHubWestend>;
    pub type RelayToParaThroughAHTest = Test<Westend, PenpalA, AssetHubWestend>;
}
#[cfg(test)]
mod tests {
    mod claim_assets {
        //! Tests related to claiming assets trapped during XCM execution.
        use crate::imports::*;
        use frame_support::{
            dispatch::RawOrigin, sp_runtime::{traits::Dispatchable, DispatchResult},
        };
        use emulated_integration_tests_common::test_chain_can_claim_assets;
        use xcm_executor::traits::DropAssets;
        use xcm_runtime_apis::{
            dry_run::runtime_decl_for_dry_run_api::DryRunApiV1,
            fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1,
        };
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::claim_assets::assets_can_be_claimed"]
        pub const assets_can_be_claimed: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName("tests::claim_assets::assets_can_be_claimed"),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/claim_assets.rs",
                start_line: 32usize,
                start_col: 4usize,
                end_line: 32usize,
                end_col: 25usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(assets_can_be_claimed()),
            ),
        };
        fn assets_can_be_claimed() {
            let amount = AssetHubWestendExistentialDeposit::get();
            let assets: Assets = (Parent, amount).into();
            let sender = AssetHubWestendSender::get();
            let origin = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeOrigin::signed(
                sender.clone(),
            );
            let beneficiary: Location = ::emulated_integration_tests_common::macros::AccountId32 {
                network: Some(NetworkId::Westend),
                id: sender.clone().into(),
            }
                .into();
            let versioned_assets: ::emulated_integration_tests_common::macros::VersionedAssets = assets
                .clone()
                .into();
            <AssetHubWestend>::execute_with(|| {
                <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::drop_assets(
                    &beneficiary,
                    assets.clone().into(),
                    &XcmContext {
                        origin: None,
                        message_id: [0u8; 32],
                        topic: None,
                    },
                );
                type RuntimeEvent = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::PolkadotXcm(
                            ::emulated_integration_tests_common::macros::pallet_xcm::Event::AssetsTrapped {
                                origin: beneficiary,
                                assets: versioned_assets,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::AssetsTrapped {\norigin: beneficiary, assets: versioned_assets, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::AssetsTrapped {\norigin: beneficiary, assets: versioned_assets, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::claim_assets",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let balance_before = <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    &sender,
                );
                let other_origin = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeOrigin::signed(
                    AssetHubWestendReceiver::get(),
                );
                if !<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::claim_assets(
                        other_origin,
                        Box::new(versioned_assets.clone().into()),
                        Box::new(beneficiary.clone().into()),
                    )
                    .is_err()
                {
                    ::core::panicking::panic(
                        "assertion failed: <AssetHubWestend as\n            AssetHubWestendPallet>::PolkadotXcm::claim_assets(other_origin,\n        bx!(versioned_assets.clone().into()),\n        bx!(beneficiary.clone().into())).is_err()",
                    )
                }
                let other_versioned_assets: ::emulated_integration_tests_common::macros::VersionedAssets = Assets::new()
                    .into();
                if !<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::claim_assets(
                        origin.clone(),
                        Box::new(other_versioned_assets.into()),
                        Box::new(beneficiary.clone().into()),
                    )
                    .is_err()
                {
                    ::core::panicking::panic(
                        "assertion failed: <AssetHubWestend as\n            AssetHubWestendPallet>::PolkadotXcm::claim_assets(origin.clone(),\n        bx!(other_versioned_assets.into()),\n        bx!(beneficiary.clone().into())).is_err()",
                    )
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::claim_assets(
                    origin.clone(),
                    Box::new(versioned_assets.clone().into()),
                    Box::new(beneficiary.clone().into()),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::PolkadotXcm(
                            ::emulated_integration_tests_common::macros::pallet_xcm::Event::AssetsClaimed {
                                origin: beneficiary,
                                assets: versioned_assets,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::AssetsClaimed {\norigin: beneficiary, assets: versioned_assets, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::AssetsClaimed {\norigin: beneficiary, assets: versioned_assets, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::claim_assets",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let balance_after = <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    &sender,
                );
                match (&balance_after, &(balance_before + amount)) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                if !<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::claim_assets(
                        origin.clone(),
                        Box::new(versioned_assets.clone().into()),
                        Box::new(beneficiary.clone().into()),
                    )
                    .is_err()
                {
                    ::core::panicking::panic(
                        "assertion failed: <AssetHubWestend as\n            AssetHubWestendPallet>::PolkadotXcm::claim_assets(origin.clone(),\n        bx!(versioned_assets.clone().into()),\n        bx!(beneficiary.clone().into())).is_err()",
                    )
                }
                let balance = <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    &sender,
                );
                match (&balance, &balance_after) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::drop_assets(
                    &beneficiary,
                    assets.clone().into(),
                    &XcmContext {
                        origin: None,
                        message_id: [0u8; 32],
                        topic: None,
                    },
                );
                let receiver = AssetHubWestendReceiver::get();
                let other_beneficiary: Location = ::emulated_integration_tests_common::macros::AccountId32 {
                    network: Some(NetworkId::Westend),
                    id: receiver.clone().into(),
                }
                    .into();
                let balance_before = <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    &receiver,
                );
                let is = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::claim_assets(
                    origin.clone(),
                    Box::new(versioned_assets.clone().into()),
                    Box::new(other_beneficiary.clone().into()),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let balance_after = <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    &receiver,
                );
                match (&balance_after, &(balance_before + amount)) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
        }
    }
    mod fellowship_treasury {
        use crate::imports::*;
        use emulated_integration_tests_common::{
            accounts::{ALICE, BOB},
            USDT_ID,
        };
        use frame_support::traits::fungibles::{Inspect, Mutate};
        use polkadot_runtime_common::impls::VersionedLocatableAsset;
        use xcm_executor::traits::ConvertLocation;
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::fellowship_treasury::create_and_claim_treasury_spend"]
        pub const create_and_claim_treasury_spend: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::fellowship_treasury::create_and_claim_treasury_spend",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/fellowship_treasury.rs",
                start_line: 26usize,
                start_col: 4usize,
                end_line: 26usize,
                end_col: 35usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(create_and_claim_treasury_spend()),
            ),
        };
        fn create_and_claim_treasury_spend() {
            const SPEND_AMOUNT: u128 = 1_000_000_000;
            let treasury_location: Location = Location::new(
                1,
                [Parachain(CollectivesWestend::para_id().into()), PalletInstance(65)],
            );
            let treasury_account = ahw_xcm_config::LocationToAccountId::convert_location(
                    &treasury_location,
                )
                .unwrap();
            let asset_hub_location = Location::new(
                1,
                [Parachain(AssetHubWestend::para_id().into())],
            );
            let root = <CollectivesWestend as Chain>::RuntimeOrigin::root();
            let asset_kind = VersionedLocatableAsset::V5 {
                location: asset_hub_location,
                asset_id: AssetId(
                    (PalletInstance(50), GeneralIndex(USDT_ID.into())).into(),
                ),
            };
            let alice: AccountId = Westend::account_id_of(ALICE);
            let bob: AccountId = CollectivesWestend::account_id_of(BOB);
            let bob_signed = <CollectivesWestend as Chain>::RuntimeOrigin::signed(
                bob.clone(),
            );
            AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                let is = <Assets as Mutate<
                    _,
                >>::mint_into(USDT_ID, &treasury_account, SPEND_AMOUNT * 4);
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                match (&<Assets as Inspect<_>>::balance(USDT_ID, &alice), &0u128) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
            CollectivesWestend::execute_with(|| {
                type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;
                type FellowshipTreasury = <CollectivesWestend as CollectivesWestendPallet>::FellowshipTreasury;
                type AssetRate = <CollectivesWestend as CollectivesWestendPallet>::AssetRate;
                let is = AssetRate::create(
                    root.clone(),
                    Box::new(asset_kind.clone()),
                    2.into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = FellowshipTreasury::spend(
                    root,
                    Box::new(asset_kind),
                    SPEND_AMOUNT,
                    Box::new(
                        Location::new(0, Into::<[u8; 32]>::into(alice.clone())).into(),
                    ),
                    None,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = FellowshipTreasury::payout(bob_signed.clone(), 0);
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <CollectivesWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::FellowshipTreasury(
                            pallet_treasury::Event::Paid { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "CollectivesWestend",
                                    "RuntimeEvent::FellowshipTreasury(pallet_treasury::Event::Paid { .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "CollectivesWestend",
                                    "RuntimeEvent::FellowshipTreasury(pallet_treasury::Event::Paid { .. })",
                                    <CollectivesWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <CollectivesWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::CollectivesWestend",
                                            "asset_hub_westend_integration_tests::tests::fellowship_treasury",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Assets(
                            pallet_assets::Event::Transferred {
                                asset_id: id,
                                from,
                                to,
                                amount,
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(id == &USDT_ID) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "id",
                                                id,
                                                "id == &USDT_ID",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= id == &USDT_ID;
                            if !(from == &treasury_account) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "from",
                                                from,
                                                "from == &treasury_account",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= from == &treasury_account;
                            if !(to == &alice) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "to",
                                                to,
                                                "to == &alice",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= to == &alice;
                            if !(amount == &SPEND_AMOUNT) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount",
                                                amount,
                                                "amount == &SPEND_AMOUNT",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= amount == &SPEND_AMOUNT;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Transferred {\nasset_id: id, from, to, amount })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Transferred {\nasset_id: id, from, to, amount })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::XcmpQueue(
                            cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { ..\n})",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::MessageQueue(
                            pallet_message_queue::Event::Processed { success: true, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::fellowship_treasury",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                match (
                    &<Assets as Inspect<_>>::balance(USDT_ID, &alice),
                    &SPEND_AMOUNT,
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
            CollectivesWestend::execute_with(|| {
                type RuntimeEvent = <CollectivesWestend as Chain>::RuntimeEvent;
                type FellowshipTreasury = <CollectivesWestend as CollectivesWestendPallet>::FellowshipTreasury;
                let is = FellowshipTreasury::check_status(bob_signed, 0);
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <CollectivesWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::FellowshipTreasury(
                            pallet_treasury::Event::SpendProcessed { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "CollectivesWestend",
                                    "RuntimeEvent::FellowshipTreasury(pallet_treasury::Event::SpendProcessed { ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "CollectivesWestend",
                                    "RuntimeEvent::FellowshipTreasury(pallet_treasury::Event::SpendProcessed { ..\n})",
                                    <CollectivesWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <CollectivesWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::CollectivesWestend",
                                            "asset_hub_westend_integration_tests::tests::fellowship_treasury",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
        }
    }
    mod hybrid_transfers {
        use super::reserve_transfer::*;
        use crate::{
            imports::*,
            tests::teleport::do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt,
        };
        fn para_to_para_assethub_hop_assertions(t: ParaToParaThroughAHTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            let sov_penpal_a_on_ah = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            let sov_penpal_b_on_ah = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalB::para_id()),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { who, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_a_on_ah) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_a_on_ah",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_a_on_ah;
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Minted { who, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_b_on_ah) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_b_on_ah",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_b_on_ah;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::MessageQueue(
                        pallet_message_queue::Event::Processed { success: true, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::hybrid_transfers",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn ah_to_para_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
            let fee_idx = t.args.fee_asset_item as usize;
            let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
            let custom_xcm_on_dest = Xcm::<
                (),
            >(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DepositAsset {
                            assets: Wild(AllCounted(t.args.assets.len() as u32)),
                            beneficiary: t.args.beneficiary,
                        },
                    ]),
                ),
            );
            <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.assets.into()),
                Box::new(TransferType::LocalReserve),
                Box::new(fee.id.into()),
                Box::new(TransferType::LocalReserve),
                Box::new(VersionedXcm::from(custom_xcm_on_dest)),
                t.args.weight_limit,
            )
        }
        fn para_to_ah_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
            let fee_idx = t.args.fee_asset_item as usize;
            let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
            let custom_xcm_on_dest = Xcm::<
                (),
            >(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DepositAsset {
                            assets: Wild(AllCounted(t.args.assets.len() as u32)),
                            beneficiary: t.args.beneficiary,
                        },
                    ]),
                ),
            );
            <PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.assets.into()),
                Box::new(TransferType::DestinationReserve),
                Box::new(fee.id.into()),
                Box::new(TransferType::DestinationReserve),
                Box::new(VersionedXcm::from(custom_xcm_on_dest)),
                t.args.weight_limit,
            )
        }
        fn para_to_para_transfer_assets_through_ah(
            t: ParaToParaThroughAHTest,
        ) -> DispatchResult {
            let fee_idx = t.args.fee_asset_item as usize;
            let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
            let asset_hub_location: Location = PenpalA::sibling_location_of(
                AssetHubWestend::para_id(),
            );
            let custom_xcm_on_dest = Xcm::<
                (),
            >(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DepositAsset {
                            assets: Wild(AllCounted(t.args.assets.len() as u32)),
                            beneficiary: t.args.beneficiary,
                        },
                    ]),
                ),
            );
            <PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.assets.into()),
                Box::new(TransferType::RemoteReserve(asset_hub_location.clone().into())),
                Box::new(fee.id.into()),
                Box::new(TransferType::RemoteReserve(asset_hub_location.into())),
                Box::new(VersionedXcm::from(custom_xcm_on_dest)),
                t.args.weight_limit,
            )
        }
        fn para_to_asset_hub_teleport_foreign_assets(
            t: ParaToSystemParaTest,
        ) -> DispatchResult {
            let fee_idx = t.args.fee_asset_item as usize;
            let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
            let custom_xcm_on_dest = Xcm::<
                (),
            >(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DepositAsset {
                            assets: Wild(AllCounted(t.args.assets.len() as u32)),
                            beneficiary: t.args.beneficiary,
                        },
                    ]),
                ),
            );
            <PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.assets.into()),
                Box::new(TransferType::Teleport),
                Box::new(fee.id.into()),
                Box::new(TransferType::DestinationReserve),
                Box::new(VersionedXcm::from(custom_xcm_on_dest)),
                t.args.weight_limit,
            )
        }
        fn asset_hub_to_para_teleport_foreign_assets(
            t: SystemParaToParaTest,
        ) -> DispatchResult {
            let fee_idx = t.args.fee_asset_item as usize;
            let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
            let custom_xcm_on_dest = Xcm::<
                (),
            >(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DepositAsset {
                            assets: Wild(AllCounted(t.args.assets.len() as u32)),
                            beneficiary: t.args.beneficiary,
                        },
                    ]),
                ),
            );
            <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.assets.into()),
                Box::new(TransferType::Teleport),
                Box::new(fee.id.into()),
                Box::new(TransferType::LocalReserve),
                Box::new(VersionedXcm::from(custom_xcm_on_dest)),
                t.args.weight_limit,
            )
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::hybrid_transfers::transfer_foreign_assets_from_asset_hub_to_para"]
        pub const transfer_foreign_assets_from_asset_hub_to_para: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::hybrid_transfers::transfer_foreign_assets_from_asset_hub_to_para",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/hybrid_transfers.rs",
                start_line: 156usize,
                start_col: 4usize,
                end_line: 156usize,
                end_col: 50usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    transfer_foreign_assets_from_asset_hub_to_para(),
                ),
            ),
        };
        /// Transfers of native asset plus bridged asset from AssetHub to some Parachain
        /// while paying fees using native asset.
        fn transfer_foreign_assets_from_asset_hub_to_para() {
            let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
            let sender = AssetHubWestendSender::get();
            let native_amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
            let native_asset_location = RelayLocation::get();
            let receiver = PenpalAReceiver::get();
            let assets_owner = PenpalAssetOwner::get();
            let foreign_amount_to_send = ASSET_HUB_WESTEND_ED * 10_000_000;
            let roc_at_westend_parachains = Location::new(
                2,
                [Junction::GlobalConsensus(NetworkId::Rococo)],
            );
            PenpalA::execute_with(|| {
                let is = <PenpalA as Chain>::System::set_storage(
                    <PenpalA as Chain>::RuntimeOrigin::root(),
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            (
                                PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
                                Location::new(2, [GlobalConsensus(Rococo)]).encode(),
                            ),
                        ]),
                    ),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
            PenpalA::force_create_foreign_asset(
                roc_at_westend_parachains.clone(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            AssetHubWestend::force_create_foreign_asset(
                roc_at_westend_parachains.clone().try_into().unwrap(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            AssetHubWestend::mint_foreign_asset(
                <AssetHubWestend as Chain>::RuntimeOrigin::signed(assets_owner),
                roc_at_westend_parachains.clone().try_into().unwrap(),
                sender.clone(),
                foreign_amount_to_send * 2,
            );
            let assets: Vec<Asset> = <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (Parent, native_amount_to_send).into(),
                    (roc_at_westend_parachains.clone(), foreign_amount_to_send).into(),
                ]),
            );
            let fee_asset_id = AssetId(Parent.into());
            let fee_asset_item = assets
                .iter()
                .position(|a| a.id == fee_asset_id)
                .unwrap() as u32;
            let test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination.clone(),
                    receiver.clone(),
                    native_amount_to_send,
                    assets.into(),
                    None,
                    fee_asset_item,
                ),
            };
            let mut test = SystemParaToParaTest::new(test_args);
            let sender_balance_before = test.sender.balance;
            let sender_rocs_before = AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &sender,
                )
            });
            let receiver_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(native_asset_location.clone(), &receiver)
            });
            let receiver_rocs_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.clone(), &receiver)
            });
            test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
            test.set_assertion::<PenpalA>(system_para_to_para_receiver_assertions);
            test.set_dispatchable::<AssetHubWestend>(ah_to_para_transfer_assets);
            test.assert();
            let sender_balance_after = test.sender.balance;
            let sender_rocs_after = AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &sender,
                )
            });
            let receiver_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(native_asset_location, &receiver)
            });
            let receiver_rocs_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains, &receiver)
            });
            if !(sender_balance_after < sender_balance_before - native_amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_balance_after < sender_balance_before - native_amount_to_send",
                )
            }
            match (&sender_rocs_after, &(sender_rocs_before - foreign_amount_to_send)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            if !(receiver_assets_after > receiver_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after > receiver_assets_before",
                )
            }
            if !(receiver_assets_after < receiver_assets_before + native_amount_to_send)
            {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after < receiver_assets_before + native_amount_to_send",
                )
            }
            match (
                &receiver_rocs_after,
                &(receiver_rocs_before + foreign_amount_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::hybrid_transfers::transfer_foreign_assets_from_para_to_asset_hub"]
        pub const transfer_foreign_assets_from_para_to_asset_hub: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::hybrid_transfers::transfer_foreign_assets_from_para_to_asset_hub",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/hybrid_transfers.rs",
                start_line: 285usize,
                start_col: 4usize,
                end_line: 285usize,
                end_col: 50usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    transfer_foreign_assets_from_para_to_asset_hub(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Parachain to System Parachain should work
        /// Transfers of native asset plus bridged asset from some Parachain to AssetHub
        /// while paying fees using native asset.
        fn transfer_foreign_assets_from_para_to_asset_hub() {
            let destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
            let sender = PenpalASender::get();
            let native_amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 10000;
            let native_asset_location = RelayLocation::get();
            let assets_owner = PenpalAssetOwner::get();
            let foreign_amount_to_send = ASSET_HUB_WESTEND_ED * 10_000_000;
            let roc_at_westend_parachains = Location::new(
                2,
                [Junction::GlobalConsensus(NetworkId::Rococo)],
            );
            PenpalA::execute_with(|| {
                let is = <PenpalA as Chain>::System::set_storage(
                    <PenpalA as Chain>::RuntimeOrigin::root(),
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            (
                                PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
                                Location::new(2, [GlobalConsensus(Rococo)]).encode(),
                            ),
                        ]),
                    ),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
            PenpalA::force_create_foreign_asset(
                roc_at_westend_parachains.clone(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            AssetHubWestend::force_create_foreign_asset(
                roc_at_westend_parachains.clone().try_into().unwrap(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
                native_asset_location.clone(),
                sender.clone(),
                native_amount_to_send * 2,
            );
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
                roc_at_westend_parachains.clone(),
                sender.clone(),
                foreign_amount_to_send * 2,
            );
            let receiver = AssetHubWestendReceiver::get();
            let penpal_location_as_seen_by_ahr = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                penpal_location_as_seen_by_ahr,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_penpal_on_ahr.clone().into(), native_amount_to_send * 2),
                    ]),
                ),
            );
            AssetHubWestend::mint_foreign_asset(
                <AssetHubWestend as Chain>::RuntimeOrigin::signed(assets_owner),
                roc_at_westend_parachains.clone().try_into().unwrap(),
                sov_penpal_on_ahr,
                foreign_amount_to_send * 2,
            );
            let assets: Vec<Asset> = <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (Parent, native_amount_to_send).into(),
                    (roc_at_westend_parachains.clone(), foreign_amount_to_send).into(),
                ]),
            );
            let fee_asset_id = AssetId(Parent.into());
            let fee_asset_item = assets
                .iter()
                .position(|a| a.id == fee_asset_id)
                .unwrap() as u32;
            let test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination.clone(),
                    receiver.clone(),
                    native_amount_to_send,
                    assets.into(),
                    None,
                    fee_asset_item,
                ),
            };
            let mut test = ParaToSystemParaTest::new(test_args);
            let sender_native_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(native_asset_location.clone(), &sender)
            });
            let sender_rocs_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.clone(), &sender)
            });
            let receiver_native_before = test.receiver.balance;
            let receiver_rocs_before = AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &receiver,
                )
            });
            test.set_assertion::<PenpalA>(para_to_system_para_sender_assertions);
            test.set_assertion::<
                    AssetHubWestend,
                >(para_to_system_para_receiver_assertions);
            test.set_dispatchable::<PenpalA>(para_to_ah_transfer_assets);
            test.assert();
            let sender_native_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(native_asset_location, &sender)
            });
            let sender_rocs_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.clone(), &sender)
            });
            let receiver_native_after = test.receiver.balance;
            let receiver_rocs_after = AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.try_into().unwrap(), &receiver)
            });
            if !(sender_native_after < sender_native_before - native_amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_native_after < sender_native_before - native_amount_to_send",
                )
            }
            match (&sender_rocs_after, &(sender_rocs_before - foreign_amount_to_send)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            if !(receiver_native_after > receiver_native_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_native_after > receiver_native_before",
                )
            }
            if !(receiver_native_after < receiver_native_before + native_amount_to_send)
            {
                ::core::panicking::panic(
                    "assertion failed: receiver_native_after < receiver_native_before + native_amount_to_send",
                )
            }
            match (
                &receiver_rocs_after,
                &(receiver_rocs_before + foreign_amount_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::hybrid_transfers::transfer_foreign_assets_from_para_to_para_through_asset_hub"]
        pub const transfer_foreign_assets_from_para_to_para_through_asset_hub: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::hybrid_transfers::transfer_foreign_assets_from_para_to_para_through_asset_hub",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/hybrid_transfers.rs",
                start_line: 440usize,
                start_col: 4usize,
                end_line: 440usize,
                end_col: 63usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    transfer_foreign_assets_from_para_to_para_through_asset_hub(),
                ),
            ),
        };
        /// Transfers of native asset plus bridged asset from Parachain to Parachain
        /// (through AssetHub reserve) with fees paid using native asset.
        fn transfer_foreign_assets_from_para_to_para_through_asset_hub() {
            let destination = PenpalA::sibling_location_of(PenpalB::para_id());
            let sender = PenpalASender::get();
            let wnd_to_send: Balance = WESTEND_ED * 10000;
            let assets_owner = PenpalAssetOwner::get();
            let wnd_location = RelayLocation::get();
            let sender_as_seen_by_ah = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_of_sender_on_ah = AssetHubWestend::sovereign_account_id_of(
                sender_as_seen_by_ah,
            );
            let receiver_as_seen_by_ah = AssetHubWestend::sibling_location_of(
                PenpalB::para_id(),
            );
            let sov_of_receiver_on_ah = AssetHubWestend::sovereign_account_id_of(
                receiver_as_seen_by_ah,
            );
            let roc_to_send = ASSET_HUB_WESTEND_ED * 10_000_000;
            PenpalB::execute_with(|| {
                let is = <PenpalB as Chain>::System::set_storage(
                    <PenpalB as Chain>::RuntimeOrigin::root(),
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            (
                                PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
                                Location::new(2, [GlobalConsensus(Rococo)]).encode(),
                            ),
                        ]),
                    ),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
            let roc_at_westend_parachains = Location::new(
                2,
                [Junction::GlobalConsensus(NetworkId::Rococo)],
            );
            AssetHubWestend::force_create_foreign_asset(
                roc_at_westend_parachains.clone().try_into().unwrap(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            PenpalA::force_create_foreign_asset(
                roc_at_westend_parachains.clone(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            PenpalB::force_create_foreign_asset(
                roc_at_westend_parachains.clone(),
                assets_owner.clone(),
                false,
                ASSET_MIN_BALANCE,
                ::alloc::vec::Vec::new(),
            );
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
                wnd_location.clone(),
                sender.clone(),
                wnd_to_send * 2,
            );
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
                roc_at_westend_parachains.clone(),
                sender.clone(),
                roc_to_send * 2,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_of_sender_on_ah.clone().into(), wnd_to_send * 2),
                    ]),
                ),
            );
            AssetHubWestend::mint_foreign_asset(
                <AssetHubWestend as Chain>::RuntimeOrigin::signed(assets_owner),
                roc_at_westend_parachains.clone().try_into().unwrap(),
                sov_of_sender_on_ah.clone(),
                roc_to_send * 2,
            );
            let receiver = PenpalBReceiver::get();
            let assets: Vec<Asset> = <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    (wnd_location.clone(), wnd_to_send).into(),
                    (roc_at_westend_parachains.clone(), roc_to_send).into(),
                ]),
            );
            let fee_asset_id: AssetId = wnd_location.clone().into();
            let fee_asset_item = assets
                .iter()
                .position(|a| a.id == fee_asset_id)
                .unwrap() as u32;
            let test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination,
                    receiver.clone(),
                    wnd_to_send,
                    assets.into(),
                    None,
                    fee_asset_item,
                ),
            };
            let mut test = ParaToParaThroughAHTest::new(test_args);
            let sender_wnds_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(wnd_location.clone(), &sender)
            });
            let sender_rocs_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.clone(), &sender)
            });
            let wnds_in_sender_reserve_on_ah_before = <AssetHubWestend as Chain>::account_data_of(
                    sov_of_sender_on_ah.clone(),
                )
                .free;
            let rocs_in_sender_reserve_on_ah_before = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <Assets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &sov_of_sender_on_ah,
                )
            });
            let wnds_in_receiver_reserve_on_ah_before = <AssetHubWestend as Chain>::account_data_of(
                    sov_of_receiver_on_ah.clone(),
                )
                .free;
            let rocs_in_receiver_reserve_on_ah_before = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <Assets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &sov_of_receiver_on_ah,
                )
            });
            let receiver_wnds_before = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(wnd_location.clone(), &receiver)
            });
            let receiver_rocs_before = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.clone(), &receiver)
            });
            test.set_assertion::<PenpalA>(para_to_para_through_hop_sender_assertions);
            test.set_assertion::<AssetHubWestend>(para_to_para_assethub_hop_assertions);
            test.set_assertion::<PenpalB>(para_to_para_through_hop_receiver_assertions);
            test.set_dispatchable::<PenpalA>(para_to_para_transfer_assets_through_ah);
            test.assert();
            let sender_wnds_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(wnd_location.clone(), &sender)
            });
            let sender_rocs_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains.clone(), &sender)
            });
            let rocs_in_sender_reserve_on_ah_after = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <Assets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &sov_of_sender_on_ah,
                )
            });
            let wnds_in_sender_reserve_on_ah_after = <AssetHubWestend as Chain>::account_data_of(
                    sov_of_sender_on_ah,
                )
                .free;
            let rocs_in_receiver_reserve_on_ah_after = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <Assets as Inspect<
                    _,
                >>::balance(
                    roc_at_westend_parachains.clone().try_into().unwrap(),
                    &sov_of_receiver_on_ah,
                )
            });
            let wnds_in_receiver_reserve_on_ah_after = <AssetHubWestend as Chain>::account_data_of(
                    sov_of_receiver_on_ah,
                )
                .free;
            let receiver_wnds_after = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(wnd_location, &receiver)
            });
            let receiver_rocs_after = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(roc_at_westend_parachains, &receiver)
            });
            if !(sender_wnds_after < sender_wnds_before - wnd_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_wnds_after < sender_wnds_before - wnd_to_send",
                )
            }
            match (&sender_rocs_after, &(sender_rocs_before - roc_to_send)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &wnds_in_sender_reserve_on_ah_after,
                &(wnds_in_sender_reserve_on_ah_before - wnd_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &rocs_in_sender_reserve_on_ah_after,
                &(rocs_in_sender_reserve_on_ah_before - roc_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            if !(wnds_in_receiver_reserve_on_ah_after
                > wnds_in_receiver_reserve_on_ah_before)
            {
                ::core::panicking::panic(
                    "assertion failed: wnds_in_receiver_reserve_on_ah_after > wnds_in_receiver_reserve_on_ah_before",
                )
            }
            match (
                &rocs_in_receiver_reserve_on_ah_after,
                &(rocs_in_receiver_reserve_on_ah_before + roc_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            if !(receiver_wnds_after > receiver_wnds_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_wnds_after > receiver_wnds_before",
                )
            }
            match (&receiver_rocs_after, &(receiver_rocs_before + roc_to_send)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::hybrid_transfers::bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types"]
        pub const bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::hybrid_transfers::bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/hybrid_transfers.rs",
                start_line: 644usize,
                start_col: 4usize,
                end_line: 644usize,
                end_col: 97usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types(),
                ),
            ),
        };
        /// Transfers of native asset plus teleportable foreign asset from Parachain to AssetHub and back
        /// with fees paid using native asset.
        fn bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types() {
            do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
                para_to_asset_hub_teleport_foreign_assets,
                asset_hub_to_para_teleport_foreign_assets,
            );
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::hybrid_transfers::transfer_native_asset_from_relay_to_para_through_asset_hub"]
        pub const transfer_native_asset_from_relay_to_para_through_asset_hub: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::hybrid_transfers::transfer_native_asset_from_relay_to_para_through_asset_hub",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/hybrid_transfers.rs",
                start_line: 658usize,
                start_col: 4usize,
                end_line: 658usize,
                end_col: 62usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    transfer_native_asset_from_relay_to_para_through_asset_hub(),
                ),
            ),
        };
        /// Transfers of native asset Relay to Parachain (using AssetHub reserve). Parachains want to avoid
        /// managing SAs on all system chains, thus want all their DOT-in-reserve to be held in their
        /// Sovereign Account on Asset Hub.
        fn transfer_native_asset_from_relay_to_para_through_asset_hub() {
            let destination = Westend::child_location_of(PenpalA::para_id());
            let sender = WestendSender::get();
            let amount_to_send: Balance = WESTEND_ED * 1000;
            let relay_native_asset_location = RelayLocation::get();
            let receiver = PenpalAReceiver::get();
            let test_args = TestContext {
                sender,
                receiver: receiver.clone(),
                args: TestArgs::new_relay(
                    destination.clone(),
                    receiver.clone(),
                    amount_to_send,
                ),
            };
            let mut test = RelayToParaThroughAHTest::new(test_args);
            let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            let sender_balance_before = test.sender.balance;
            let sov_penpal_on_ah_before = AssetHubWestend::execute_with(|| {
                <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    sov_penpal_on_ah.clone(),
                )
            });
            let receiver_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &receiver)
            });
            fn relay_assertions(t: RelayToParaThroughAHTest) {
                type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
                Westend::assert_xcm_pallet_attempted_complete(None);
                let mut message: Vec<String> = Vec::new();
                let mut events = <Westend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Balances(
                            pallet_balances::Event::Burned { who, amount },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*who == t.sender.account_id) && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "who",
                                                who,
                                                "*who == t.sender.account_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *who == t.sender.account_id;
                            if !(*amount == t.args.amount) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount",
                                                amount,
                                                "*amount == t.args.amount",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount == t.args.amount;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Balances(
                            pallet_balances::Event::Minted { who, amount },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*who
                                == <Westend as WestendPallet>::XcmPallet::check_account())
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "who",
                                                who,
                                                "*who == <Westend as WestendPallet>::XcmPallet::check_account()",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions
                                &= *who
                                    == <Westend as WestendPallet>::XcmPallet::check_account();
                            if !(*amount == t.args.amount) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount",
                                                amount,
                                                "*amount == t.args.amount",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount == t.args.amount;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, amount })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, amount })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <Westend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::Westend",
                                            "asset_hub_westend_integration_tests::tests::hybrid_transfers",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
            fn asset_hub_assertions(_: RelayToParaThroughAHTest) {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(
                    AssetHubWestend::sibling_location_of(PenpalA::para_id()),
                );
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Balances(
                            pallet_balances::Event::Minted { who, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*who == sov_penpal_on_ah) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "who",
                                                who,
                                                "*who == sov_penpal_on_ah",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *who == sov_penpal_on_ah;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::MessageQueue(
                            pallet_message_queue::Event::Processed { success: true, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::hybrid_transfers",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
            fn penpal_assertions(t: RelayToParaThroughAHTest) {
                type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
                let expected_id = t
                    .args
                    .assets
                    .into_inner()
                    .first()
                    .unwrap()
                    .id
                    .0
                    .clone()
                    .try_into()
                    .unwrap();
                let mut message: Vec<String> = Vec::new();
                let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Issued { asset_id, owner, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == expected_id) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == expected_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == expected_id;
                            if !(*owner == t.receiver.account_id)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == t.receiver.account_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == t.receiver.account_id;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                    <PenpalA as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <PenpalA as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::PenpalA",
                                            "asset_hub_westend_integration_tests::tests::hybrid_transfers",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
            fn transfer_assets_dispatchable(
                t: RelayToParaThroughAHTest,
            ) -> DispatchResult {
                let fee_idx = t.args.fee_asset_item as usize;
                let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
                let asset_hub_location = Westend::child_location_of(
                    AssetHubWestend::para_id(),
                );
                let context = WestendUniversalLocation::get();
                let mut remote_fees = fee
                    .clone()
                    .reanchored(&t.args.dest, &context)
                    .unwrap();
                if let Fungible(ref mut amount) = remote_fees.fun {
                    *amount = *amount / 2;
                }
                let xcm_on_final_dest = Xcm::<
                    (),
                >(
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            BuyExecution {
                                fees: remote_fees,
                                weight_limit: t.args.weight_limit.clone(),
                            },
                            DepositAsset {
                                assets: Wild(AllCounted(t.args.assets.len() as u32)),
                                beneficiary: t.args.beneficiary,
                            },
                        ]),
                    ),
                );
                let mut dest = t.args.dest.clone();
                dest.reanchor(&asset_hub_location, &context).unwrap();
                let xcm_on_hop = Xcm::<
                    (),
                >(
                    <[_]>::into_vec(
                        #[rustc_box]
                        ::alloc::boxed::Box::new([
                            DepositReserveAsset {
                                assets: Wild(AllCounted(t.args.assets.len() as u32)),
                                dest,
                                xcm: xcm_on_final_dest,
                            },
                        ]),
                    ),
                );
                <Westend as WestendPallet>::XcmPallet::transfer_assets_using_type_and_then(
                    t.signed_origin,
                    Box::new(asset_hub_location.into()),
                    Box::new(t.args.assets.into()),
                    Box::new(TransferType::Teleport),
                    Box::new(fee.id.into()),
                    Box::new(TransferType::Teleport),
                    Box::new(VersionedXcm::from(xcm_on_hop)),
                    t.args.weight_limit,
                )
            }
            test.set_assertion::<Westend>(relay_assertions);
            test.set_assertion::<AssetHubWestend>(asset_hub_assertions);
            test.set_assertion::<PenpalA>(penpal_assertions);
            test.set_dispatchable::<Westend>(transfer_assets_dispatchable);
            test.assert();
            let sender_balance_after = test.sender.balance;
            let sov_penpal_on_ah_after = AssetHubWestend::execute_with(|| {
                <AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                    sov_penpal_on_ah,
                )
            });
            let receiver_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location, &receiver)
            });
            if !(sender_balance_after < sender_balance_before - amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_balance_after < sender_balance_before - amount_to_send",
                )
            }
            if !(sov_penpal_on_ah_after > sov_penpal_on_ah_before) {
                ::core::panicking::panic(
                    "assertion failed: sov_penpal_on_ah_after > sov_penpal_on_ah_before",
                )
            }
            if !(receiver_assets_after > receiver_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after > receiver_assets_before",
                )
            }
            if !(receiver_assets_after < receiver_assets_before + amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after < receiver_assets_before + amount_to_send",
                )
            }
        }
    }
    mod reserve_transfer {
        use crate::imports::*;
        fn relay_to_para_sender_assertions(t: RelayToParaTest) {
            type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
            Westend::assert_xcm_pallet_attempted_complete(
                Some(Weight::from_parts(864_610_000, 8_799)),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <Westend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Transfer { from, to, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*from == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "from",
                                            from,
                                            "*from == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *from == t.sender.account_id;
                        if !(*to
                            == Westend::sovereign_account_id_of(t.args.dest.clone()))
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "to",
                                            to,
                                            "*to == Westend::sovereign_account_id_of(t.args.dest.clone())",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *to
                                == Westend::sovereign_account_id_of(t.args.dest.clone());
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Westend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Westend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn para_to_relay_sender_assertions(t: ParaToRelayTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            PenpalA::assert_xcm_pallet_attempted_complete(
                Some(Weight::from_parts(864_610_000, 8_799)),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Burned { asset_id, owner, balance, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == RelayLocation::get())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == RelayLocation::get()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == RelayLocation::get();
                        if !(*owner == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.sender.account_id;
                        if !(*balance == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance, .. })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        pub fn system_para_to_para_sender_assertions(t: SystemParaToParaTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            AssetHubWestend::assert_xcm_pallet_attempted_complete(None);
            let sov_acc_of_dest = AssetHubWestend::sovereign_account_id_of(
                t.args.dest.clone(),
            );
            for (idx, asset) in t.args.assets.into_inner().into_iter().enumerate() {
                let expected_id = asset.id.0.clone().try_into().unwrap();
                let asset_amount = if let Fungible(a) = asset.fun {
                    Some(a)
                } else {
                    None
                }
                    .unwrap();
                if idx == t.args.fee_asset_item as usize {
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                pallet_balances::Event::Transfer { from, to, amount },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if !(*from == t.sender.account_id)
                                    && event_message.is_empty()
                                {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "from",
                                                    from,
                                                    "*from == t.sender.account_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *from == t.sender.account_id;
                                if !(*to == sov_acc_of_dest) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "to",
                                                    to,
                                                    "*to == sov_acc_of_dest",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *to == sov_acc_of_dest;
                                if !(*amount == asset_amount) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "amount",
                                                    amount,
                                                    "*amount == asset_amount",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *amount == asset_amount;
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <AssetHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::AssetHubWestend",
                                                "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                } else {
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::ForeignAssets(
                                pallet_assets::Event::Transferred {
                                    asset_id,
                                    from,
                                    to,
                                    amount,
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if !(*asset_id == expected_id) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "asset_id",
                                                    asset_id,
                                                    "*asset_id == expected_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *asset_id == expected_id;
                                if !(*from == t.sender.account_id)
                                    && event_message.is_empty()
                                {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "from",
                                                    from,
                                                    "*from == t.sender.account_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *from == t.sender.account_id;
                                if !(*to == sov_acc_of_dest) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "to",
                                                    to,
                                                    "*to == sov_acc_of_dest",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *to == sov_acc_of_dest;
                                if !(*amount == asset_amount) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "amount",
                                                    amount,
                                                    "*amount == asset_amount",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *amount == asset_amount;
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::ForeignAssets(pallet_assets::Event::Transferred {\nasset_id, from, to, amount })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::ForeignAssets(pallet_assets::Event::Transferred {\nasset_id, from, to, amount })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <AssetHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::AssetHubWestend",
                                                "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                }
            }
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. }) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
            AssetHubWestend::assert_xcm_pallet_sent();
        }
        pub fn system_para_to_para_receiver_assertions(t: SystemParaToParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            PenpalA::assert_xcmp_queue_success(None);
            for asset in t.args.assets.into_inner().into_iter() {
                let expected_id = asset.id.0.try_into().unwrap();
                let mut message: Vec<String> = Vec::new();
                let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Issued { asset_id, owner, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == expected_id) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == expected_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == expected_id;
                            if !(*owner == t.receiver.account_id)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == t.receiver.account_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == t.receiver.account_id;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                    <PenpalA as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <PenpalA as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::PenpalA",
                                            "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
        }
        pub fn para_to_system_para_sender_assertions(t: ParaToSystemParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            PenpalA::assert_xcm_pallet_attempted_complete(None);
            for asset in t.args.assets.into_inner().into_iter() {
                let expected_id = asset.id.0;
                let asset_amount = if let Fungible(a) = asset.fun {
                    Some(a)
                } else {
                    None
                }
                    .unwrap();
                let mut message: Vec<String> = Vec::new();
                let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Burned { asset_id, owner, balance },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == expected_id) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == expected_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == expected_id;
                            if !(*owner == t.sender.account_id)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == t.sender.account_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == t.sender.account_id;
                            if !(*balance == asset_amount) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "balance",
                                                balance,
                                                "*balance == asset_amount",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *balance == asset_amount;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                    <PenpalA as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <PenpalA as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::PenpalA",
                                            "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
        }
        fn para_to_relay_receiver_assertions(t: ParaToRelayTest) {
            type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
            let sov_penpal_on_relay = Westend::sovereign_account_id_of(
                Westend::child_location_of(PenpalA::para_id()),
            );
            Westend::assert_ump_queue_processed(
                true,
                Some(PenpalA::para_id()),
                Some(Weight::from_parts(306305000, 7_186)),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <Westend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { who, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_on_relay.clone().into())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_on_relay.clone().into()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_on_relay.clone().into();
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(pallet_balances::Event::Minted { .. }) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { .. })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::MessageQueue(
                        pallet_message_queue::Event::Processed { success: true, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Westend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Westend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        pub fn para_to_system_para_receiver_assertions(t: ParaToSystemParaTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            AssetHubWestend::assert_xcmp_queue_success(None);
            let sov_acc_of_penpal = AssetHubWestend::sovereign_account_id_of(
                t.args.dest.clone(),
            );
            for (idx, asset) in t.args.assets.into_inner().into_iter().enumerate() {
                let expected_id = asset.id.0.clone().try_into().unwrap();
                let asset_amount = if let Fungible(a) = asset.fun {
                    Some(a)
                } else {
                    None
                }
                    .unwrap();
                if idx == t.args.fee_asset_item as usize {
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                pallet_balances::Event::Burned { who, amount },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if !(*who == sov_acc_of_penpal.clone().into())
                                    && event_message.is_empty()
                                {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "who",
                                                    who,
                                                    "*who == sov_acc_of_penpal.clone().into()",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *who == sov_acc_of_penpal.clone().into();
                                if !(*amount == asset_amount) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "amount",
                                                    amount,
                                                    "*amount == asset_amount",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *amount == asset_amount;
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                pallet_balances::Event::Minted { who, .. },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if !(*who == t.receiver.account_id)
                                    && event_message.is_empty()
                                {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "who",
                                                    who,
                                                    "*who == t.receiver.account_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *who == t.receiver.account_id;
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <AssetHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::AssetHubWestend",
                                                "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                } else {
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::ForeignAssets(
                                pallet_assets::Event::Burned { asset_id, owner, balance },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if !(*asset_id == expected_id) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "asset_id",
                                                    asset_id,
                                                    "*asset_id == expected_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *asset_id == expected_id;
                                if !(*owner == sov_acc_of_penpal)
                                    && event_message.is_empty()
                                {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "owner",
                                                    owner,
                                                    "*owner == sov_acc_of_penpal",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *owner == sov_acc_of_penpal;
                                if !(*balance == asset_amount) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "balance",
                                                    balance,
                                                    "*balance == asset_amount",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *balance == asset_amount;
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::ForeignAssets(
                                pallet_assets::Event::Issued { asset_id, owner, amount },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if !(*asset_id == expected_id) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "asset_id",
                                                    asset_id,
                                                    "*asset_id == expected_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *asset_id == expected_id;
                                if !(*owner == t.receiver.account_id)
                                    && event_message.is_empty()
                                {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "owner",
                                                    owner,
                                                    "*owner == t.receiver.account_id",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *owner == t.receiver.account_id;
                                if !(*amount == asset_amount) && event_message.is_empty() {
                                    conditions_message
                                        .push({
                                            let res = ::alloc::fmt::format(
                                                format_args!(
                                                    " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                    "amount",
                                                    amount,
                                                    "*amount == asset_amount",
                                                ),
                                            );
                                            res
                                        });
                                }
                                meet_conditions &= *amount == asset_amount;
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <AssetHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::AssetHubWestend",
                                                "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                }
            }
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::MessageQueue(
                        pallet_message_queue::Event::Processed { success: true, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn system_para_to_para_assets_sender_assertions(t: SystemParaToParaTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            AssetHubWestend::assert_xcm_pallet_attempted_complete(
                Some(Weight::from_parts(864_610_000, 8799)),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Assets(
                        pallet_assets::Event::Transferred { asset_id, from, to, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == RESERVABLE_ASSET_ID)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == RESERVABLE_ASSET_ID",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == RESERVABLE_ASSET_ID;
                        if !(*from == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "from",
                                            from,
                                            "*from == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *from == t.sender.account_id;
                        if !(*to
                            == AssetHubWestend::sovereign_account_id_of(
                                t.args.dest.clone(),
                            )) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "to",
                                            to,
                                            "*to == AssetHubWestend::sovereign_account_id_of(t.args.dest.clone())",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *to
                                == AssetHubWestend::sovereign_account_id_of(
                                    t.args.dest.clone(),
                                );
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Assets(pallet_assets::Event::Transferred {\nasset_id, from, to, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Assets(pallet_assets::Event::Transferred {\nasset_id, from, to, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Minted { who, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who
                            == AssetHubWestend::sovereign_account_id_of(
                                t.args.dest.clone(),
                            )) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == AssetHubWestend::sovereign_account_id_of(t.args.dest.clone())",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *who
                                == AssetHubWestend::sovereign_account_id_of(
                                    t.args.dest.clone(),
                                );
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. }) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn para_to_system_para_assets_sender_assertions(t: ParaToSystemParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            let system_para_native_asset_location = RelayLocation::get();
            let reservable_asset_location = PenpalLocalReservableFromAssetHub::get();
            PenpalA::assert_xcm_pallet_attempted_complete(
                Some(Weight::from_parts(864_610_000, 8799)),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Burned { asset_id, owner, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == system_para_native_asset_location)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == system_para_native_asset_location",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *asset_id == system_para_native_asset_location;
                        if !(*owner == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.sender.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { asset_id, owner, ..\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { asset_id, owner, ..\n})",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Burned { asset_id, owner, balance },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == reservable_asset_location)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == reservable_asset_location",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == reservable_asset_location;
                        if !(*owner == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.sender.account_id;
                        if !(*balance == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. }) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn system_para_to_para_assets_receiver_assertions(t: SystemParaToParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            let system_para_asset_location = PenpalLocalReservableFromAssetHub::get();
            PenpalA::assert_xcmp_queue_success(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Issued { asset_id, owner, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == RelayLocation::get())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == RelayLocation::get()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == RelayLocation::get();
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Issued { asset_id, owner, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == system_para_asset_location)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == system_para_asset_location",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == system_para_asset_location;
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn para_to_system_para_assets_receiver_assertions(t: ParaToSystemParaTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            AssetHubWestend::assert_xcmp_queue_success(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Assets(
                        pallet_assets::Event::Burned { asset_id, owner, balance },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == RESERVABLE_ASSET_ID)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == RESERVABLE_ASSET_ID",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == RESERVABLE_ASSET_ID;
                        if !(*owner == sov_penpal_on_ahr) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == sov_penpal_on_ahr",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == sov_penpal_on_ahr;
                        if !(*balance == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { who, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_on_ahr) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_on_ahr",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_on_ahr;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Assets(
                        pallet_assets::Event::Issued { asset_id, owner, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == RESERVABLE_ASSET_ID)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == RESERVABLE_ASSET_ID",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == RESERVABLE_ASSET_ID;
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, owner, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Minted { who, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == t.receiver.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == t.receiver.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn relay_to_para_assets_receiver_assertions(t: RelayToParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Issued { asset_id, owner, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == RelayLocation::get())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == RelayLocation::get()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == RelayLocation::get();
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::MessageQueue(
                        pallet_message_queue::Event::Processed { success: true, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        pub fn para_to_para_through_hop_sender_assertions<Hop: Clone>(
            t: Test<PenpalA, PenpalB, Hop>,
        ) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            PenpalA::assert_xcm_pallet_attempted_complete(None);
            for asset in t.args.assets.into_inner() {
                let expected_id = asset.id.0.clone().try_into().unwrap();
                let amount = if let Fungible(a) = asset.fun { Some(a) } else { None }
                    .unwrap();
                let mut message: Vec<String> = Vec::new();
                let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Burned { asset_id, owner, balance },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == expected_id) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == expected_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == expected_id;
                            if !(*owner == t.sender.account_id)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == t.sender.account_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == t.sender.account_id;
                            if !(*balance == amount) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "balance",
                                                balance,
                                                "*balance == amount",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *balance == amount;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "PenpalA",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                    <PenpalA as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <PenpalA as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::PenpalA",
                                            "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
        }
        fn para_to_para_relay_hop_assertions(t: ParaToParaThroughRelayTest) {
            type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
            let sov_penpal_a_on_westend = Westend::sovereign_account_id_of(
                Westend::child_location_of(PenpalA::para_id()),
            );
            let sov_penpal_b_on_westend = Westend::sovereign_account_id_of(
                Westend::child_location_of(PenpalB::para_id()),
            );
            let mut message: Vec<String> = Vec::new();
            let mut events = <Westend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { who, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_a_on_westend) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_a_on_westend",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_a_on_westend;
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Minted { who, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_b_on_westend) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_b_on_westend",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_b_on_westend;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::MessageQueue(
                        pallet_message_queue::Event::Processed { success: true, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "Westend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "Westend",
                                "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                <Westend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <Westend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::Westend",
                                        "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        pub fn para_to_para_through_hop_receiver_assertions<Hop: Clone>(
            t: Test<PenpalA, PenpalB, Hop>,
        ) {
            type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
            PenpalB::assert_xcmp_queue_success(None);
            for asset in t.args.assets.into_inner().into_iter() {
                let expected_id = asset.id.0.try_into().unwrap();
                let mut message: Vec<String> = Vec::new();
                let mut events = <PenpalB as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Issued { asset_id, owner, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == expected_id) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == expected_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == expected_id;
                            if !(*owner == t.receiver.account_id)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == t.receiver.account_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == t.receiver.account_id;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "PenpalB",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "PenpalB",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                    <PenpalB as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <PenpalB as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::PenpalB",
                                            "asset_hub_westend_integration_tests::tests::reserve_transfer",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            }
        }
        fn relay_to_para_reserve_transfer_assets(t: RelayToParaTest) -> DispatchResult {
            <Westend as WestendPallet>::XcmPallet::limited_reserve_transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        fn para_to_relay_reserve_transfer_assets(t: ParaToRelayTest) -> DispatchResult {
            <PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        fn system_para_to_para_reserve_transfer_assets(
            t: SystemParaToParaTest,
        ) -> DispatchResult {
            <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        fn para_to_system_para_reserve_transfer_assets(
            t: ParaToSystemParaTest,
        ) -> DispatchResult {
            <PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        fn para_to_para_through_relay_limited_reserve_transfer_assets(
            t: ParaToParaThroughRelayTest,
        ) -> DispatchResult {
            <PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_relay_to_asset_hub_fails"]
        pub const reserve_transfer_native_asset_from_relay_to_asset_hub_fails: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_relay_to_asset_hub_fails",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 498usize,
                start_col: 4usize,
                end_line: 498usize,
                end_col: 63usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_relay_to_asset_hub_fails(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Relay Chain to the Asset Hub shouldn't work
        fn reserve_transfer_native_asset_from_relay_to_asset_hub_fails() {
            let signed_origin = <Westend as Chain>::RuntimeOrigin::signed(
                WestendSender::get().into(),
            );
            let destination = Westend::child_location_of(AssetHubWestend::para_id());
            let beneficiary: Location = AccountId32Junction {
                network: None,
                id: AssetHubWestendReceiver::get().into(),
            }
                .into();
            let amount_to_send: Balance = WESTEND_ED * 1000;
            let assets: Assets = (Here, amount_to_send).into();
            let fee_asset_item = 0;
            Westend::execute_with(|| {
                let result = <Westend as WestendPallet>::XcmPallet::limited_reserve_transfer_assets(
                    signed_origin,
                    Box::new(destination.into()),
                    Box::new(beneficiary.into()),
                    Box::new(assets.into()),
                    fee_asset_item,
                    WeightLimit::Unlimited,
                );
                match (
                    &result,
                    &Err(
                        DispatchError::Module(sp_runtime::ModuleError {
                                index: 99,
                                error: [2, 0, 0, 0],
                                message: Some("Filtered"),
                            })
                            .into(),
                    ),
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_asset_hub_to_relay_fails"]
        pub const reserve_transfer_native_asset_from_asset_hub_to_relay_fails: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_asset_hub_to_relay_fails",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 531usize,
                start_col: 4usize,
                end_line: 531usize,
                end_col: 63usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_asset_hub_to_relay_fails(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Asset Hub to Relay Chain shouldn't work
        fn reserve_transfer_native_asset_from_asset_hub_to_relay_fails() {
            let signed_origin = <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                AssetHubWestendSender::get().into(),
            );
            let destination = AssetHubWestend::parent_location();
            let beneficiary_id = WestendReceiver::get();
            let beneficiary: Location = AccountId32Junction {
                network: None,
                id: beneficiary_id.into(),
            }
                .into();
            let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
            let assets: Assets = (Parent, amount_to_send).into();
            let fee_asset_item = 0;
            AssetHubWestend::execute_with(|| {
                let result = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
                    signed_origin,
                    Box::new(destination.into()),
                    Box::new(beneficiary.into()),
                    Box::new(assets.into()),
                    fee_asset_item,
                    WeightLimit::Unlimited,
                );
                match (
                    &result,
                    &Err(
                        DispatchError::Module(sp_runtime::ModuleError {
                                index: 31,
                                error: [2, 0, 0, 0],
                                message: Some("Filtered"),
                            })
                            .into(),
                    ),
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_relay_to_para"]
        pub const reserve_transfer_native_asset_from_relay_to_para: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_relay_to_para",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 571usize,
                start_col: 4usize,
                end_line: 571usize,
                end_col: 52usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_relay_to_para(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Relay to Parachain should work
        fn reserve_transfer_native_asset_from_relay_to_para() {
            let destination = Westend::child_location_of(PenpalA::para_id());
            let sender = WestendSender::get();
            let amount_to_send: Balance = WESTEND_ED * 1000;
            let relay_native_asset_location = RelayLocation::get();
            let receiver = PenpalAReceiver::get();
            let test_args = TestContext {
                sender,
                receiver: receiver.clone(),
                args: TestArgs::new_relay(
                    destination.clone(),
                    receiver.clone(),
                    amount_to_send,
                ),
            };
            let mut test = RelayToParaTest::new(test_args);
            let sender_balance_before = test.sender.balance;
            let receiver_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &receiver)
            });
            test.set_assertion::<Westend>(relay_to_para_sender_assertions);
            test.set_assertion::<PenpalA>(relay_to_para_assets_receiver_assertions);
            test.set_dispatchable::<Westend>(relay_to_para_reserve_transfer_assets);
            test.assert();
            let sender_balance_after = test.sender.balance;
            let receiver_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location, &receiver)
            });
            if !(sender_balance_after < sender_balance_before - amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_balance_after < sender_balance_before - amount_to_send",
                )
            }
            if !(receiver_assets_after > receiver_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after > receiver_assets_before",
                )
            }
            if !(receiver_assets_after < receiver_assets_before + amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after < receiver_assets_before + amount_to_send",
                )
            }
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_para_to_relay"]
        pub const reserve_transfer_native_asset_from_para_to_relay: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_para_to_relay",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 621usize,
                start_col: 4usize,
                end_line: 621usize,
                end_col: 52usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_para_to_relay(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Parachain to Relay should work
        fn reserve_transfer_native_asset_from_para_to_relay() {
            let destination = PenpalA::parent_location();
            let sender = PenpalASender::get();
            let amount_to_send: Balance = WESTEND_ED * 1000;
            let assets: Assets = (Parent, amount_to_send).into();
            let asset_owner = PenpalAssetOwner::get();
            let relay_native_asset_location = RelayLocation::get();
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
                relay_native_asset_location.clone(),
                sender.clone(),
                amount_to_send * 2,
            );
            let receiver = WestendReceiver::get();
            let penpal_location_as_seen_by_relay = Westend::child_location_of(
                PenpalA::para_id(),
            );
            let sov_penpal_on_relay = Westend::sovereign_account_id_of(
                penpal_location_as_seen_by_relay,
            );
            Westend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_penpal_on_relay.into(), amount_to_send * 2),
                    ]),
                ),
            );
            let test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination.clone(),
                    receiver,
                    amount_to_send,
                    assets.clone(),
                    None,
                    0,
                ),
            };
            let mut test = ParaToRelayTest::new(test_args);
            let sender_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &sender)
            });
            let receiver_balance_before = test.receiver.balance;
            test.set_assertion::<PenpalA>(para_to_relay_sender_assertions);
            test.set_assertion::<Westend>(para_to_relay_receiver_assertions);
            test.set_dispatchable::<PenpalA>(para_to_relay_reserve_transfer_assets);
            test.assert();
            let sender_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location, &sender)
            });
            let receiver_balance_after = test.receiver.balance;
            if !(sender_assets_after < sender_assets_before - amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_assets_after < sender_assets_before - amount_to_send",
                )
            }
            if !(receiver_balance_after > receiver_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_balance_after > receiver_balance_before",
                )
            }
            if !(receiver_balance_after < receiver_balance_before + amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: receiver_balance_after < receiver_balance_before + amount_to_send",
                )
            }
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_asset_hub_to_para"]
        pub const reserve_transfer_native_asset_from_asset_hub_to_para: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_asset_hub_to_para",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 696usize,
                start_col: 4usize,
                end_line: 696usize,
                end_col: 56usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_asset_hub_to_para(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Asset Hub to Parachain should work
        fn reserve_transfer_native_asset_from_asset_hub_to_para() {
            let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
            let sender = AssetHubWestendSender::get();
            let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 2000;
            let assets: Assets = (Parent, amount_to_send).into();
            let system_para_native_asset_location = RelayLocation::get();
            let receiver = PenpalAReceiver::get();
            let test_args = TestContext {
                sender,
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination.clone(),
                    receiver.clone(),
                    amount_to_send,
                    assets.clone(),
                    None,
                    0,
                ),
            };
            let mut test = SystemParaToParaTest::new(test_args);
            let sender_balance_before = test.sender.balance;
            let receiver_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location.clone(), &receiver)
            });
            test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
            test.set_assertion::<PenpalA>(system_para_to_para_receiver_assertions);
            test.set_dispatchable::<
                    AssetHubWestend,
                >(system_para_to_para_reserve_transfer_assets);
            test.assert();
            let sender_balance_after = test.sender.balance;
            let receiver_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location, &receiver)
            });
            if !(sender_balance_after < sender_balance_before - amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_balance_after < sender_balance_before - amount_to_send",
                )
            }
            if !(receiver_assets_after > receiver_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after > receiver_assets_before",
                )
            }
            if !(receiver_assets_after < receiver_assets_before + amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after < receiver_assets_before + amount_to_send",
                )
            }
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_para_to_asset_hub"]
        pub const reserve_transfer_native_asset_from_para_to_asset_hub: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_para_to_asset_hub",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 754usize,
                start_col: 4usize,
                end_line: 754usize,
                end_col: 56usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_para_to_asset_hub(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Parachain to Asset Hub should work
        fn reserve_transfer_native_asset_from_para_to_asset_hub() {
            let destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
            let sender = PenpalASender::get();
            let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
            let assets: Assets = (Parent, amount_to_send).into();
            let system_para_native_asset_location = RelayLocation::get();
            let asset_owner = PenpalAssetOwner::get();
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
                system_para_native_asset_location.clone(),
                sender.clone(),
                amount_to_send * 2,
            );
            let receiver = AssetHubWestendReceiver::get();
            let penpal_location_as_seen_by_ahr = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                penpal_location_as_seen_by_ahr,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_penpal_on_ahr.into(), amount_to_send * 2),
                    ]),
                ),
            );
            let test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination.clone(),
                    receiver.clone(),
                    amount_to_send,
                    assets.clone(),
                    None,
                    0,
                ),
            };
            let mut test = ParaToSystemParaTest::new(test_args);
            let sender_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location.clone(), &sender)
            });
            let receiver_balance_before = test.receiver.balance;
            test.set_assertion::<PenpalA>(para_to_system_para_sender_assertions);
            test.set_assertion::<
                    AssetHubWestend,
                >(para_to_system_para_receiver_assertions);
            test.set_dispatchable::<
                    PenpalA,
                >(para_to_system_para_reserve_transfer_assets);
            test.assert();
            let sender_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location, &sender)
            });
            let receiver_balance_after = test.receiver.balance;
            if !(sender_assets_after < sender_assets_before - amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_assets_after < sender_assets_before - amount_to_send",
                )
            }
            if !(receiver_balance_after > receiver_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_balance_after > receiver_balance_before",
                )
            }
            if !(receiver_balance_after < receiver_balance_before + amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: receiver_balance_after < receiver_balance_before + amount_to_send",
                )
            }
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_multiple_assets_from_asset_hub_to_para"]
        pub const reserve_transfer_multiple_assets_from_asset_hub_to_para: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_multiple_assets_from_asset_hub_to_para",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 831usize,
                start_col: 4usize,
                end_line: 831usize,
                end_col: 59usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_multiple_assets_from_asset_hub_to_para(),
                ),
            ),
        };
        /// Reserve Transfers of a local asset and native asset from Asset Hub to Parachain should
        /// work
        fn reserve_transfer_multiple_assets_from_asset_hub_to_para() {
            let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                destination.clone(),
            );
            let sender = AssetHubWestendSender::get();
            let fee_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
            let asset_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
            let asset_owner = AssetHubWestendAssetOwner::get();
            let asset_owner_signer = <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                asset_owner.clone(),
            );
            let assets: Assets = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (Parent, fee_amount_to_send).into(),
                        (
                            [
                                PalletInstance(ASSETS_PALLET_ID),
                                GeneralIndex(RESERVABLE_ASSET_ID.into()),
                            ],
                            asset_amount_to_send,
                        )
                            .into(),
                    ]),
                )
                .into();
            let fee_asset_index = assets
                .inner()
                .iter()
                .position(|r| r == &(Parent, fee_amount_to_send).into())
                .unwrap() as u32;
            AssetHubWestend::mint_asset(
                asset_owner_signer,
                RESERVABLE_ASSET_ID,
                asset_owner,
                asset_amount_to_send * 2,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_penpal_on_ahr.into(), ASSET_HUB_WESTEND_ED),
                    ]),
                ),
            );
            let receiver = PenpalAReceiver::get();
            let system_para_native_asset_location = RelayLocation::get();
            let system_para_foreign_asset_location = PenpalLocalReservableFromAssetHub::get();
            let para_test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination,
                    receiver.clone(),
                    asset_amount_to_send,
                    assets,
                    None,
                    fee_asset_index,
                ),
            };
            let mut test = SystemParaToParaTest::new(para_test_args);
            let sender_balance_before = test.sender.balance;
            let sender_assets_before = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                <Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &sender)
            });
            let receiver_system_native_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location.clone(), &receiver)
            });
            let receiver_foreign_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_foreign_asset_location.clone(), &receiver)
            });
            test.set_assertion::<
                    AssetHubWestend,
                >(system_para_to_para_assets_sender_assertions);
            test.set_assertion::<
                    PenpalA,
                >(system_para_to_para_assets_receiver_assertions);
            test.set_dispatchable::<
                    AssetHubWestend,
                >(system_para_to_para_reserve_transfer_assets);
            test.assert();
            let sender_balance_after = test.sender.balance;
            let sender_assets_after = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                <Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &sender)
            });
            let receiver_system_native_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location, &receiver)
            });
            let receiver_foreign_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_foreign_asset_location, &receiver)
            });
            if !(sender_balance_after < sender_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: sender_balance_after < sender_balance_before",
                )
            }
            if !(receiver_foreign_assets_after > receiver_foreign_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_foreign_assets_after > receiver_foreign_assets_before",
                )
            }
            if !(receiver_system_native_assets_after
                < receiver_system_native_assets_before + fee_amount_to_send)
            {
                ::core::panicking::panic(
                    "assertion failed: receiver_system_native_assets_after <\n    receiver_system_native_assets_before + fee_amount_to_send",
                )
            }
            match (
                &(sender_assets_before - asset_amount_to_send),
                &sender_assets_after,
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &receiver_foreign_assets_after,
                &(receiver_foreign_assets_before + asset_amount_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_multiple_assets_from_para_to_asset_hub"]
        pub const reserve_transfer_multiple_assets_from_para_to_asset_hub: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_multiple_assets_from_para_to_asset_hub",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 948usize,
                start_col: 4usize,
                end_line: 948usize,
                end_col: 59usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_multiple_assets_from_para_to_asset_hub(),
                ),
            ),
        };
        /// Reserve Transfers of a random asset and native asset from Parachain to Asset Hub should work
        /// Receiver is empty account to show deposit works as long as transfer includes enough DOT for ED.
        /// Once we have https://github.com/paritytech/polkadot-sdk/issues/5298,
        /// we should do equivalent test with USDT instead of DOT.
        fn reserve_transfer_multiple_assets_from_para_to_asset_hub() {
            let destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
            let sender = PenpalASender::get();
            let fee_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
            let asset_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
            let penpal_asset_owner = PenpalAssetOwner::get();
            let penpal_asset_owner_signer = <PenpalA as Chain>::RuntimeOrigin::signed(
                penpal_asset_owner,
            );
            let asset_location_on_penpal = PenpalLocalReservableFromAssetHub::get();
            let system_asset_location_on_penpal = RelayLocation::get();
            let assets: Assets = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (Parent, fee_amount_to_send).into(),
                        (asset_location_on_penpal.clone(), asset_amount_to_send).into(),
                    ]),
                )
                .into();
            let fee_asset_index = assets
                .inner()
                .iter()
                .position(|r| r == &(Parent, fee_amount_to_send).into())
                .unwrap() as u32;
            PenpalA::mint_foreign_asset(
                penpal_asset_owner_signer.clone(),
                asset_location_on_penpal.clone(),
                sender.clone(),
                asset_amount_to_send * 2,
            );
            PenpalA::mint_foreign_asset(
                penpal_asset_owner_signer,
                system_asset_location_on_penpal.clone(),
                sender.clone(),
                fee_amount_to_send * 2,
            );
            let receiver = get_account_id_from_seed::<
                sp_runtime::testing::sr25519::Public,
            >(DUMMY_EMPTY);
            let penpal_location_as_seen_by_ahr = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                penpal_location_as_seen_by_ahr,
            );
            let ah_asset_owner = AssetHubWestendAssetOwner::get();
            let ah_asset_owner_signer = <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                ah_asset_owner,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_penpal_on_ahr.clone().into(), ASSET_HUB_WESTEND_ED * 1000),
                    ]),
                ),
            );
            AssetHubWestend::mint_asset(
                ah_asset_owner_signer,
                RESERVABLE_ASSET_ID,
                sov_penpal_on_ahr,
                asset_amount_to_send * 2,
            );
            let para_test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination,
                    receiver.clone(),
                    asset_amount_to_send,
                    assets,
                    None,
                    fee_asset_index,
                ),
            };
            let mut test = ParaToSystemParaTest::new(para_test_args);
            let sender_system_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_asset_location_on_penpal.clone(), &sender)
            });
            let sender_foreign_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(asset_location_on_penpal.clone(), &sender)
            });
            let receiver_balance_before = test.receiver.balance;
            let receiver_assets_before = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                <Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &receiver)
            });
            test.set_assertion::<PenpalA>(para_to_system_para_assets_sender_assertions);
            test.set_assertion::<
                    AssetHubWestend,
                >(para_to_system_para_assets_receiver_assertions);
            test.set_dispatchable::<
                    PenpalA,
                >(para_to_system_para_reserve_transfer_assets);
            test.assert();
            let sender_system_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_asset_location_on_penpal, &sender)
            });
            let sender_foreign_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<_>>::balance(asset_location_on_penpal, &sender)
            });
            let receiver_balance_after = test.receiver.balance;
            let receiver_assets_after = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                <Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &receiver)
            });
            if !(sender_system_assets_after < sender_system_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: sender_system_assets_after < sender_system_assets_before",
                )
            }
            if !(receiver_balance_after > receiver_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_balance_after > receiver_balance_before",
                )
            }
            if !(receiver_balance_after < receiver_balance_before + fee_amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: receiver_balance_after < receiver_balance_before + fee_amount_to_send",
                )
            }
            match (
                &(sender_foreign_assets_before - asset_amount_to_send),
                &sender_foreign_assets_after,
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &receiver_assets_after,
                &(receiver_assets_before + asset_amount_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::reserve_transfer::reserve_transfer_native_asset_from_para_to_para_through_relay"]
        pub const reserve_transfer_native_asset_from_para_to_para_through_relay: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::reserve_transfer::reserve_transfer_native_asset_from_para_to_para_through_relay",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/reserve_transfer.rs",
                start_line: 1076usize,
                start_col: 4usize,
                end_line: 1076usize,
                end_col: 65usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    reserve_transfer_native_asset_from_para_to_para_through_relay(),
                ),
            ),
        };
        /// Reserve Transfers of native asset from Parachain to Parachain (through Relay reserve) should
        /// work
        fn reserve_transfer_native_asset_from_para_to_para_through_relay() {
            let destination = PenpalA::sibling_location_of(PenpalB::para_id());
            let sender = PenpalASender::get();
            let amount_to_send: Balance = WESTEND_ED * 10000;
            let asset_owner = PenpalAssetOwner::get();
            let assets = (Parent, amount_to_send).into();
            let relay_native_asset_location = RelayLocation::get();
            let sender_as_seen_by_relay = Westend::child_location_of(PenpalA::para_id());
            let sov_of_sender_on_relay = Westend::sovereign_account_id_of(
                sender_as_seen_by_relay,
            );
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
                relay_native_asset_location.clone(),
                sender.clone(),
                amount_to_send * 2,
            );
            Westend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_of_sender_on_relay.into(), amount_to_send * 2),
                    ]),
                ),
            );
            let receiver = PenpalBReceiver::get();
            let test_args = TestContext {
                sender: sender.clone(),
                receiver: receiver.clone(),
                args: TestArgs::new_para(
                    destination,
                    receiver.clone(),
                    amount_to_send,
                    assets,
                    None,
                    0,
                ),
            };
            let mut test = ParaToParaThroughRelayTest::new(test_args);
            let sender_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &sender)
            });
            let receiver_assets_before = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &receiver)
            });
            test.set_assertion::<PenpalA>(para_to_para_through_hop_sender_assertions);
            test.set_assertion::<Westend>(para_to_para_relay_hop_assertions);
            test.set_assertion::<PenpalB>(para_to_para_through_hop_receiver_assertions);
            test.set_dispatchable::<
                    PenpalA,
                >(para_to_para_through_relay_limited_reserve_transfer_assets);
            test.assert();
            let sender_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &sender)
            });
            let receiver_assets_after = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location, &receiver)
            });
            if !(sender_assets_after < sender_assets_before - amount_to_send) {
                ::core::panicking::panic(
                    "assertion failed: sender_assets_after < sender_assets_before - amount_to_send",
                )
            }
            if !(receiver_assets_after > receiver_assets_before) {
                ::core::panicking::panic(
                    "assertion failed: receiver_assets_after > receiver_assets_before",
                )
            }
        }
    }
    mod send {
        use crate::imports::*;
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::send::send_transact_as_superuser_from_relay_to_asset_hub_works"]
        pub const send_transact_as_superuser_from_relay_to_asset_hub_works: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::send::send_transact_as_superuser_from_relay_to_asset_hub_works",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/send.rs",
                start_line: 21usize,
                start_col: 4usize,
                end_line: 21usize,
                end_col: 60usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    send_transact_as_superuser_from_relay_to_asset_hub_works(),
                ),
            ),
        };
        /// Relay Chain should be able to execute `Transact` instructions in System Parachain
        /// when `OriginKind::Superuser`.
        fn send_transact_as_superuser_from_relay_to_asset_hub_works() {
            AssetHubWestend::force_create_asset_from_relay_as_root(
                ASSET_ID,
                ASSET_MIN_BALANCE,
                true,
                AssetHubWestendSender::get().into(),
                Some(Weight::from_parts(1_019_445_000, 200_000)),
            )
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::send::send_xcm_from_para_to_asset_hub_paying_fee_with_system_asset"]
        pub const send_xcm_from_para_to_asset_hub_paying_fee_with_system_asset: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::send::send_xcm_from_para_to_asset_hub_paying_fee_with_system_asset",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/send.rs",
                start_line: 35usize,
                start_col: 4usize,
                end_line: 35usize,
                end_col: 64usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    send_xcm_from_para_to_asset_hub_paying_fee_with_system_asset(),
                ),
            ),
        };
        /// We tests two things here:
        /// - Parachain should be able to send XCM paying its fee at Asset Hub using system asset
        /// - Parachain should be able to create a new Foreign Asset at Asset Hub
        fn send_xcm_from_para_to_asset_hub_paying_fee_with_system_asset() {
            let para_sovereign_account = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            let asset_location_on_penpal = Location::new(
                0,
                [
                    Junction::PalletInstance(ASSETS_PALLET_ID),
                    Junction::GeneralIndex(ASSET_ID.into()),
                ],
            );
            let foreign_asset_at_asset_hub = Location::new(
                    1,
                    [Junction::Parachain(PenpalA::para_id().into())],
                )
                .appended_with(asset_location_on_penpal)
                .unwrap();
            let call = AssetHubWestend::create_foreign_asset_call(
                foreign_asset_at_asset_hub.clone(),
                ASSET_MIN_BALANCE,
                para_sovereign_account.clone(),
            );
            let origin_kind = OriginKind::Xcm;
            let fee_amount = ASSET_HUB_WESTEND_ED * 1000000;
            let system_asset = (Parent, fee_amount).into();
            let root_origin = <PenpalA as Chain>::RuntimeOrigin::root();
            let system_para_destination = PenpalA::sibling_location_of(
                    AssetHubWestend::para_id(),
                )
                .into();
            let xcm = xcm_transact_paid_execution(
                call,
                origin_kind,
                system_asset,
                para_sovereign_account.clone(),
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (
                            para_sovereign_account.clone().into(),
                            ASSET_HUB_WESTEND_ED * 10000000000,
                        ),
                    ]),
                ),
            );
            PenpalA::execute_with(|| {
                let is = <PenpalA as PenpalAPallet>::PolkadotXcm::send(
                    root_origin,
                    Box::new(system_para_destination),
                    Box::new(xcm),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                PenpalA::assert_xcm_pallet_sent();
            });
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                AssetHubWestend::assert_xcmp_queue_success(None);
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Balances(
                            pallet_balances::Event::Burned { who, amount },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*who == para_sovereign_account)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "who",
                                                who,
                                                "*who == para_sovereign_account",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *who == para_sovereign_account;
                            if !(*amount == fee_amount) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount",
                                                amount,
                                                "*amount == fee_amount",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount == fee_amount;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Created { asset_id, creator, owner },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == foreign_asset_at_asset_hub)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == foreign_asset_at_asset_hub",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == foreign_asset_at_asset_hub;
                            if !(*creator == para_sovereign_account.clone())
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "creator",
                                                creator,
                                                "*creator == para_sovereign_account.clone()",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions
                                &= *creator == para_sovereign_account.clone();
                            if !(*owner == para_sovereign_account)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == para_sovereign_account",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == para_sovereign_account;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Created {\nasset_id, creator, owner })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Created {\nasset_id, creator, owner })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::send",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                if !ForeignAssets::asset_exists(foreign_asset_at_asset_hub) {
                    ::core::panicking::panic(
                        "assertion failed: ForeignAssets::asset_exists(foreign_asset_at_asset_hub)",
                    )
                }
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::send::send_xcm_from_para_to_asset_hub_paying_fee_with_sufficient_asset"]
        pub const send_xcm_from_para_to_asset_hub_paying_fee_with_sufficient_asset: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::send::send_xcm_from_para_to_asset_hub_paying_fee_with_sufficient_asset",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/send.rs",
                start_line: 113usize,
                start_col: 4usize,
                end_line: 113usize,
                end_col: 68usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    send_xcm_from_para_to_asset_hub_paying_fee_with_sufficient_asset(),
                ),
            ),
        };
        /// We tests two things here:
        /// - Parachain should be able to send XCM paying its fee at Asset Hub using sufficient asset
        /// - Parachain should be able to create a new Asset at Asset Hub
        fn send_xcm_from_para_to_asset_hub_paying_fee_with_sufficient_asset() {
            let para_sovereign_account = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            AssetHubWestend::force_create_and_mint_asset(
                ASSET_ID,
                ASSET_MIN_BALANCE,
                true,
                para_sovereign_account.clone(),
                Some(Weight::from_parts(1_019_445_000, 200_000)),
                ASSET_MIN_BALANCE * 1000000000,
            );
            let new_asset_id = ASSET_ID + 1;
            let call = AssetHubWestend::create_asset_call(
                new_asset_id,
                ASSET_MIN_BALANCE,
                para_sovereign_account.clone(),
            );
            let origin_kind = OriginKind::SovereignAccount;
            let fee_amount = ASSET_MIN_BALANCE * 1000000;
            let asset = (
                [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())],
                fee_amount,
            )
                .into();
            let root_origin = <PenpalA as Chain>::RuntimeOrigin::root();
            let system_para_destination = PenpalA::sibling_location_of(
                    AssetHubWestend::para_id(),
                )
                .into();
            let xcm = xcm_transact_paid_execution(
                call,
                origin_kind,
                asset,
                para_sovereign_account.clone(),
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (
                            para_sovereign_account.clone().into(),
                            ASSET_HUB_WESTEND_ED * 10000000000,
                        ),
                    ]),
                ),
            );
            PenpalA::execute_with(|| {
                let is = <PenpalA as PenpalAPallet>::PolkadotXcm::send(
                    root_origin,
                    Box::new(system_para_destination),
                    Box::new(xcm),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                PenpalA::assert_xcm_pallet_sent();
            });
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                AssetHubWestend::assert_xcmp_queue_success(None);
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Assets(
                            pallet_assets::Event::Burned { asset_id, owner, balance },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == ASSET_ID) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == ASSET_ID",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == ASSET_ID;
                            if !(*owner == para_sovereign_account)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == para_sovereign_account",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == para_sovereign_account;
                            if !(*balance == fee_amount) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "balance",
                                                balance,
                                                "*balance == fee_amount",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *balance == fee_amount;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Assets(
                            pallet_assets::Event::Created { asset_id, creator, owner },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*asset_id == new_asset_id) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "asset_id",
                                                asset_id,
                                                "*asset_id == new_asset_id",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *asset_id == new_asset_id;
                            if !(*creator == para_sovereign_account.clone())
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "creator",
                                                creator,
                                                "*creator == para_sovereign_account.clone()",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions
                                &= *creator == para_sovereign_account.clone();
                            if !(*owner == para_sovereign_account)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "owner",
                                                owner,
                                                "*owner == para_sovereign_account",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *owner == para_sovereign_account;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Created { asset_id, creator, owner\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Created { asset_id, creator, owner\n})",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::send",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
        }
    }
    mod set_xcm_versions {
        use crate::imports::*;
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::set_xcm_versions::relay_sets_system_para_xcm_supported_version"]
        pub const relay_sets_system_para_xcm_supported_version: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::set_xcm_versions::relay_sets_system_para_xcm_supported_version",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/set_xcm_versions.rs",
                start_line: 19usize,
                start_col: 4usize,
                end_line: 19usize,
                end_col: 48usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    relay_sets_system_para_xcm_supported_version(),
                ),
            ),
        };
        fn relay_sets_system_para_xcm_supported_version() {
            let sudo_origin = <Westend as Chain>::RuntimeOrigin::root();
            let system_para_destination: Location = Westend::child_location_of(
                AssetHubWestend::para_id(),
            );
            Westend::execute_with(|| {
                let is = <Westend as WestendPallet>::XcmPallet::force_xcm_version(
                    sudo_origin,
                    Box::new(system_para_destination.clone()),
                    XCM_V3,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
                let mut message: Vec<String> = Vec::new();
                let mut events = <Westend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::XcmPallet(
                            pallet_xcm::Event::SupportedVersionChanged {
                                location,
                                version: XCM_V3,
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*location == system_para_destination)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "location",
                                                location,
                                                "*location == system_para_destination",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *location == system_para_destination;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::XcmPallet(pallet_xcm::Event::SupportedVersionChanged {\nlocation, version: XCM_V3 })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::XcmPallet(pallet_xcm::Event::SupportedVersionChanged {\nlocation, version: XCM_V3 })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <Westend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::Westend",
                                            "asset_hub_westend_integration_tests::tests::set_xcm_versions",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::set_xcm_versions::system_para_sets_relay_xcm_supported_version"]
        pub const system_para_sets_relay_xcm_supported_version: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::set_xcm_versions::system_para_sets_relay_xcm_supported_version",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/set_xcm_versions.rs",
                start_line: 47usize,
                start_col: 4usize,
                end_line: 47usize,
                end_col: 48usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    system_para_sets_relay_xcm_supported_version(),
                ),
            ),
        };
        fn system_para_sets_relay_xcm_supported_version() {
            let parent_location = AssetHubWestend::parent_location();
            let force_xcm_version_call = <AssetHubWestend as Chain>::RuntimeCall::PolkadotXcm(pallet_xcm::Call::<
                    <AssetHubWestend as Chain>::Runtime,
                >::force_xcm_version {
                    location: Box::new(parent_location.clone()),
                    version: XCM_V3,
                })
                .encode()
                .into();
            Westend::send_unpaid_transact_to_parachain_as_root(
                AssetHubWestend::para_id(),
                force_xcm_version_call,
            );
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                AssetHubWestend::assert_dmp_queue_complete(
                    Some(Weight::from_parts(1_019_210_000, 200_000)),
                );
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::PolkadotXcm(
                            pallet_xcm::Event::SupportedVersionChanged {
                                location,
                                version: XCM_V3,
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*location == parent_location)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "location",
                                                location,
                                                "*location == parent_location",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *location == parent_location;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::SupportedVersionChanged {\nlocation, version: XCM_V3 })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(pallet_xcm::Event::SupportedVersionChanged {\nlocation, version: XCM_V3 })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::set_xcm_versions",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
        }
    }
    mod swap {
        use crate::imports::*;
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::swap::swap_locally_on_chain_using_local_assets"]
        pub const swap_locally_on_chain_using_local_assets: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::swap::swap_locally_on_chain_using_local_assets",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/swap.rs",
                start_line: 19usize,
                start_col: 4usize,
                end_line: 19usize,
                end_col: 44usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(swap_locally_on_chain_using_local_assets()),
            ),
        };
        fn swap_locally_on_chain_using_local_assets() {
            let asset_native = Box::new(
                Location::try_from(RelayLocation::get()).expect("conversion works"),
            );
            let asset_one = Box::new(Location {
                parents: 0,
                interior: [
                    Junction::PalletInstance(ASSETS_PALLET_ID),
                    Junction::GeneralIndex(ASSET_ID.into()),
                ]
                    .into(),
            });
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                let is = <AssetHubWestend as AssetHubWestendPallet>::Assets::create(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    ASSET_ID.into(),
                    AssetHubWestendSender::get().into(),
                    1000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                if !<AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(
                    ASSET_ID,
                ) {
                    ::core::panicking::panic(
                        "assertion failed: <AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(ASSET_ID)",
                    )
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    ASSET_ID.into(),
                    AssetHubWestendSender::get().into(),
                    3_000_000_000_000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    asset_native.clone(),
                    asset_one.clone(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::PoolCreated { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { ..\n})",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    asset_native.clone(),
                    asset_one.clone(),
                    1_000_000_000_000,
                    2_000_000_000_000,
                    0,
                    0,
                    AssetHubWestendSender::get().into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::LiquidityAdded {
                                lp_token_minted,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*lp_token_minted == 1414213562273)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "lp_token_minted",
                                                lp_token_minted,
                                                "*lp_token_minted == 1414213562273",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *lp_token_minted == 1414213562273;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {\nlp_token_minted, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {\nlp_token_minted, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let path = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([asset_native.clone(), asset_one.clone()]),
                );
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::swap_exact_tokens_for_tokens(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    path,
                    100,
                    1,
                    AssetHubWestendSender::get().into(),
                    true,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::SwapExecuted {
                                amount_in,
                                amount_out,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*amount_in == 100) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount_in",
                                                amount_in,
                                                "*amount_in == 100",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount_in == 100;
                            if !(*amount_out == 199) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount_out",
                                                amount_out,
                                                "*amount_out == 199",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount_out == 199;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted {\namount_in, amount_out, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted {\namount_in, amount_out, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::remove_liquidity(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    asset_native.clone(),
                    asset_one.clone(),
                    1414213562273 - 2_000_000_000,
                    0,
                    0,
                    AssetHubWestendSender::get().into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::swap::swap_locally_on_chain_using_foreign_assets"]
        pub const swap_locally_on_chain_using_foreign_assets: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::swap::swap_locally_on_chain_using_foreign_assets",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/swap.rs",
                start_line: 114usize,
                start_col: 4usize,
                end_line: 114usize,
                end_col: 46usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(swap_locally_on_chain_using_foreign_assets()),
            ),
        };
        fn swap_locally_on_chain_using_foreign_assets() {
            let asset_native = Box::new(
                Location::try_from(RelayLocation::get()).unwrap(),
            );
            let asset_location_on_penpal = Location::try_from(
                    PenpalLocalTeleportableToAssetHub::get(),
                )
                .expect("conversion_works");
            let foreign_asset_at_asset_hub_westend = Location::new(
                    1,
                    [Junction::Parachain(PenpalA::para_id().into())],
                )
                .appended_with(asset_location_on_penpal)
                .unwrap();
            let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                penpal_as_seen_by_ah,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (
                            AssetHubWestendSender::get().into(),
                            5_000_000 * ASSET_HUB_WESTEND_ED,
                        ),
                        (
                            sov_penpal_on_ahr.clone().into(),
                            100_000_000 * ASSET_HUB_WESTEND_ED,
                        ),
                    ]),
                ),
            );
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                let is = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        sov_penpal_on_ahr.clone().into(),
                    ),
                    foreign_asset_at_asset_hub_westend.clone(),
                    sov_penpal_on_ahr.clone().into(),
                    ASSET_HUB_WESTEND_ED * 3_000_000_000_000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ForeignAssets(
                            pallet_assets::Event::Issued { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    asset_native.clone(),
                    Box::new(foreign_asset_at_asset_hub_westend.clone()),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::PoolCreated { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { ..\n})",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        sov_penpal_on_ahr.clone(),
                    ),
                    asset_native.clone(),
                    Box::new(foreign_asset_at_asset_hub_westend.clone()),
                    1_000_000_000_000_000,
                    2_000_000_000_000_000,
                    0,
                    0,
                    sov_penpal_on_ahr.clone().into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::LiquidityAdded {
                                lp_token_minted,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*lp_token_minted == 1414213562372995)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "lp_token_minted",
                                                lp_token_minted,
                                                "*lp_token_minted == 1414213562372995",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *lp_token_minted == 1414213562372995;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {\nlp_token_minted, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {\nlp_token_minted, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let path = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        asset_native.clone(),
                        Box::new(foreign_asset_at_asset_hub_westend.clone()),
                    ]),
                );
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::swap_exact_tokens_for_tokens(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    path,
                    100000 * ASSET_HUB_WESTEND_ED,
                    1000 * ASSET_HUB_WESTEND_ED,
                    AssetHubWestendSender::get().into(),
                    true,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::SwapExecuted {
                                amount_in,
                                amount_out,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*amount_in == 100000000000000)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount_in",
                                                amount_in,
                                                "*amount_in == 100000000000000",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount_in == 100000000000000;
                            if !(*amount_out == 181322178776029)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount_out",
                                                amount_out,
                                                "*amount_out == 181322178776029",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *amount_out == 181322178776029;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted {\namount_in, amount_out, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted {\namount_in, amount_out, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::remove_liquidity(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        sov_penpal_on_ahr.clone(),
                    ),
                    asset_native.clone(),
                    Box::new(foreign_asset_at_asset_hub_westend),
                    1414213562372995 - ASSET_HUB_WESTEND_ED * 2,
                    0,
                    0,
                    sov_penpal_on_ahr.clone().into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::swap::cannot_create_pool_from_pool_assets"]
        pub const cannot_create_pool_from_pool_assets: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::swap::cannot_create_pool_from_pool_assets",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/swap.rs",
                start_line: 229usize,
                start_col: 4usize,
                end_line: 229usize,
                end_col: 39usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(cannot_create_pool_from_pool_assets()),
            ),
        };
        fn cannot_create_pool_from_pool_assets() {
            let asset_native = RelayLocation::get();
            let mut asset_one = ahw_xcm_config::PoolAssetsPalletLocation::get();
            asset_one.append_with(GeneralIndex(ASSET_ID.into())).expect("pool assets");
            AssetHubWestend::execute_with(|| {
                let pool_owner_account_id = AssetHubWestendAssetConversionOrigin::get();
                let is = <AssetHubWestend as AssetHubWestendPallet>::PoolAssets::create(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        pool_owner_account_id.clone(),
                    ),
                    ASSET_ID.into(),
                    pool_owner_account_id.clone().into(),
                    1000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                if !<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::asset_exists(
                    ASSET_ID,
                ) {
                    ::core::panicking::panic(
                        "assertion failed: <AssetHubWestend as AssetHubWestendPallet>::PoolAssets::asset_exists(ASSET_ID)",
                    )
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::PoolAssets::mint(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        pool_owner_account_id,
                    ),
                    ASSET_ID.into(),
                    AssetHubWestendSender::get().into(),
                    3_000_000_000_000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                match <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    Box::new(
                        Location::try_from(asset_native).expect("conversion works"),
                    ),
                    Box::new(Location::try_from(asset_one).expect("conversion works")),
                ) {
                    Err(
                        DispatchError::Module(
                            ModuleError { index: _, error: _, message },
                        ),
                    ) => {
                        match (&message, &Some("Unknown")) {
                            (left_val, right_val) => {
                                if !(*left_val == *right_val) {
                                    let kind = ::core::panicking::AssertKind::Eq;
                                    ::core::panicking::assert_failed(
                                        kind,
                                        &*left_val,
                                        &*right_val,
                                        ::core::option::Option::None,
                                    );
                                }
                            }
                        }
                    }
                    ref e => {
                        ::std::rt::panic_fmt(
                            format_args!(
                                "assertion failed: `{0:?}` does not match `{1}`",
                                e,
                                "Err(DispatchError::Module(ModuleError { index: _, error: _, message }))",
                            ),
                        );
                    }
                };
            });
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::swap::pay_xcm_fee_with_some_asset_swapped_for_native"]
        pub const pay_xcm_fee_with_some_asset_swapped_for_native: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::swap::pay_xcm_fee_with_some_asset_swapped_for_native",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/swap.rs",
                start_line: 264usize,
                start_col: 4usize,
                end_line: 264usize,
                end_col: 50usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    pay_xcm_fee_with_some_asset_swapped_for_native(),
                ),
            ),
        };
        fn pay_xcm_fee_with_some_asset_swapped_for_native() {
            let asset_native = Location::try_from(RelayLocation::get())
                .expect("conversion works");
            let asset_one = Location {
                parents: 0,
                interior: [
                    Junction::PalletInstance(ASSETS_PALLET_ID),
                    Junction::GeneralIndex(ASSET_ID.into()),
                ]
                    .into(),
            };
            let penpal = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                let is = <AssetHubWestend as AssetHubWestendPallet>::Assets::create(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    ASSET_ID.into(),
                    AssetHubWestendSender::get().into(),
                    ASSET_MIN_BALANCE,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                if !<AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(
                    ASSET_ID,
                ) {
                    ::core::panicking::panic(
                        "assertion failed: <AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(ASSET_ID)",
                    )
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    ASSET_ID.into(),
                    AssetHubWestendSender::get().into(),
                    3_000_000_000_000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    Box::new(asset_native.clone()),
                    Box::new(asset_one.clone()),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::PoolCreated { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { ..\n})",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { ..\n})",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                let is = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    Box::new(asset_native),
                    Box::new(asset_one),
                    1_000_000_000_000,
                    2_000_000_000_000,
                    0,
                    0,
                    AssetHubWestendSender::get().into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::LiquidityAdded {
                                lp_token_minted,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(*lp_token_minted == 1414213562273)
                                && event_message.is_empty()
                            {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "lp_token_minted",
                                                lp_token_minted,
                                                "*lp_token_minted == 1414213562273",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= *lp_token_minted == 1414213562273;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {\nlp_token_minted, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {\nlp_token_minted, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                match (
                    &<AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(
                        penpal.clone(),
                    ),
                    &0,
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                let is = <AssetHubWestend as AssetHubWestendPallet>::Assets::touch_other(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    ASSET_ID.into(),
                    penpal.clone().into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = <AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendSender::get(),
                    ),
                    ASSET_ID.into(),
                    penpal.clone().into(),
                    10_000_000_000_000,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
            PenpalA::execute_with(|| {
                let call = AssetHubWestend::force_create_asset_call(
                    ASSET_ID + 1000,
                    penpal.clone(),
                    true,
                    ASSET_MIN_BALANCE,
                );
                let penpal_root = <PenpalA as Chain>::RuntimeOrigin::root();
                let fee_amount = 4_000_000_000_000u128;
                let asset_one = (
                    [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())],
                    fee_amount,
                )
                    .into();
                let asset_hub_location = PenpalA::sibling_location_of(
                        AssetHubWestend::para_id(),
                    )
                    .into();
                let xcm = xcm_transact_paid_execution(
                    call,
                    OriginKind::SovereignAccount,
                    asset_one,
                    penpal.clone(),
                );
                let is = <PenpalA as PenpalAPallet>::PolkadotXcm::send(
                    penpal_root,
                    Box::new(asset_hub_location),
                    Box::new(xcm),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                PenpalA::assert_xcm_pallet_sent();
            });
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                AssetHubWestend::assert_xcmp_queue_success(None);
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::AssetConversion(
                            pallet_asset_conversion::Event::SwapCreditExecuted { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapCreditExecuted {\n.. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapCreditExecuted {\n.. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::MessageQueue(
                            pallet_message_queue::Event::Processed { success: true, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::swap",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
        }
    }
    mod teleport {
        use crate::imports::*;
        fn relay_dest_assertions_fail(_t: SystemParaToRelayTest) {
            Westend::assert_ump_queue_processed(
                false,
                Some(AssetHubWestend::para_id()),
                Some(Weight::from_parts(157_718_000, 3_593)),
            );
        }
        fn para_origin_assertions(t: SystemParaToRelayTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            AssetHubWestend::assert_xcm_pallet_attempted_complete(
                Some(Weight::from_parts(720_053_000, 7_203)),
            );
            AssetHubWestend::assert_parachain_system_ump_sent();
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { who, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == t.sender.account_id;
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::teleport",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn penpal_to_ah_foreign_assets_sender_assertions(t: ParaToSystemParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            let system_para_native_asset_location = RelayLocation::get();
            let expected_asset_id = t.args.asset_id.unwrap();
            let (_, expected_asset_amount) = non_fee_asset(
                    &t.args.assets,
                    t.args.fee_asset_item as usize,
                )
                .unwrap();
            PenpalA::assert_xcm_pallet_attempted_complete(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Burned { asset_id, owner, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == system_para_native_asset_location)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == system_para_native_asset_location",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *asset_id == system_para_native_asset_location;
                        if !(*owner == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.sender.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { asset_id, owner, ..\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { asset_id, owner, ..\n})",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Assets(
                        pallet_assets::Event::Burned { asset_id, owner, balance },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == expected_asset_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == expected_asset_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == expected_asset_id;
                        if !(*owner == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.sender.account_id;
                        if !(*balance == expected_asset_amount)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == expected_asset_amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == expected_asset_amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::teleport",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn penpal_to_ah_foreign_assets_receiver_assertions(t: ParaToSystemParaTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
                AssetHubWestend::sibling_location_of(PenpalA::para_id()),
            );
            let (expected_foreign_asset_id, expected_foreign_asset_amount) = non_fee_asset(
                    &t.args.assets,
                    t.args.fee_asset_item as usize,
                )
                .unwrap();
            AssetHubWestend::assert_xcmp_queue_success(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { who, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == sov_penpal_on_ahr.clone().into())
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == sov_penpal_on_ahr.clone().into()",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == sov_penpal_on_ahr.clone().into();
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Minted { who, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*who == t.receiver.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "who",
                                            who,
                                            "*who == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *who == t.receiver.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Issued { asset_id, owner, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == expected_foreign_asset_id)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == expected_foreign_asset_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == expected_foreign_asset_id;
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if !(*amount == expected_foreign_asset_amount)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == expected_foreign_asset_amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == expected_foreign_asset_amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::teleport",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn ah_to_penpal_foreign_assets_sender_assertions(t: SystemParaToParaTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            AssetHubWestend::assert_xcm_pallet_attempted_complete(None);
            let (expected_foreign_asset_id, expected_foreign_asset_amount) = non_fee_asset(
                    &t.args.assets,
                    t.args.fee_asset_item as usize,
                )
                .unwrap();
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Transfer { from, to, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*from == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "from",
                                            from,
                                            "*from == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *from == t.sender.account_id;
                        if !(*to
                            == AssetHubWestend::sovereign_account_id_of(
                                t.args.dest.clone(),
                            )) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "to",
                                            to,
                                            "*to == AssetHubWestend::sovereign_account_id_of(t.args.dest.clone())",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *to
                                == AssetHubWestend::sovereign_account_id_of(
                                    t.args.dest.clone(),
                                );
                        if !(*amount == t.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == t.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == t.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Burned { asset_id, owner, balance },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == expected_foreign_asset_id)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == expected_foreign_asset_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == expected_foreign_asset_id;
                        if !(*owner == t.sender.account_id) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.sender.account_id;
                        if !(*balance == expected_foreign_asset_amount)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == expected_foreign_asset_amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == expected_foreign_asset_amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::teleport",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn ah_to_penpal_foreign_assets_receiver_assertions(t: SystemParaToParaTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            let expected_asset_id = t.args.asset_id.unwrap();
            let (_, expected_asset_amount) = non_fee_asset(
                    &t.args.assets,
                    t.args.fee_asset_item as usize,
                )
                .unwrap();
            let checking_account = <PenpalA as PenpalAPallet>::PolkadotXcm::check_account();
            let system_para_native_asset_location = RelayLocation::get();
            PenpalA::assert_xcmp_queue_success(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Assets(
                        pallet_assets::Event::Burned { asset_id, owner, balance },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == expected_asset_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == expected_asset_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == expected_asset_id;
                        if !(*owner == checking_account) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == checking_account",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == checking_account;
                        if !(*balance == expected_asset_amount)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == expected_asset_amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == expected_asset_amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance\n})",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Assets(
                        pallet_assets::Event::Issued { asset_id, owner, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == expected_asset_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == expected_asset_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == expected_asset_id;
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if !(*amount == expected_asset_amount)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == expected_asset_amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == expected_asset_amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, owner, amount })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Issued { asset_id, owner, amount },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == system_para_native_asset_location)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == system_para_native_asset_location",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions
                            &= *asset_id == system_para_native_asset_location;
                        if !(*owner == t.receiver.account_id) && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == t.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == t.receiver.account_id;
                        if !(*amount == expected_asset_amount)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == expected_asset_amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == expected_asset_amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {\nasset_id, owner, amount })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::teleport",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn system_para_limited_teleport_assets(
            t: SystemParaToRelayTest,
        ) -> DispatchResult {
            <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_teleport_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        fn para_to_system_para_transfer_assets(
            t: ParaToSystemParaTest,
        ) -> DispatchResult {
            <PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        fn system_para_to_para_transfer_assets(
            t: SystemParaToParaTest,
        ) -> DispatchResult {
            <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets(
                t.signed_origin,
                Box::new(t.args.dest.into()),
                Box::new(t.args.beneficiary.into()),
                Box::new(t.args.assets.into()),
                t.args.fee_asset_item,
                t.args.weight_limit,
            )
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::teleport::teleport_to_other_system_parachains_works"]
        pub const teleport_to_other_system_parachains_works: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::teleport::teleport_to_other_system_parachains_works",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/teleport.rs",
                start_line: 204usize,
                start_col: 4usize,
                end_line: 204usize,
                end_col: 45usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(teleport_to_other_system_parachains_works()),
            ),
        };
        fn teleport_to_other_system_parachains_works() {
            let amount = ASSET_HUB_WESTEND_ED * 100;
            let native_asset: Assets = (Parent, amount).into();
            let sender = AssetHubWestendSender::get();
            let mut para_sender_balance_before = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                    sender.clone(),
                )
                .free;
            let origin = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeOrigin::signed(
                sender.clone(),
            );
            let fee_asset_item = 0;
            let weight_limit = ::emulated_integration_tests_common::macros::WeightLimit::Unlimited;
            {
                let receiver = BridgeHubWestendReceiver::get();
                let para_receiver_balance_before = <BridgeHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        receiver.clone(),
                    )
                    .free;
                let para_destination = <AssetHubWestend>::sibling_location_of(
                    <BridgeHubWestend>::para_id(),
                );
                let beneficiary: Location = ::emulated_integration_tests_common::macros::AccountId32 {
                    network: None,
                    id: receiver.clone().into(),
                }
                    .into();
                <AssetHubWestend>::execute_with(|| {
                    let is = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_teleport_assets(
                        origin.clone(),
                        Box::new(para_destination.clone().into()),
                        Box::new(beneficiary.clone().into()),
                        Box::new(native_asset.clone().into()),
                        fee_asset_item,
                        weight_limit.clone(),
                    );
                    match is {
                        Ok(_) => {}
                        _ => {
                            if !false {
                                {
                                    ::core::panicking::panic_fmt(
                                        format_args!("Expected Ok(_). Got {0:#?}", is),
                                    );
                                }
                            }
                        }
                    };
                    type RuntimeEvent = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::PolkadotXcm(
                                ::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {
                                    outcome: Outcome::Complete { .. },
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {\noutcome: Outcome::Complete { .. } })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {\noutcome: Outcome::Complete { .. } })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::XcmpQueue(
                                ::emulated_integration_tests_common::macros::cumulus_pallet_xcmp_queue::Event::XcmpMessageSent {
                                    ..
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::XcmpQueue(::emulated_integration_tests_common::macros::cumulus_pallet_xcmp_queue::Event::XcmpMessageSent {\n.. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::XcmpQueue(::emulated_integration_tests_common::macros::cumulus_pallet_xcmp_queue::Event::XcmpMessageSent {\n.. })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                ::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {
                                    who: sender,
                                    amount,
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {\nwho: sender, amount })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {\nwho: sender, amount })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <AssetHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::AssetHubWestend",
                                                "asset_hub_westend_integration_tests::tests::teleport",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                });
                <BridgeHubWestend>::execute_with(|| {
                    type RuntimeEvent = <BridgeHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <BridgeHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                ::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {
                                    who: receiver,
                                    ..
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "BridgeHubWestend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {\nwho: receiver, .. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "BridgeHubWestend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {\nwho: receiver, .. })",
                                        <BridgeHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::MessageQueue(
                                ::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {
                                    success: true,
                                    ..
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "BridgeHubWestend",
                                        "RuntimeEvent::MessageQueue(::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "BridgeHubWestend",
                                        "RuntimeEvent::MessageQueue(::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                        <BridgeHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <BridgeHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::BridgeHubWestend",
                                                "asset_hub_westend_integration_tests::tests::teleport",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                });
                let para_sender_balance_after = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        sender.clone(),
                    )
                    .free;
                let para_receiver_balance_after = <BridgeHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        receiver.clone(),
                    )
                    .free;
                let delivery_fees = <AssetHubWestend>::execute_with(|| {
                    ::emulated_integration_tests_common::macros::asset_test_utils::xcm_helpers::teleport_assets_delivery_fees::<
                        <AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
                    >(
                        native_asset.clone(),
                        fee_asset_item,
                        weight_limit.clone(),
                        beneficiary,
                        para_destination,
                    )
                });
                match (
                    &(para_sender_balance_before - amount - delivery_fees),
                    &para_sender_balance_after,
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                if !(para_receiver_balance_after > para_receiver_balance_before) {
                    ::core::panicking::panic(
                        "assertion failed: para_receiver_balance_after > para_receiver_balance_before",
                    )
                }
                para_sender_balance_before = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        sender.clone(),
                    )
                    .free;
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::teleport::teleport_from_and_to_relay"]
        pub const teleport_from_and_to_relay: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::teleport::teleport_from_and_to_relay",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/teleport.rs",
                start_line: 217usize,
                start_col: 4usize,
                end_line: 217usize,
                end_col: 30usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(teleport_from_and_to_relay()),
            ),
        };
        fn teleport_from_and_to_relay() {
            let amount = WESTEND_ED * 100;
            let native_asset: Assets = (Here, amount).into();
            let sender = WestendSender::get();
            let mut relay_sender_balance_before = <Westend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                    sender.clone(),
                )
                .free;
            let origin = <Westend as ::emulated_integration_tests_common::macros::Chain>::RuntimeOrigin::signed(
                sender.clone(),
            );
            let fee_asset_item = 0;
            let weight_limit = ::emulated_integration_tests_common::macros::WeightLimit::Unlimited;
            {
                let receiver = AssetHubWestendReceiver::get();
                let para_receiver_balance_before = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        receiver.clone(),
                    )
                    .free;
                let para_destination = <Westend>::child_location_of(
                    <AssetHubWestend>::para_id(),
                );
                let beneficiary: Location = ::emulated_integration_tests_common::macros::AccountId32 {
                    network: None,
                    id: receiver.clone().into(),
                }
                    .into();
                <Westend>::execute_with(|| {
                    let is = <Westend as WestendPallet>::XcmPallet::limited_teleport_assets(
                        origin.clone(),
                        Box::new(para_destination.clone().into()),
                        Box::new(beneficiary.clone().into()),
                        Box::new(native_asset.clone().into()),
                        fee_asset_item,
                        weight_limit.clone(),
                    );
                    match is {
                        Ok(_) => {}
                        _ => {
                            if !false {
                                {
                                    ::core::panicking::panic_fmt(
                                        format_args!("Expected Ok(_). Got {0:#?}", is),
                                    );
                                }
                            }
                        }
                    };
                    type RuntimeEvent = <Westend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <Westend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::XcmPallet(
                                ::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {
                                    outcome: Outcome::Complete { .. },
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "Westend",
                                        "RuntimeEvent::XcmPallet(::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {\noutcome: Outcome::Complete { .. } })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "Westend",
                                        "RuntimeEvent::XcmPallet(::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {\noutcome: Outcome::Complete { .. } })",
                                        <Westend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                ::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {
                                    who: sender,
                                    amount,
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "Westend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {\nwho: sender, amount })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "Westend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {\nwho: sender, amount })",
                                        <Westend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::XcmPallet(
                                ::emulated_integration_tests_common::macros::pallet_xcm::Event::Sent {
                                    ..
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "Westend",
                                        "RuntimeEvent::XcmPallet(::emulated_integration_tests_common::macros::pallet_xcm::Event::Sent {\n.. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "Westend",
                                        "RuntimeEvent::XcmPallet(::emulated_integration_tests_common::macros::pallet_xcm::Event::Sent {\n.. })",
                                        <Westend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <Westend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::Westend",
                                                "asset_hub_westend_integration_tests::tests::teleport",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                });
                <AssetHubWestend>::execute_with(|| {
                    type RuntimeEvent = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                    let mut message: Vec<String> = Vec::new();
                    let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::Balances(
                                ::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {
                                    who: receiver,
                                    ..
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {\nwho: receiver, .. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {\nwho: receiver, .. })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    let mut event_received = false;
                    let mut meet_conditions = true;
                    let mut index_match = 0;
                    let mut event_message: Vec<String> = Vec::new();
                    for (index, event) in events.iter().enumerate() {
                        meet_conditions = true;
                        match event {
                            RuntimeEvent::MessageQueue(
                                ::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {
                                    success: true,
                                    ..
                                },
                            ) => {
                                event_received = true;
                                let mut conditions_message: Vec<String> = Vec::new();
                                if event_received && meet_conditions {
                                    index_match = index;
                                    break;
                                } else {
                                    event_message.extend(conditions_message);
                                }
                            }
                            _ => {}
                        }
                    }
                    if event_received && !meet_conditions {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::MessageQueue(::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                        event_message.concat(),
                                    ),
                                );
                                res
                            });
                    } else if !event_received {
                        message
                            .push({
                                let res = ::alloc::fmt::format(
                                    format_args!(
                                        "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                        "AssetHubWestend",
                                        "RuntimeEvent::MessageQueue(::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                        <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                    ),
                                );
                                res
                            });
                    } else {
                        events.remove(index_match);
                    }
                    if !message.is_empty() {
                        <AssetHubWestend as ::xcm_emulator::Chain>::events()
                            .iter()
                            .for_each(|event| {
                                {
                                    let lvl = ::log::Level::Debug;
                                    if lvl <= ::log::STATIC_MAX_LEVEL
                                        && lvl <= ::log::max_level()
                                    {
                                        ::log::__private_api::log(
                                            format_args!("{0:?}", event),
                                            lvl,
                                            &(
                                                "events::AssetHubWestend",
                                                "asset_hub_westend_integration_tests::tests::teleport",
                                                ::log::__private_api::loc(),
                                            ),
                                            (),
                                        );
                                    }
                                };
                            });
                        {
                            #[cold]
                            #[track_caller]
                            #[inline(never)]
                            #[rustc_const_panic_str]
                            #[rustc_do_not_const_check]
                            const fn panic_cold_display<T: ::core::fmt::Display>(
                                arg: &T,
                            ) -> ! {
                                ::core::panicking::panic_display(arg)
                            }
                            panic_cold_display(&message.concat());
                        }
                    }
                });
                let relay_sender_balance_after = <Westend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        sender.clone(),
                    )
                    .free;
                let para_receiver_balance_after = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        receiver.clone(),
                    )
                    .free;
                let delivery_fees = <Westend>::execute_with(|| {
                    ::emulated_integration_tests_common::macros::asset_test_utils::xcm_helpers::teleport_assets_delivery_fees::<
                        <WestendXcmConfig as xcm_executor::Config>::XcmSender,
                    >(
                        native_asset.clone(),
                        fee_asset_item,
                        weight_limit.clone(),
                        beneficiary,
                        para_destination,
                    )
                });
                match (
                    &(relay_sender_balance_before - amount - delivery_fees),
                    &relay_sender_balance_after,
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                if !(para_receiver_balance_after > para_receiver_balance_before) {
                    ::core::panicking::panic(
                        "assertion failed: para_receiver_balance_after > para_receiver_balance_before",
                    )
                }
                relay_sender_balance_before = <Westend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                        sender.clone(),
                    )
                    .free;
            };
            let sender = AssetHubWestendSender::get();
            let para_sender_balance_before = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                    sender.clone(),
                )
                .free;
            let origin = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeOrigin::signed(
                sender.clone(),
            );
            let assets: Assets = (Parent, amount).into();
            let fee_asset_item = 0;
            let weight_limit = ::emulated_integration_tests_common::macros::WeightLimit::Unlimited;
            let receiver = WestendReceiver::get();
            let relay_receiver_balance_before = <Westend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                    receiver.clone(),
                )
                .free;
            let relay_destination: Location = Parent.into();
            let beneficiary: Location = ::emulated_integration_tests_common::macros::AccountId32 {
                network: None,
                id: receiver.clone().into(),
            }
                .into();
            <AssetHubWestend>::execute_with(|| {
                let is = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_teleport_assets(
                    origin.clone(),
                    Box::new(relay_destination.clone().into()),
                    Box::new(beneficiary.clone().into()),
                    Box::new(assets.clone().into()),
                    fee_asset_item,
                    weight_limit.clone(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                type RuntimeEvent = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::PolkadotXcm(
                            ::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {
                                outcome: Outcome::Complete { .. },
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {\noutcome: Outcome::Complete { .. } })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::Attempted {\noutcome: Outcome::Complete { .. } })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Balances(
                            ::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {
                                who: sender,
                                amount,
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {\nwho: sender, amount })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Burned {\nwho: sender, amount })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::PolkadotXcm(
                            ::emulated_integration_tests_common::macros::pallet_xcm::Event::Sent {
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::Sent {\n.. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::PolkadotXcm(::emulated_integration_tests_common::macros::pallet_xcm::Event::Sent {\n.. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::teleport",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
            <Westend>::execute_with(|| {
                type RuntimeEvent = <Westend as ::emulated_integration_tests_common::macros::Chain>::RuntimeEvent;
                let mut message: Vec<String> = Vec::new();
                let mut events = <Westend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Balances(
                            ::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {
                                who: receiver,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {\nwho: receiver, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::Balances(::emulated_integration_tests_common::macros::pallet_balances::Event::Minted {\nwho: receiver, .. })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::MessageQueue(
                            ::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {
                                success: true,
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::MessageQueue(::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::MessageQueue(::emulated_integration_tests_common::macros::pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <Westend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::Westend",
                                            "asset_hub_westend_integration_tests::tests::teleport",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
            let para_sender_balance_after = <AssetHubWestend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                    sender.clone(),
                )
                .free;
            let relay_receiver_balance_after = <Westend as ::emulated_integration_tests_common::macros::Chain>::account_data_of(
                    receiver.clone(),
                )
                .free;
            let delivery_fees = <AssetHubWestend>::execute_with(|| {
                ::emulated_integration_tests_common::macros::asset_test_utils::xcm_helpers::teleport_assets_delivery_fees::<
                    <AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
                >(
                    assets,
                    fee_asset_item,
                    weight_limit.clone(),
                    beneficiary,
                    relay_destination,
                )
            });
            match (
                &(para_sender_balance_before - amount - delivery_fees),
                &para_sender_balance_after,
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            if !(relay_receiver_balance_after > relay_receiver_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: relay_receiver_balance_after > relay_receiver_balance_before",
                )
            }
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::teleport::limited_teleport_native_assets_from_system_para_to_relay_fails"]
        pub const limited_teleport_native_assets_from_system_para_to_relay_fails: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::teleport::limited_teleport_native_assets_from_system_para_to_relay_fails",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/teleport.rs",
                start_line: 239usize,
                start_col: 4usize,
                end_line: 239usize,
                end_col: 66usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    limited_teleport_native_assets_from_system_para_to_relay_fails(),
                ),
            ),
        };
        /// Limited Teleport of native asset from System Parachain to Relay Chain
        /// shouldn't work when there is not enough balance in Relay Chain's `CheckAccount`
        fn limited_teleport_native_assets_from_system_para_to_relay_fails() {
            let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
            let destination = AssetHubWestend::parent_location().into();
            let beneficiary_id = WestendReceiver::get().into();
            let assets = (Parent, amount_to_send).into();
            let test_args = TestContext {
                sender: AssetHubWestendSender::get(),
                receiver: WestendReceiver::get(),
                args: TestArgs::new_para(
                    destination,
                    beneficiary_id,
                    amount_to_send,
                    assets,
                    None,
                    0,
                ),
            };
            let mut test = SystemParaToRelayTest::new(test_args);
            let sender_balance_before = test.sender.balance;
            let receiver_balance_before = test.receiver.balance;
            test.set_assertion::<AssetHubWestend>(para_origin_assertions);
            test.set_assertion::<Westend>(relay_dest_assertions_fail);
            test.set_dispatchable::<
                    AssetHubWestend,
                >(system_para_limited_teleport_assets);
            test.assert();
            let sender_balance_after = test.sender.balance;
            let receiver_balance_after = test.receiver.balance;
            let delivery_fees = AssetHubWestend::execute_with(|| {
                xcm_helpers::teleport_assets_delivery_fees::<
                    <AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
                >(
                    test.args.assets.clone(),
                    0,
                    test.args.weight_limit,
                    test.args.beneficiary,
                    test.args.dest,
                )
            });
            match (
                &(sender_balance_before - amount_to_send - delivery_fees),
                &sender_balance_after,
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (&receiver_balance_after, &receiver_balance_before) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        /// Bidirectional teleports of local Penpal assets to Asset Hub as foreign assets while paying
        /// fees using (reserve transferred) native asset.
        pub fn do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
            para_to_ah_dispatchable: fn(ParaToSystemParaTest) -> DispatchResult,
            ah_to_para_dispatchable: fn(SystemParaToParaTest) -> DispatchResult,
        ) {
            let fee_amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 100;
            let asset_location_on_penpal = PenpalLocalTeleportableToAssetHub::get();
            let asset_id_on_penpal = match asset_location_on_penpal.last() {
                Some(Junction::GeneralIndex(id)) => *id as u32,
                _ => ::core::panicking::panic("internal error: entered unreachable code"),
            };
            let asset_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
            let asset_owner = PenpalAssetOwner::get();
            let system_para_native_asset_location = RelayLocation::get();
            let sender = PenpalASender::get();
            let penpal_check_account = <PenpalA as PenpalAPallet>::PolkadotXcm::check_account();
            let ah_as_seen_by_penpal = PenpalA::sibling_location_of(
                AssetHubWestend::para_id(),
            );
            let penpal_assets: Assets = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (Parent, fee_amount_to_send).into(),
                        (asset_location_on_penpal.clone(), asset_amount_to_send).into(),
                    ]),
                )
                .into();
            let fee_asset_index = penpal_assets
                .inner()
                .iter()
                .position(|r| r == &(Parent, fee_amount_to_send).into())
                .unwrap() as u32;
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
                system_para_native_asset_location.clone(),
                sender.clone(),
                fee_amount_to_send * 2,
            );
            PenpalA::mint_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
                asset_id_on_penpal,
                sender.clone(),
                asset_amount_to_send,
            );
            PenpalA::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (
                            penpal_check_account.clone().into(),
                            ASSET_HUB_WESTEND_ED * 1000,
                        ),
                    ]),
                ),
            );
            let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(
                penpal_as_seen_by_ah,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (
                            sov_penpal_on_ah.clone().into(),
                            ASSET_HUB_WESTEND_ED * 100_000_000_000,
                        ),
                    ]),
                ),
            );
            let foreign_asset_at_asset_hub_westend = Location::new(
                    1,
                    [Junction::Parachain(PenpalA::para_id().into())],
                )
                .appended_with(asset_location_on_penpal)
                .unwrap();
            let penpal_to_ah_beneficiary_id = AssetHubWestendReceiver::get();
            let penpal_to_ah_test_args = TestContext {
                sender: PenpalASender::get(),
                receiver: AssetHubWestendReceiver::get(),
                args: TestArgs::new_para(
                    ah_as_seen_by_penpal,
                    penpal_to_ah_beneficiary_id,
                    asset_amount_to_send,
                    penpal_assets,
                    Some(asset_id_on_penpal),
                    fee_asset_index,
                ),
            };
            let mut penpal_to_ah = ParaToSystemParaTest::new(penpal_to_ah_test_args);
            let penpal_sender_balance_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    system_para_native_asset_location.clone(),
                    &PenpalASender::get(),
                )
            });
            let ah_receiver_balance_before = penpal_to_ah.receiver.balance;
            let penpal_sender_assets_before = PenpalA::execute_with(|| {
                type Assets = <PenpalA as PenpalAPallet>::Assets;
                <Assets as Inspect<
                    _,
                >>::balance(asset_id_on_penpal, &PenpalASender::get())
            });
            let ah_receiver_assets_before = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <Assets as Inspect<
                    _,
                >>::balance(
                    foreign_asset_at_asset_hub_westend.clone().try_into().unwrap(),
                    &AssetHubWestendReceiver::get(),
                )
            });
            penpal_to_ah
                .set_assertion::<PenpalA>(penpal_to_ah_foreign_assets_sender_assertions);
            penpal_to_ah
                .set_assertion::<
                    AssetHubWestend,
                >(penpal_to_ah_foreign_assets_receiver_assertions);
            penpal_to_ah.set_dispatchable::<PenpalA>(para_to_ah_dispatchable);
            penpal_to_ah.assert();
            let penpal_sender_balance_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    system_para_native_asset_location.clone(),
                    &PenpalASender::get(),
                )
            });
            let ah_receiver_balance_after = penpal_to_ah.receiver.balance;
            let penpal_sender_assets_after = PenpalA::execute_with(|| {
                type Assets = <PenpalA as PenpalAPallet>::Assets;
                <Assets as Inspect<
                    _,
                >>::balance(asset_id_on_penpal, &PenpalASender::get())
            });
            let ah_receiver_assets_after = AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <Assets as Inspect<
                    _,
                >>::balance(
                    foreign_asset_at_asset_hub_westend.clone().try_into().unwrap(),
                    &AssetHubWestendReceiver::get(),
                )
            });
            if !(penpal_sender_balance_after < penpal_sender_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: penpal_sender_balance_after < penpal_sender_balance_before",
                )
            }
            if !(ah_receiver_balance_after > ah_receiver_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: ah_receiver_balance_after > ah_receiver_balance_before",
                )
            }
            if !(ah_receiver_balance_after
                < ah_receiver_balance_before + fee_amount_to_send)
            {
                ::core::panicking::panic(
                    "assertion failed: ah_receiver_balance_after < ah_receiver_balance_before + fee_amount_to_send",
                )
            }
            match (
                &(penpal_sender_assets_before - asset_amount_to_send),
                &penpal_sender_assets_after,
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &ah_receiver_assets_after,
                &(ah_receiver_assets_before + asset_amount_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                let is = ForeignAssets::transfer(
                    <AssetHubWestend as Chain>::RuntimeOrigin::signed(
                        AssetHubWestendReceiver::get(),
                    ),
                    foreign_asset_at_asset_hub_westend.clone().try_into().unwrap(),
                    AssetHubWestendSender::get().into(),
                    asset_amount_to_send,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
            });
            let ah_to_penpal_beneficiary_id = PenpalAReceiver::get();
            let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let ah_assets: Assets = <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (Parent, fee_amount_to_send).into(),
                        (
                            foreign_asset_at_asset_hub_westend.clone(),
                            asset_amount_to_send,
                        )
                            .into(),
                    ]),
                )
                .into();
            let fee_asset_index = ah_assets
                .inner()
                .iter()
                .position(|r| r == &(Parent, fee_amount_to_send).into())
                .unwrap() as u32;
            let ah_to_penpal_test_args = TestContext {
                sender: AssetHubWestendSender::get(),
                receiver: PenpalAReceiver::get(),
                args: TestArgs::new_para(
                    penpal_as_seen_by_ah,
                    ah_to_penpal_beneficiary_id,
                    asset_amount_to_send,
                    ah_assets,
                    Some(asset_id_on_penpal),
                    fee_asset_index,
                ),
            };
            let mut ah_to_penpal = SystemParaToParaTest::new(ah_to_penpal_test_args);
            let ah_sender_balance_before = ah_to_penpal.sender.balance;
            let penpal_receiver_balance_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    system_para_native_asset_location.clone(),
                    &PenpalAReceiver::get(),
                )
            });
            let ah_sender_assets_before = AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    foreign_asset_at_asset_hub_westend.clone().try_into().unwrap(),
                    &AssetHubWestendSender::get(),
                )
            });
            let penpal_receiver_assets_before = PenpalA::execute_with(|| {
                type Assets = <PenpalA as PenpalAPallet>::Assets;
                <Assets as Inspect<
                    _,
                >>::balance(asset_id_on_penpal, &PenpalAReceiver::get())
            });
            ah_to_penpal
                .set_assertion::<
                    AssetHubWestend,
                >(ah_to_penpal_foreign_assets_sender_assertions);
            ah_to_penpal
                .set_assertion::<
                    PenpalA,
                >(ah_to_penpal_foreign_assets_receiver_assertions);
            ah_to_penpal.set_dispatchable::<AssetHubWestend>(ah_to_para_dispatchable);
            ah_to_penpal.assert();
            let ah_sender_balance_after = ah_to_penpal.sender.balance;
            let penpal_receiver_balance_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(system_para_native_asset_location, &PenpalAReceiver::get())
            });
            let ah_sender_assets_after = AssetHubWestend::execute_with(|| {
                type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(
                    foreign_asset_at_asset_hub_westend.clone().try_into().unwrap(),
                    &AssetHubWestendSender::get(),
                )
            });
            let penpal_receiver_assets_after = PenpalA::execute_with(|| {
                type Assets = <PenpalA as PenpalAPallet>::Assets;
                <Assets as Inspect<
                    _,
                >>::balance(asset_id_on_penpal, &PenpalAReceiver::get())
            });
            if !(ah_sender_balance_after < ah_sender_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: ah_sender_balance_after < ah_sender_balance_before",
                )
            }
            if !(penpal_receiver_balance_after > penpal_receiver_balance_before) {
                ::core::panicking::panic(
                    "assertion failed: penpal_receiver_balance_after > penpal_receiver_balance_before",
                )
            }
            if !(penpal_receiver_balance_after
                < penpal_receiver_balance_before + fee_amount_to_send)
            {
                ::core::panicking::panic(
                    "assertion failed: penpal_receiver_balance_after <\n    penpal_receiver_balance_before + fee_amount_to_send",
                )
            }
            match (
                &(ah_sender_assets_before - asset_amount_to_send),
                &ah_sender_assets_after,
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &penpal_receiver_assets_after,
                &(penpal_receiver_assets_before + asset_amount_to_send),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::teleport::bidirectional_teleport_foreign_assets_between_para_and_asset_hub"]
        pub const bidirectional_teleport_foreign_assets_between_para_and_asset_hub: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::teleport::bidirectional_teleport_foreign_assets_between_para_and_asset_hub",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/teleport.rs",
                start_line: 527usize,
                start_col: 4usize,
                end_line: 527usize,
                end_col: 68usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(
                    bidirectional_teleport_foreign_assets_between_para_and_asset_hub(),
                ),
            ),
        };
        /// Bidirectional teleports of local Penpal assets to Asset Hub as foreign assets should work
        /// (using native reserve-based transfer for fees)
        fn bidirectional_teleport_foreign_assets_between_para_and_asset_hub() {
            do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
                para_to_system_para_transfer_assets,
                system_para_to_para_transfer_assets,
            );
        }
    }
    mod treasury {
        use crate::imports::*;
        use emulated_integration_tests_common::{
            accounts::{ALICE, BOB},
            USDT_ID,
        };
        use frame_support::traits::fungibles::{Inspect, Mutate};
        use polkadot_runtime_common::impls::VersionedLocatableAsset;
        use xcm_executor::traits::ConvertLocation;
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::treasury::create_and_claim_treasury_spend"]
        pub const create_and_claim_treasury_spend: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName(
                    "tests::treasury::create_and_claim_treasury_spend",
                ),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/treasury.rs",
                start_line: 26usize,
                start_col: 4usize,
                end_line: 26usize,
                end_col: 35usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(create_and_claim_treasury_spend()),
            ),
        };
        fn create_and_claim_treasury_spend() {
            const SPEND_AMOUNT: u128 = 1_000_000_000;
            let treasury_location: Location = Location::new(1, PalletInstance(37));
            let treasury_account = ahw_xcm_config::LocationToAccountId::convert_location(
                    &treasury_location,
                )
                .unwrap();
            let asset_hub_location = Location::new(
                0,
                Parachain(AssetHubWestend::para_id().into()),
            );
            let root = <Westend as Chain>::RuntimeOrigin::root();
            let asset_kind = VersionedLocatableAsset::V5 {
                location: asset_hub_location,
                asset_id: AssetId(
                    [PalletInstance(50), GeneralIndex(USDT_ID.into())].into(),
                ),
            };
            let alice: AccountId = Westend::account_id_of(ALICE);
            let bob: AccountId = Westend::account_id_of(BOB);
            let bob_signed = <Westend as Chain>::RuntimeOrigin::signed(bob.clone());
            AssetHubWestend::execute_with(|| {
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                let is = <Assets as Mutate<
                    _,
                >>::mint_into(USDT_ID, &treasury_account, SPEND_AMOUNT * 4);
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                match (&<Assets as Inspect<_>>::balance(USDT_ID, &alice), &0u128) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
            Westend::execute_with(|| {
                type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
                type Treasury = <Westend as WestendPallet>::Treasury;
                type AssetRate = <Westend as WestendPallet>::AssetRate;
                let is = AssetRate::create(
                    root.clone(),
                    Box::new(asset_kind.clone()),
                    2.into(),
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = Treasury::spend(
                    root,
                    Box::new(asset_kind),
                    SPEND_AMOUNT,
                    Box::new(
                        Location::new(0, Into::<[u8; 32]>::into(alice.clone())).into(),
                    ),
                    None,
                );
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let is = Treasury::payout(bob_signed.clone(), 0);
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <Westend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <Westend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::Westend",
                                            "asset_hub_westend_integration_tests::tests::treasury",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
            AssetHubWestend::execute_with(|| {
                type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
                type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
                let mut message: Vec<String> = Vec::new();
                let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Assets(
                            pallet_assets::Event::Transferred {
                                asset_id: id,
                                from,
                                to,
                                amount,
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if !(id == &USDT_ID) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "id",
                                                id,
                                                "id == &USDT_ID",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= id == &USDT_ID;
                            if !(from == &treasury_account) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "from",
                                                from,
                                                "from == &treasury_account",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= from == &treasury_account;
                            if !(to == &alice) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "to",
                                                to,
                                                "to == &alice",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= to == &alice;
                            if !(amount == &SPEND_AMOUNT) && event_message.is_empty() {
                                conditions_message
                                    .push({
                                        let res = ::alloc::fmt::format(
                                            format_args!(
                                                " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                                "amount",
                                                amount,
                                                "amount == &SPEND_AMOUNT",
                                            ),
                                        );
                                        res
                                    });
                            }
                            meet_conditions &= amount == &SPEND_AMOUNT;
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Transferred {\nasset_id: id, from, to, amount })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::Assets(pallet_assets::Event::Transferred {\nasset_id: id, from, to, amount })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::ParachainSystem(
                            cumulus_pallet_parachain_system::Event::UpwardMessageSent {
                                ..
                            },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent {\n.. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent {\n.. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::MessageQueue(
                            pallet_message_queue::Event::Processed { success: true, .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "AssetHubWestend",
                                    "RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {\nsuccess: true, .. })",
                                    <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <AssetHubWestend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::AssetHubWestend",
                                            "asset_hub_westend_integration_tests::tests::treasury",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
                match (
                    &<Assets as Inspect<_>>::balance(USDT_ID, &alice),
                    &SPEND_AMOUNT,
                ) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
            });
            Westend::execute_with(|| {
                type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
                type Treasury = <Westend as WestendPallet>::Treasury;
                let is = Treasury::check_status(bob_signed, 0);
                match is {
                    Ok(_) => {}
                    _ => {
                        if !false {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!("Expected Ok(_). Got {0:#?}", is),
                                );
                            }
                        }
                    }
                };
                let mut message: Vec<String> = Vec::new();
                let mut events = <Westend as ::xcm_emulator::Chain>::events();
                let mut event_received = false;
                let mut meet_conditions = true;
                let mut index_match = 0;
                let mut event_message: Vec<String> = Vec::new();
                for (index, event) in events.iter().enumerate() {
                    meet_conditions = true;
                    match event {
                        RuntimeEvent::Treasury(
                            pallet_treasury::Event::SpendProcessed { .. },
                        ) => {
                            event_received = true;
                            let mut conditions_message: Vec<String> = Vec::new();
                            if event_received && meet_conditions {
                                index_match = index;
                                break;
                            } else {
                                event_message.extend(conditions_message);
                            }
                        }
                        _ => {}
                    }
                }
                if event_received && !meet_conditions {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                    "Westend",
                                    "RuntimeEvent::Treasury(pallet_treasury::Event::SpendProcessed { .. })",
                                    event_message.concat(),
                                ),
                            );
                            res
                        });
                } else if !event_received {
                    message
                        .push({
                            let res = ::alloc::fmt::format(
                                format_args!(
                                    "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                    "Westend",
                                    "RuntimeEvent::Treasury(pallet_treasury::Event::SpendProcessed { .. })",
                                    <Westend as ::xcm_emulator::Chain>::events(),
                                ),
                            );
                            res
                        });
                } else {
                    events.remove(index_match);
                }
                if !message.is_empty() {
                    <Westend as ::xcm_emulator::Chain>::events()
                        .iter()
                        .for_each(|event| {
                            {
                                let lvl = ::log::Level::Debug;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api::log(
                                        format_args!("{0:?}", event),
                                        lvl,
                                        &(
                                            "events::Westend",
                                            "asset_hub_westend_integration_tests::tests::treasury",
                                            ::log::__private_api::loc(),
                                        ),
                                        (),
                                    );
                                }
                            };
                        });
                    {
                        #[cold]
                        #[track_caller]
                        #[inline(never)]
                        #[rustc_const_panic_str]
                        #[rustc_do_not_const_check]
                        const fn panic_cold_display<T: ::core::fmt::Display>(
                            arg: &T,
                        ) -> ! {
                            ::core::panicking::panic_display(arg)
                        }
                        panic_cold_display(&message.concat());
                    }
                }
            });
        }
    }
    mod xcm_fee_estimation {
        //! Tests to ensure correct XCM fee estimation for cross-chain asset transfers.
        use crate::imports::*;
        use frame_support::{
            dispatch::RawOrigin, sp_runtime::{traits::Dispatchable, DispatchResult},
        };
        use xcm_runtime_apis::{
            dry_run::runtime_decl_for_dry_run_api::DryRunApiV1,
            fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1,
        };
        fn sender_assertions(test: ParaToParaThroughAHTest) {
            type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
            PenpalA::assert_xcm_pallet_attempted_complete(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalA as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Burned { asset_id, owner, balance },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == Location::new(1, []))
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == Location::new(1, [])",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == Location::new(1, []);
                        if !(*owner == test.sender.account_id)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == test.sender.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == test.sender.account_id;
                        if !(*balance == test.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "balance",
                                            balance,
                                            "*balance == test.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *balance == test.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalA",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned {\nasset_id, owner, balance })",
                                <PenpalA as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalA as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalA",
                                        "asset_hub_westend_integration_tests::tests::xcm_fee_estimation",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn hop_assertions(test: ParaToParaThroughAHTest) {
            type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
            AssetHubWestend::assert_xcmp_queue_success(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <AssetHubWestend as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::Balances(
                        pallet_balances::Event::Burned { amount, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*amount == test.args.amount) && event_message.is_empty() {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "amount",
                                            amount,
                                            "*amount == test.args.amount",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *amount == test.args.amount;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { amount, .. })",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "AssetHubWestend",
                                "RuntimeEvent::Balances(pallet_balances::Event::Burned { amount, .. })",
                                <AssetHubWestend as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <AssetHubWestend as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::AssetHubWestend",
                                        "asset_hub_westend_integration_tests::tests::xcm_fee_estimation",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn receiver_assertions(test: ParaToParaThroughAHTest) {
            type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
            PenpalB::assert_xcmp_queue_success(None);
            let mut message: Vec<String> = Vec::new();
            let mut events = <PenpalB as ::xcm_emulator::Chain>::events();
            let mut event_received = false;
            let mut meet_conditions = true;
            let mut index_match = 0;
            let mut event_message: Vec<String> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                meet_conditions = true;
                match event {
                    RuntimeEvent::ForeignAssets(
                        pallet_assets::Event::Issued { asset_id, owner, .. },
                    ) => {
                        event_received = true;
                        let mut conditions_message: Vec<String> = Vec::new();
                        if !(*asset_id == Location::new(1, []))
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "asset_id",
                                            asset_id,
                                            "*asset_id == Location::new(1, [])",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *asset_id == Location::new(1, []);
                        if !(*owner == test.receiver.account_id)
                            && event_message.is_empty()
                        {
                            conditions_message
                                .push({
                                    let res = ::alloc::fmt::format(
                                        format_args!(
                                            " - The attribute {0:?} = {1:?} did not met the condition {2:?}\n",
                                            "owner",
                                            owner,
                                            "*owner == test.receiver.account_id",
                                        ),
                                    );
                                    res
                                });
                        }
                        meet_conditions &= *owner == test.receiver.account_id;
                        if event_received && meet_conditions {
                            index_match = index;
                            break;
                        } else {
                            event_message.extend(conditions_message);
                        }
                    }
                    _ => {}
                }
            }
            if event_received && !meet_conditions {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was received but some of its attributes did not meet the conditions:\n{2}",
                                "PenpalB",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                event_message.concat(),
                            ),
                        );
                        res
                    });
            } else if !event_received {
                message
                    .push({
                        let res = ::alloc::fmt::format(
                            format_args!(
                                "\n\n{0}::\u{1b}[31m{1}\u{1b}[0m was never received. All events:\n{2:#?}",
                                "PenpalB",
                                "RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, ..\n})",
                                <PenpalB as ::xcm_emulator::Chain>::events(),
                            ),
                        );
                        res
                    });
            } else {
                events.remove(index_match);
            }
            if !message.is_empty() {
                <PenpalB as ::xcm_emulator::Chain>::events()
                    .iter()
                    .for_each(|event| {
                        {
                            let lvl = ::log::Level::Debug;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api::log(
                                    format_args!("{0:?}", event),
                                    lvl,
                                    &(
                                        "events::PenpalB",
                                        "asset_hub_westend_integration_tests::tests::xcm_fee_estimation",
                                        ::log::__private_api::loc(),
                                    ),
                                    (),
                                );
                            }
                        };
                    });
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    #[rustc_const_panic_str]
                    #[rustc_do_not_const_check]
                    const fn panic_cold_display<T: ::core::fmt::Display>(arg: &T) -> ! {
                        ::core::panicking::panic_display(arg)
                    }
                    panic_cold_display(&message.concat());
                }
            }
        }
        fn transfer_assets_para_to_para_through_ah_dispatchable(
            test: ParaToParaThroughAHTest,
        ) -> DispatchResult {
            let call = transfer_assets_para_to_para_through_ah_call(test.clone());
            match call.dispatch(test.signed_origin) {
                Ok(_) => Ok(()),
                Err(error_with_post_info) => Err(error_with_post_info.error),
            }
        }
        fn transfer_assets_para_to_para_through_ah_call(
            test: ParaToParaThroughAHTest,
        ) -> <PenpalA as Chain>::RuntimeCall {
            type RuntimeCall = <PenpalA as Chain>::RuntimeCall;
            let asset_hub_location: Location = PenpalB::sibling_location_of(
                AssetHubWestend::para_id(),
            );
            let custom_xcm_on_dest = Xcm::<
                (),
            >(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        DepositAsset {
                            assets: Wild(AllCounted(test.args.assets.len() as u32)),
                            beneficiary: test.args.beneficiary,
                        },
                    ]),
                ),
            );
            RuntimeCall::PolkadotXcm(pallet_xcm::Call::transfer_assets_using_type_and_then {
                dest: Box::new(test.args.dest.into()),
                assets: Box::new(test.args.assets.clone().into()),
                assets_transfer_type: Box::new(
                    TransferType::RemoteReserve(asset_hub_location.clone().into()),
                ),
                remote_fees_id: Box::new(
                    VersionedAssetId::V5(AssetId(Location::new(1, []))),
                ),
                fees_transfer_type: Box::new(
                    TransferType::RemoteReserve(asset_hub_location.into()),
                ),
                custom_xcm_on_dest: Box::new(VersionedXcm::from(custom_xcm_on_dest)),
                weight_limit: test.args.weight_limit,
            })
        }
        extern crate test;
        #[cfg(test)]
        #[rustc_test_marker = "tests::xcm_fee_estimation::multi_hop_works"]
        pub const multi_hop_works: test::TestDescAndFn = test::TestDescAndFn {
            desc: test::TestDesc {
                name: test::StaticTestName("tests::xcm_fee_estimation::multi_hop_works"),
                ignore: false,
                ignore_message: ::core::option::Option::None,
                source_file: "cumulus/parachains/integration-tests/emulated/tests/assets/asset-hub-westend/src/tests/xcm_fee_estimation.rs",
                start_line: 115usize,
                start_col: 4usize,
                end_line: 115usize,
                end_col: 19usize,
                compile_fail: false,
                no_run: false,
                should_panic: test::ShouldPanic::No,
                test_type: test::TestType::UnitTest,
            },
            testfn: test::StaticTestFn(
                #[coverage(off)]
                || test::assert_test_result(multi_hop_works()),
            ),
        };
        /// We are able to dry-run and estimate the fees for a multi-hop XCM journey.
        /// Scenario: Alice on PenpalA has some WND and wants to send them to PenpalB.
        /// We want to know the fees using the `DryRunApi` and `XcmPaymentApi`.
        fn multi_hop_works() {
            let destination = PenpalA::sibling_location_of(PenpalB::para_id());
            let sender = PenpalASender::get();
            let amount_to_send = 1_000_000_000_000;
            let asset_owner = PenpalAssetOwner::get();
            let assets: Assets = (Parent, amount_to_send).into();
            let relay_native_asset_location = Location::parent();
            let sender_as_seen_by_ah = AssetHubWestend::sibling_location_of(
                PenpalA::para_id(),
            );
            let sov_of_sender_on_ah = AssetHubWestend::sovereign_account_id_of(
                sender_as_seen_by_ah.clone(),
            );
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
                relay_native_asset_location.clone(),
                sender.clone(),
                amount_to_send * 2,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([
                        (sov_of_sender_on_ah.clone(), amount_to_send * 2),
                    ]),
                ),
            );
            let beneficiary_id = PenpalBReceiver::get();
            let test_args = TestContext {
                sender: PenpalASender::get(),
                receiver: PenpalBReceiver::get(),
                args: TestArgs::new_para(
                    destination,
                    beneficiary_id.clone(),
                    amount_to_send,
                    assets,
                    None,
                    0,
                ),
            };
            let mut test = ParaToParaThroughAHTest::new(test_args);
            let mut delivery_fees_amount = 0;
            let mut remote_message = VersionedXcm::V5(Xcm(Vec::new()));
            <PenpalA as TestExt>::execute_with(|| {
                type Runtime = <PenpalA as Chain>::Runtime;
                type OriginCaller = <PenpalA as Chain>::OriginCaller;
                let call = transfer_assets_para_to_para_through_ah_call(test.clone());
                let origin = OriginCaller::system(RawOrigin::Signed(sender.clone()));
                let result = Runtime::dry_run_call(origin, call).unwrap();
                let (destination_to_query, messages_to_query) = &result
                    .forwarded_xcms
                    .iter()
                    .find(|(destination, _)| {
                        *destination
                            == VersionedLocation::V5(Location::new(1, [Parachain(1000)]))
                    })
                    .unwrap();
                match (&messages_to_query.len(), &1) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                };
                remote_message = messages_to_query[0].clone();
                let delivery_fees = Runtime::query_delivery_fees(
                        destination_to_query.clone(),
                        remote_message.clone(),
                    )
                    .unwrap();
                delivery_fees_amount = get_amount_from_versioned_assets(delivery_fees);
            });
            let mut intermediate_execution_fees = 0;
            let mut intermediate_delivery_fees_amount = 0;
            let mut intermediate_remote_message = VersionedXcm::V5(
                Xcm::<()>(Vec::new()),
            );
            <AssetHubWestend as TestExt>::execute_with(|| {
                type Runtime = <AssetHubWestend as Chain>::Runtime;
                type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
                let weight = Runtime::query_xcm_weight(remote_message.clone()).unwrap();
                intermediate_execution_fees = Runtime::query_weight_to_asset_fee(
                        weight,
                        VersionedAssetId::V5(Location::new(1, []).into()),
                    )
                    .unwrap();
                let xcm_program = VersionedXcm::V5(
                    Xcm::<RuntimeCall>::from(remote_message.clone().try_into().unwrap()),
                );
                let result = Runtime::dry_run_xcm(
                        sender_as_seen_by_ah.clone().into(),
                        xcm_program,
                    )
                    .unwrap();
                let (destination_to_query, messages_to_query) = &result
                    .forwarded_xcms
                    .iter()
                    .find(|(destination, _)| {
                        *destination
                            == VersionedLocation::V5(Location::new(1, [Parachain(2001)]))
                    })
                    .unwrap();
                intermediate_remote_message = messages_to_query[0].clone();
                let delivery_fees = Runtime::query_delivery_fees(
                        destination_to_query.clone(),
                        intermediate_remote_message.clone(),
                    )
                    .unwrap();
                intermediate_delivery_fees_amount = get_amount_from_versioned_assets(
                    delivery_fees,
                );
            });
            let mut final_execution_fees = 0;
            <PenpalB as TestExt>::execute_with(|| {
                type Runtime = <PenpalA as Chain>::Runtime;
                let weight = Runtime::query_xcm_weight(
                        intermediate_remote_message.clone(),
                    )
                    .unwrap();
                final_execution_fees = Runtime::query_weight_to_asset_fee(
                        weight,
                        VersionedAssetId::V5(Parent.into()),
                    )
                    .unwrap();
            });
            PenpalA::reset_ext();
            AssetHubWestend::reset_ext();
            PenpalB::reset_ext();
            PenpalA::mint_foreign_asset(
                <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
                relay_native_asset_location.clone(),
                sender.clone(),
                amount_to_send * 2,
            );
            AssetHubWestend::fund_accounts(
                <[_]>::into_vec(
                    #[rustc_box]
                    ::alloc::boxed::Box::new([(sov_of_sender_on_ah, amount_to_send * 2)]),
                ),
            );
            let sender_assets_before = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &sender)
            });
            let receiver_assets_before = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &beneficiary_id)
            });
            test.set_assertion::<PenpalA>(sender_assertions);
            test.set_assertion::<AssetHubWestend>(hop_assertions);
            test.set_assertion::<PenpalB>(receiver_assertions);
            test.set_dispatchable::<
                    PenpalA,
                >(transfer_assets_para_to_para_through_ah_dispatchable);
            test.assert();
            let sender_assets_after = PenpalA::execute_with(|| {
                type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location.clone(), &sender)
            });
            let receiver_assets_after = PenpalB::execute_with(|| {
                type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
                <ForeignAssets as Inspect<
                    _,
                >>::balance(relay_native_asset_location, &beneficiary_id)
            });
            match (
                &sender_assets_after,
                &(sender_assets_before - amount_to_send - delivery_fees_amount),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (
                &receiver_assets_after,
                &(receiver_assets_before + amount_to_send - intermediate_execution_fees
                    - intermediate_delivery_fees_amount - final_execution_fees),
            ) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
        }
    }
}
#[rustc_main]
#[coverage(off)]
pub fn main() -> () {
    extern crate test;
    test::test_main_static(
        &[
            &assets_can_be_claimed,
            &create_and_claim_treasury_spend,
            &bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types,
            &transfer_foreign_assets_from_asset_hub_to_para,
            &transfer_foreign_assets_from_para_to_asset_hub,
            &transfer_foreign_assets_from_para_to_para_through_asset_hub,
            &transfer_native_asset_from_relay_to_para_through_asset_hub,
            &reserve_transfer_multiple_assets_from_asset_hub_to_para,
            &reserve_transfer_multiple_assets_from_para_to_asset_hub,
            &reserve_transfer_native_asset_from_asset_hub_to_para,
            &reserve_transfer_native_asset_from_asset_hub_to_relay_fails,
            &reserve_transfer_native_asset_from_para_to_asset_hub,
            &reserve_transfer_native_asset_from_para_to_para_through_relay,
            &reserve_transfer_native_asset_from_para_to_relay,
            &reserve_transfer_native_asset_from_relay_to_asset_hub_fails,
            &reserve_transfer_native_asset_from_relay_to_para,
            &send_transact_as_superuser_from_relay_to_asset_hub_works,
            &send_xcm_from_para_to_asset_hub_paying_fee_with_sufficient_asset,
            &send_xcm_from_para_to_asset_hub_paying_fee_with_system_asset,
            &relay_sets_system_para_xcm_supported_version,
            &system_para_sets_relay_xcm_supported_version,
            &cannot_create_pool_from_pool_assets,
            &pay_xcm_fee_with_some_asset_swapped_for_native,
            &swap_locally_on_chain_using_foreign_assets,
            &swap_locally_on_chain_using_local_assets,
            &bidirectional_teleport_foreign_assets_between_para_and_asset_hub,
            &limited_teleport_native_assets_from_system_para_to_relay_fails,
            &teleport_from_and_to_relay,
            &teleport_to_other_system_parachains_works,
            &create_and_claim_treasury_spend,
            &multi_hop_works,
        ],
    )
}
