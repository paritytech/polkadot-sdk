use crate::*;
use frame_support::{instances::Instance2, BoundedVec};
use sp_runtime::{DispatchError, ModuleError};
use xcm_emulator::Parachain;

#[test]
fn swap_locally_on_chain_using_local_assets() {
	const ASSET_ID: u32 = 1;

	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());
	let asset_one = Box::new(MultiLocation {
		parents: 0,
		interior: X2(PalletInstance(50), GeneralIndex(ASSET_ID.into())),
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Parachain>::RuntimeEvent;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::create(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			1000,
		));
		assert!(<AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			3_000_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			asset_one.clone(),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			asset_one.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			AssetHubWestendSender::get().into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => { lp_token_minted: *lp_token_minted == 1414213562273, },
			]
		);

		let path = BoundedVec::<_, _>::truncate_from(vec![asset_native.clone(), asset_one.clone()]);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::swap_exact_tokens_for_tokens(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			path,
			100,
			1,
			AssetHubWestendSender::get().into(),
			true
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. }) => {
					amount_in: *amount_in == 100,
					amount_out: *amount_out == 199,
				},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::remove_liquidity(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native,
			asset_one,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			AssetHubWestendSender::get().into(),
		));
	});
}

#[test]
fn swap_locally_on_chain_using_foreign_assets() {
	use frame_support::weights::WeightToFee;

	const ASSET_ID: u32 = 1;
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());

	let foreign_asset1_at_asset_hub_westend = Box::new(MultiLocation {
		parents: 1,
		interior: X3(
			Parachain(PenpalWestend::para_id().into()),
			PalletInstance(50),
			GeneralIndex(ASSET_ID.into()),
		),
	});

	let assets_para_destination: VersionedMultiLocation =
		MultiLocation { parents: 1, interior: X1(Parachain(AssetHubWestend::para_id().into())) }
			.into();

	let penpal_location =
		MultiLocation { parents: 1, interior: X1(Parachain(PenpalWestend::para_id().into())) };

	// 1. Create asset on penpal:
	PenpalWestend::execute_with(|| {
		assert_ok!(<PenpalWestend as PenpalWestendPallet>::Assets::create(
			<PenpalWestend as Parachain>::RuntimeOrigin::signed(PenpalWestendSender::get()),
			ASSET_ID.into(),
			PenpalWestendSender::get().into(),
			1000,
		));

		assert!(<PenpalWestend as PenpalWestendPallet>::Assets::asset_exists(ASSET_ID));
	});

	// 2. Create foreign asset on asset_hub_westend:

	let require_weight_at_most = Weight::from_parts(1_100_000_000_000, 30_000);
	let origin_kind = OriginKind::Xcm;
	let sov_penpal_on_asset_hub_westend = AssetHubWestend::sovereign_account_id_of(penpal_location);

	AssetHubWestend::fund_accounts(vec![
		(AssetHubWestendSender::get(), 5_000_000), // An account to swap dot for something else.
		(sov_penpal_on_asset_hub_westend.clone(), 1000_000_000_000_000_000),
	]);

	let sov_penpal_on_asset_hub_westend_as_location: MultiLocation = MultiLocation {
		parents: 0,
		interior: X1(AccountId32 {
			network: None,
			id: sov_penpal_on_asset_hub_westend.clone().into(),
		}),
	};

	let call_foreign_assets_create =
		<AssetHubWestend as Para>::RuntimeCall::ForeignAssets(pallet_assets::Call::<
			<AssetHubWestend as Para>::Runtime,
			Instance2,
		>::create {
			id: *foreign_asset1_at_asset_hub_westend,
			min_balance: 1000,
			admin: sov_penpal_on_asset_hub_westend.clone().into(),
		})
		.encode()
		.into();

	let buy_execution_fee_amount =
		asset_hub_westend_runtime::constants::fee::WeightToFee::weight_to_fee(&Weight::from_parts(
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
			beneficiary: sov_penpal_on_asset_hub_westend_as_location,
		},
	]));

	// Send XCM message from penpal => asset_hub_westend
	let sudo_penpal_origin = <PenpalWestend as Parachain>::RuntimeOrigin::root();
	PenpalWestend::execute_with(|| {
		assert_ok!(<PenpalWestend as PenpalWestendPallet>::PolkadotXcm::send(
			sudo_penpal_origin.clone(),
			bx!(assets_para_destination.clone()),
			bx!(xcm),
		));

		type RuntimeEvent = <PenpalWestend as Parachain>::RuntimeEvent;

		assert_expected_events!(
			PenpalWestend,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubWestend::execute_with(|| {
		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			*foreign_asset1_at_asset_hub_westend
		));

		// 3: Mint foreign asset on asset_hub_westend:
		//
		// (While it might be nice to use batch,
		// currently that's disabled due to safe call filters.)

		type RuntimeEvent = <AssetHubWestend as Parachain>::RuntimeEvent;
		// 3. Mint foreign asset (in reality this should be a teleport or some such)
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_westend.clone().into()
			),
			*foreign_asset1_at_asset_hub_westend,
			sov_penpal_on_asset_hub_westend.clone().into(),
			3_000_000_000_000,
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},
			]
		);

		// 4. Create pool:
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_westend.clone(),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		// 5. Add liquidity:
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_westend.clone()
			),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_westend.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			sov_penpal_on_asset_hub_westend.clone().into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => {
					lp_token_minted: *lp_token_minted == 1414213562273,
				},
			]
		);

		// 6. Swap!
		let path = BoundedVec::<_, _>::truncate_from(vec![
			asset_native.clone(),
			foreign_asset1_at_asset_hub_westend.clone(),
		]);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::swap_exact_tokens_for_tokens(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			path,
			100000,
			1000,
			AssetHubWestendSender::get().into(),
			true
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. },) => {
					amount_in: *amount_in == 100000,
					amount_out: *amount_out == 199399,
				},
			]
		);

		// 7. Remove liquidity
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::remove_liquidity(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_westend.clone()
			),
			asset_native,
			foreign_asset1_at_asset_hub_westend,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			sov_penpal_on_asset_hub_westend.clone().into(),
		));
	});
}

#[test]
fn cannot_create_pool_from_pool_assets() {
	const ASSET_ID: u32 = 1;

	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());
	let mut asset_one = asset_hub_westend_runtime::xcm_config::PoolAssetsPalletLocation::get();
	asset_one.append_with(GeneralIndex(ASSET_ID.into())).expect("pool assets");

	AssetHubWestend::execute_with(|| {
		let pool_owner_account_id = asset_hub_westend_runtime::AssetConversionOrigin::get();

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::create(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(pool_owner_account_id.clone()),
			ASSET_ID.into(),
			pool_owner_account_id.clone().into(),
			1000,
		));
		assert!(<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::mint(
			<AssetHubWestend as Parachain>::RuntimeOrigin::signed(pool_owner_account_id),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			3_000_000_000_000,
		));

		assert_matches::assert_matches!(
			<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
				<AssetHubWestend as Parachain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
				asset_native.clone(),
				Box::new(asset_one),
			),
			Err(DispatchError::Module(ModuleError{index: _, error: _, message})) => assert_eq!(message, Some("UnsupportedAsset"))
		);
	});
}
