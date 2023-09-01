use crate::tests::mock_network::{
    parachain::{self, RuntimeOrigin},
    parachain_account_sovereign_account_id,
    primitives::{AccountId, CENTS},
    relay_chain, MockNet, ParaA, ParachainBalances, ParachainPalletXcm, Relay, ALICE, BOB,
    INITIAL_BALANCE,
};
use codec::Decode;
use codec::Encode;
use frame_support::pallet_prelude::Weight;
use frame_support::traits::Currency;
use frame_support::{assert_ok, traits::fungibles::Mutate};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_balances::{BalanceLock, Reasons};
use crate::{CollectEvents, DebugInfo, Determinism};
use pallet_contracts_primitives::Code;
use xcm::{v3::prelude::*, VersionedResponse, VersionedXcm};
use xcm_executor::traits::{QueryHandler, QueryResponseStatus};
use xcm_simulator::TestExt;

type ParachainContracts = crate::Pallet<parachain::Runtime>;

/// Instantiate the tests contract, and fund it with some balance and assets.
fn instantiate_test_contract() -> AccountId {
	let wasm = vec![]; // TODO

    // Instantiate contract.
    let contract_addr = ParaA::execute_with(|| {
        ParachainContracts::bare_instantiate(
            ALICE,
            0,
            Weight::MAX,
            None,
            Code::Upload(wasm),
            vec![],
            vec![],
            DebugInfo::UnsafeDebug,
            CollectEvents::Skip,
        )
        .result
        .unwrap()
        .account_id
    });

    // Funds contract account with some balance and assets.
    ParaA::execute_with(|| {
        parachain::Balances::make_free_balance_be(&contract_addr, INITIAL_BALANCE);
        parachain::Assets::mint_into(0u32.into(), &contract_addr, INITIAL_BALANCE).unwrap();
    });
    Relay::execute_with(|| {
        let sovereign_account = parachain_account_sovereign_account_id(1u32, contract_addr.clone());
        relay_chain::Balances::make_free_balance_be(&sovereign_account, INITIAL_BALANCE);
    });

    contract_addr
}

#[test]
fn test_xcm_execute() {
    MockNet::reset();

    let contract_addr = instantiate_test_contract();

    // Execute XCM instructions through the contract.
    ParaA::execute_with(|| {
        let amount: u128 = 10 * CENTS;
        let fee = parachain::estimate_message_fee(3);

        // The XCM used to transfer funds to Bob.
        let message: xcm_simulator::Xcm<()> = Xcm(vec![
            WithdrawAsset(vec![(Here, amount).into(), (Parent, fee).into()].into()),
            BuyExecution {
                fees: (Parent, fee).into(),
                weight_limit: WeightLimit::Unlimited,
            },
            DepositAsset {
                assets: All.into(),
                beneficiary: MultiLocation {
                    parents: 0,
                    interior: AccountId32 {
                        network: None,
                        id: BOB.clone().into(),
                    }
                    .into(),
                }
                .into(),
            },
        ]);

        // Execute the XCM message, through the contract.
        assert_ok!(
            ParachainContracts::bare_call(
                ALICE,
                contract_addr.clone(),
                0,
                Weight::MAX,
                None,
                message.encode(),
                DebugInfo::UnsafeDebug,
                CollectEvents::UnsafeCollect,
                Determinism::Enforced,
            )
            .result
        );

        // Check if the funds are subtracted from the account of Alice and added to the account of Bob.
        let initial = INITIAL_BALANCE;
        assert_eq!(parachain::Assets::balance(0, contract_addr), initial - fee);
        assert_eq!(ParachainBalances::free_balance(BOB), initial + amount);
    });
}

