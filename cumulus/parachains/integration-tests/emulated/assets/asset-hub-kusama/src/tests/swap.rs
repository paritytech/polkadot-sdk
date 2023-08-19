use crate::*;
use asset_hub_kusama_runtime::constants::currency::EXISTENTIAL_DEPOSIT;
use frame_support::{instances::Instance2, BoundedVec};
use sp_runtime::{DispatchError, ModuleError};

#[test]
fn swap_locally_on_chain_using_local_assets() {
	let asset_native = Box::new(asset_hub_kusama_runtime::xcm_config::KsmLocation::get());
	let asset_one = Box::new(MultiLocation {
		parents: 0,
		interior: X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())),
	});

	AssetHubKusama::execute_with(|| {
		type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::Assets::create(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
			ASSET_ID.into(),
			AssetHubKusamaSender::get().into(),
			1000,
		));
		assert!(<AssetHubKusama as AssetHubKusamaPallet>::Assets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::Assets::mint(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
			ASSET_ID.into(),
			AssetHubKusamaSender::get().into(),
			100_000_000_000_000,
		));

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::Balances::force_set_balance(
			<AssetHubKusama as Chain>::RuntimeOrigin::root(),
			AssetHubKusamaSender::get().into(),
			100_000_000_000_000,
		));

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::create_pool(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
			asset_native.clone(),
			asset_one.clone(),
		));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::add_liquidity(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
			asset_native.clone(),
			asset_one.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			AssetHubKusamaSender::get().into()
		));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => { lp_token_minted: *lp_token_minted == 1414213562273, },
			]
		);

		let path = BoundedVec::<_, _>::truncate_from(vec![asset_native.clone(), asset_one.clone()]);

		assert_ok!(
			<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::swap_exact_tokens_for_tokens(
				<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
				path,
				100,
				1,
				AssetHubKusamaSender::get().into(),
				true
			)
		);

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. }) => {
					amount_in: *amount_in == 100,
					amount_out: *amount_out == 199,
				},
			]
		);

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::remove_liquidity(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
			asset_native,
			asset_one,
			1414213562273 - EXISTENTIAL_DEPOSIT * 2, // all but the 2 EDs can't be retrieved.
			0,
			0,
			AssetHubKusamaSender::get().into(),
		));
	});
}

