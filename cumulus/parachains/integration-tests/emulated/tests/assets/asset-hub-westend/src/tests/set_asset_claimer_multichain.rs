use emulated_integration_tests_common::accounts::{ALICE, BOB, CHARLIE};
use emulated_integration_tests_common::impls::AccountId32;
use emulated_integration_tests_common::xcm_emulator::log;
use crate::{
    imports::*,
};

use frame_support::{sp_runtime::{traits::Dispatchable}, assert_ok, LOG_TARGET};
use westend_system_emulated_network::asset_hub_westend_emulated_chain::asset_hub_westend_runtime;
use westend_system_emulated_network::bridge_hub_westend_emulated_chain::BridgeHubWestendParaPallet;
use westend_system_emulated_network::westend_emulated_chain::westend_runtime::xcm_config::AssetHub;
use crate::imports::ahw_xcm_config::UniversalLocation;

#[test]
fn test_set_asset_claimer_on_multiverse() {
    println!("printlined_error");

    let alice = AssetHubWestend::account_id_of(ALICE);
    let bob = BridgeHubWestend::account_id_of(BOB);
    let destination = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
    let alice_location = Location::new(0, Junction::AccountId32 { network: None, id: alice.clone().into() });
    let assets: Assets = (Parent, 12300456000u128).into();
    // let alice_on_asset_hub = Location::new(1, [Parachain(1000), Junction::AccountId32 { id: [67u8; 32], network: Some(Westend) }]);


    let bob_on_bh = Location::new(
        1,
        [
            Parachain(BridgeHubWestend::para_id().into()),
            Junction::AccountId32 {
                network: Some(Westend),
                id: bob.clone().into()}],
    );
    println!("BOB on BridgeHub: {:?}", bob_on_bh);
    // let bob_acc = AssetHubWestend::sovereign_account_id_of(PenpalASender::);

    let amount_to_send = 16_000_000_000_000u128;
    let alice_acc = AssetHubWestend::account_id_of(ALICE);
    AssetHubWestend::fund_accounts(vec![(
        alice_acc.clone(),
        amount_to_send * 2,
    )]);

    let balance = <AssetHubWestend as Chain>::account_data_of(alice_acc.clone()).free;
    println!("alice balance {:?}", balance);


    let test_args = TestContext {
        sender: alice_acc.clone(),
        receiver: alice_acc.clone(),
        args: TestArgs::new_para(
            alice_location.clone(),
            alice_acc.clone(),
            amount_to_send,
            assets.clone(),
            None,
            0,
        ),
    };
    let test = SystemParaToParaTest::new(test_args);
    execute_ah_test(test.clone(), bob_on_bh.clone(), trap_assets_ah);

    let balance = <AssetHubWestend as Chain>::account_data_of(alice_acc.clone()).free;
    println!("ali balance {:?}", balance);



    // SECOND PART OF THE TEST

    BridgeHubWestend::fund_accounts(vec![(
        bob.clone(),
        amount_to_send * 20000,
    )]);

    let balance = <BridgeHubWestend as Chain>::account_data_of(bob.clone()).free;
    println!("bob balance {:?}", balance);

    let test_args = TestContext {
        sender: bob.clone(),
        receiver: alice.clone(),
        args: TestArgs::new_para(
            destination.clone(),
            alice_acc.clone(),
            amount_to_send,
            assets.clone(),
            None,
            0,
        ),
    };
    let test = BridgeToAssetHubTest::new(test_args);
    let asset_hub = BridgeHubWestend::sibling_location_of(
        AssetHubWestend::para_id()
    ).into();
    let xcm_there = Xcm::<()>::builder_unsafe()
        .claim_asset(test.args.assets.clone(), Here)
        .pay_fees((Parent, 40_000_000_000u128))
        .deposit_asset(AllCounted(1), bob_on_bh.clone())
        .build();

    BridgeHubWestend::execute_with(|| {
        assert_ok!(<BridgeHubWestend as BridgeHubWestendParaPallet>::PolkadotXcm::send(
            test.signed_origin,
            bx!(asset_hub),
            bx!(VersionedXcm::from(xcm_there)),
        ));
    });

    // execute_bh_test(test.clone(), bob_on_bh.clone(), transfer_assets_from_bh);
}

fn execute_bh_test(
    test: BridgeToAssetHubTest,
    claimer: Location,
    xcm_fn: impl Fn(BridgeToAssetHubTest, Location) -> <BridgeHubWestend as Chain>::RuntimeCall,
) {
    let call = xcm_fn(test.clone(), claimer.clone());
    BridgeHubWestend::execute_with(|| {
        assert!(call.dispatch(test.signed_origin).is_ok());
    });
}

fn transfer_assets_from_bh(
    test: BridgeToAssetHubTest,
    claimer: Location
) -> <BridgeHubWestend as Chain>::RuntimeCall {

    let xcm_there = Xcm::<()>::builder_unsafe()
        .pay_fees((Parent, 40_000_000_000u128))
        .claim_asset(test.args.assets.clone(), Here)
        .deposit_asset(AllCounted(1), claimer)
        .build();

    let xcm_here = Xcm::<RuntimeCall>::builder_unsafe()
        .withdraw_asset((Parent, 200_000_000_000u128))
        .pay_fees((Parent, 40_000_000_000u128))
        .export_message(
            Westend,
            Parachain(1000),
            xcm_there
        )
        // .transfer_reserve_asset(
        //     (Parent, 60_000_000_000u128),
        //     test.args.dest.clone(), xcm_there)
        // .initiate_teleport(
        //     Definite((Parent, 60_000_000_000u128).into()),
        //     test.args.dest.clone(), xcm_there)
        .build();

    type RuntimeCall = <BridgeHubWestend as Chain>::RuntimeCall;
    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(xcm_here)),
        max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
    })


    // .transfer_reserve_asset(test.args.assets.clone(), bob_on_ah, xcm_there)
    // .withdraw_asset(test.args.assets.clone())
    // .transfer_reserve_asset(test.args.assets.clone(), bob_on_ah, xcm_there)
    // .deposit_reserve_asset(test.args.assets.clone(), test.args.dest.clone(), xcm_there)
    // let al_ac = AssetHubWestend::account_id_of(ALICE);
    // let bob_on_ah = Location::new(
    //     1,
    //     [
    //         Parachain(AssetHubWestend::para_id().into()),
    //         Junction::AccountId32 {
    //             network: None,
    //             id: al_ac.clone().into()}
    //     ],
    // );
    // let fee_asset = Asset { id: AssetId(Location::new(0, [])), fun: Fungibility::Fungible(1_000_000_000u128) };
    // let asset_hub = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());a
}


fn execute_ah_test(
    test: SystemParaToParaTest,
    claimer: Location,
    xcm_fn: impl Fn(SystemParaToParaTest, Location) -> <AssetHubWestend as Chain>::RuntimeCall,
) {
    let call = xcm_fn(test.clone(), claimer.clone());
    AssetHubWestend::execute_with(|| {
        assert!(call.dispatch(test.signed_origin).is_ok());
    });
}

fn trap_assets_ah(
    test: SystemParaToParaTest,
    claimer: Location
) -> <AssetHubWestend as Chain>::RuntimeCall {
    type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;

    let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
        .set_asset_claimer(claimer.clone())
        .withdraw_asset(test.args.assets.clone())
        .clear_origin()
        .build();

    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(local_xcm)),
        max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
    })
}