#[test]
fn test_xcm_send() {
    MockNet::reset();
    let contract_addr = instantiate_test_contract();
    let fee = parachain::estimate_message_fee(4); // Accounts for the `DescendOrigin` instruction added by `send_xcm`

    ParaA::execute_with(|| {
        let message: xcm_simulator::Xcm<()> = Xcm(vec![
            WithdrawAsset((Here, fee).into()),
            BuyExecution {
                fees: (Here, fee).into(),
                weight_limit: WeightLimit::Unlimited,
            },
            LockAsset {
                asset: (Here, 5 * CENTS).into(),
                unlocker: (Parachain(1)).into(),
            },
        ]);

        assert_ok!(
            ParachainContracts::bare_call(
                ALICE,
                contract_addr.clone(),
                0,
                Weight::MAX,
                None,
                (
                    MultiLocation::from(Parent),
                    message
                )
                    .encode(),
                DebugInfo::UnsafeDebug,
                CollectEvents::UnsafeCollect,
                Determinism::Enforced,
            )
            .result
        );
    });

    Relay::execute_with(|| {
        // Check if the funds are locked on the relay chain.
        assert_eq!(
            relay_chain::Balances::locks(&parachain_account_sovereign_account_id(1, contract_addr)),
            vec![BalanceLock {
                id: *b"py/xcmlk",
                amount: 5 * CENTS,
                reasons: Reasons::All
            }]
        );
    });
}

#[test]
fn test_xcm_new_query() {
    MockNet::reset();
    let  contract_addr = instantiate_test_contract();

    ParaA::execute_with(|| {
        let match_querier = MultiLocation::from(AccountId32 {
            network: None,
            id: ALICE.into(),
        });
        let timeout = 1u32;

        let exec = ParachainContracts::bare_call(
            ALICE,
            contract_addr.clone(),
            0,
            Weight::MAX,
            None,
            (
                timeout,
                match_querier,
            )
                .encode(),
            DebugInfo::UnsafeDebug,
            CollectEvents::UnsafeCollect,
            Determinism::Enforced,
        );

        let query_id = Result::<Result<u64, u8>, u8>::decode(&mut &exec.result.unwrap().data[..])
            .expect("Failed to decode message")
            .expect("Contract execution trapped")
            .expect("xcm_new_query failed");

        let response = ParachainPalletXcm::take_response(query_id);
        let expected_response = QueryResponseStatus::Pending {
            timeout: timeout as BlockNumberFor<parachain::Runtime>,
        };
        assert_eq!(response, expected_response);
    });
}

#[test]
fn test_xcm_take_response() {
    MockNet::reset();
    let contract_addr = instantiate_test_contract();
    ParaA::execute_with(|| {
        let querier: MultiLocation = (
            Parent,
            AccountId32 {
                network: None,
                id: ALICE.into(),
            },
        )
            .into();
        let responder = MultiLocation::from(AccountId32 {
            network: Some(NetworkId::ByGenesis([0u8; 32])),
            id: ALICE.into(),
        });
        let query_id = ParachainPalletXcm::new_query(responder, 1u32.into(), querier.clone());

        let fee = parachain::estimate_message_fee(4);
        let message = Xcm(vec![
            WithdrawAsset(vec![(Parent, fee).into()].into()),
            BuyExecution {
                fees: (Parent, fee).into(),
                weight_limit: WeightLimit::Unlimited,
            },
            QueryResponse {
                query_id,
                response: Response::ExecutionResult(None),
                max_weight: Weight::zero(),
                querier: Some(querier),
            },
        ]);

        let call = |query_id| {
            let exec = ParachainContracts::bare_call(
                ALICE,
                contract_addr.clone(),
                0,
                Weight::MAX,
                None,
                (
                    query_id,
                )
                    .encode(),
                DebugInfo::UnsafeDebug,
                CollectEvents::UnsafeCollect,
                Determinism::Enforced,
            );

            Result::<Result<Option<VersionedResponse>, u8>, u8>::decode(
                &mut &exec.result.unwrap().data[..],
            )
            .expect("Failed to decode message")
            .expect("Contract execution trapped")
        };

        // Query is not yet answered.
        assert_eq!(Ok(None), call(query_id));

        ParachainPalletXcm::execute(
            RuntimeOrigin::signed(ALICE),
            Box::new(VersionedXcm::V3(message)),
            Weight::from_parts(1_000_000_000, 1_000_000_000),
        )
        .unwrap();

        // Query is answered.
        assert_eq!(
            Ok(Some(VersionedResponse::V3(Response::ExecutionResult(None)))),
            call(query_id)
        );

        // Query is not found. (Query was already answered)
        assert_eq!(Err(1u8), call(query_id));
    })
}