#[test]
fn swap_locally_on_chain_using_foreign_assets() {
	use frame_support::weights::WeightToFee;

	let asset_native = Box::new(asset_hub_kusama_runtime::xcm_config::KsmLocation::get());

	let foreign_asset1_at_asset_hub_kusama = Box::new(MultiLocation {
		parents: 1,
		interior: X3(
			Parachain(PenpalKusamaA::para_id().into()),
			PalletInstance(ASSETS_PALLET_ID),
			GeneralIndex(ASSET_ID.into()),
		),
	});

	let assets_para_destination: VersionedMultiLocation =
		MultiLocation { parents: 1, interior: X1(Parachain(AssetHubKusama::para_id().into())) }
			.into();

	let penpal_location =
		MultiLocation { parents: 1, interior: X1(Parachain(PenpalKusamaA::para_id().into())) };

	// 1. Create asset on penpal:
	PenpalKusamaA::execute_with(|| {
		assert_ok!(<PenpalKusamaA as PenpalKusamaAPallet>::Assets::create(
			<PenpalKusamaA as Chain>::RuntimeOrigin::signed(PenpalKusamaASender::get()),
			ASSET_ID.into(),
			PenpalKusamaASender::get().into(),
			1000,
		));

		assert!(<PenpalKusamaA as PenpalKusamaAPallet>::Assets::asset_exists(ASSET_ID));
	});

	// 2. Create foreign asset on asset_hub_kusama:

	let require_weight_at_most = Weight::from_parts(1_100_000_000_000, 30_000);
	let origin_kind = OriginKind::Xcm;
	let sov_penpal_on_asset_hub_kusama = AssetHubKusama::sovereign_account_id_of(penpal_location);

	AssetHubKusama::fund_accounts(vec![
		(AssetHubKusamaSender::get().into(), 5_000_000 * KUSAMA_ED), /* An account to swap dot
		                                                              * for something else. */
		(sov_penpal_on_asset_hub_kusama.clone().into(), 1000_000_000_000_000_000 * KUSAMA_ED),
	]);

	let sov_penpal_on_asset_hub_kusama_as_location: MultiLocation = MultiLocation {
		parents: 0,
		interior: X1(AccountId32Junction {
			network: None,
			id: sov_penpal_on_asset_hub_kusama.clone().into(),
		}),
	};

	let call_foreign_assets_create =
		<AssetHubKusama as Chain>::RuntimeCall::ForeignAssets(pallet_assets::Call::<
			<AssetHubKusama as Chain>::Runtime,
			Instance2,
		>::create {
			id: *foreign_asset1_at_asset_hub_kusama,
			min_balance: 1000,
			admin: sov_penpal_on_asset_hub_kusama.clone().into(),
		})
		.encode()
		.into();

	let buy_execution_fee_amount =
		asset_hub_kusama_runtime::constants::fee::WeightToFee::weight_to_fee(&Weight::from_parts(
			10_100_000_000_000,
			300_000,
		));
	let buy_execution_fee = MultiAsset {
		id: Concrete(MultiLocation { parents: 1, interior: Here }),
		fun: Fungible(buy_execution_fee_amount),
	};

	let xcm = VersionedXcm::from(Xcm(vec![
		WithdrawAsset { 0: vec![buy_execution_fee.clone()].into() },
		BuyExecution { fees: buy_execution_fee.clone(), weight_limit: Unlimited },
		Transact { require_weight_at_most, origin_kind, call: call_foreign_assets_create },
		RefundSurplus,
		DepositAsset {
			assets: All.into(),
			beneficiary: sov_penpal_on_asset_hub_kusama_as_location,
		},
	]));

	// Send XCM message from penpal => asset_hub_kusama
	let sudo_penpal_origin = <PenpalKusamaA as Chain>::RuntimeOrigin::root();
	PenpalKusamaA::execute_with(|| {
		assert_ok!(<PenpalKusamaA as PenpalKusamaAPallet>::PolkadotXcm::send(
			sudo_penpal_origin.clone(),
			bx!(assets_para_destination.clone()),
			bx!(xcm),
		));

		type RuntimeEvent = <PenpalKusamaA as Chain>::RuntimeEvent;

		assert_expected_events!(
			PenpalKusamaA,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubKusama::execute_with(|| {
		assert!(<AssetHubKusama as AssetHubKusamaPallet>::ForeignAssets::asset_exists(
			*foreign_asset1_at_asset_hub_kusama
		));

		// 3: Mint foreign asset on asset_hub_kusama:
		//
		// (While it might be nice to use batch,
		// currently that's disabled due to safe call filters.)

		type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;
		// 3. Mint foreign asset (in reality this should be a teleport or some such)
		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::ForeignAssets::mint(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_kusama.clone().into()
			),
			*foreign_asset1_at_asset_hub_kusama,
			sov_penpal_on_asset_hub_kusama.clone().into(),
			3_000_000_000_000,
		));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},
			]
		);

		// 4. Create pool:
		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::create_pool(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_kusama.clone(),
		));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		// 5. Add liquidity:
		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::add_liquidity(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_kusama.clone()
			),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_kusama.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			sov_penpal_on_asset_hub_kusama.clone().into()
		));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => {
					lp_token_minted: *lp_token_minted == 1414213562273,
				},
			]
		);

		// 6. Swap!
		let path = BoundedVec::<_, _>::truncate_from(vec![
			asset_native.clone(),
			foreign_asset1_at_asset_hub_kusama.clone(),
		]);

		assert_ok!(
			<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::swap_exact_tokens_for_tokens(
				<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
				path,
				100000,
				1000,
				AssetHubKusamaSender::get().into(),
				true
			)
		);

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. },) => {
					amount_in: *amount_in == 100000,
					amount_out: *amount_out == 199399,
				},
			]
		);

		// 7. Remove liquidity
		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::remove_liquidity(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_kusama.clone()
			),
			asset_native,
			foreign_asset1_at_asset_hub_kusama,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			sov_penpal_on_asset_hub_kusama.clone().into(),
		));
	});
}

#[test]
fn cannot_create_pool_from_pool_assets() {
	let asset_native = Box::new(asset_hub_kusama_runtime::xcm_config::KsmLocation::get());
	let mut asset_one = asset_hub_kusama_runtime::xcm_config::PoolAssetsPalletLocation::get();
	asset_one.append_with(GeneralIndex(ASSET_ID.into())).expect("pool assets");

	AssetHubKusama::execute_with(|| {
		let pool_owner_account_id = asset_hub_kusama_runtime::AssetConversionOrigin::get();

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::PoolAssets::create(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(pool_owner_account_id.clone()),
			ASSET_ID.into(),
			pool_owner_account_id.clone().into(),
			1000,
		));
		assert!(<AssetHubKusama as AssetHubKusamaPallet>::PoolAssets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubKusama as AssetHubKusamaPallet>::PoolAssets::mint(
			<AssetHubKusama as Chain>::RuntimeOrigin::signed(pool_owner_account_id),
			ASSET_ID.into(),
			AssetHubKusamaSender::get().into(),
			3_000_000_000_000,
		));

		assert_matches::assert_matches!(
			<AssetHubKusama as AssetHubKusamaPallet>::AssetConversion::create_pool(
				<AssetHubKusama as Chain>::RuntimeOrigin::signed(AssetHubKusamaSender::get()),
				asset_native.clone(),
				Box::new(asset_one),
			),
			Err(DispatchError::Module(ModuleError{index: _, error: _, message})) => assert_eq!(message, Some("UnsupportedAsset"))
		);
	});
}
