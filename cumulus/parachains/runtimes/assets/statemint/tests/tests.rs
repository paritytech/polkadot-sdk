use asset_test_utils::{ExtBuilder, RuntimeHelper};
use frame_support::{
	assert_noop, assert_ok,
	traits::PalletInfo,
	weights::{Weight, WeightToFee as WeightToFeeT},
};
use parachains_common::{AccountId, StatemintAuraId as AuraId};
pub use statemint_runtime::{
	constants::fee::WeightToFee, xcm_config::XcmConfig, Assets, Balances, ExistentialDeposit,
	Runtime, SessionKeys, System,
};
use xcm::latest::prelude::*;
use xcm_executor::traits::WeightTrader;
pub const ALICE: [u8; 32] = [1u8; 32];

#[test]
fn test_asset_xcm_trader_does_not_work_in_statemine() {
	ExtBuilder::<Runtime>::default()
		.with_collators(vec![AccountId::from(ALICE)])
		.with_session_keys(vec![(
			AccountId::from(ALICE),
			AccountId::from(ALICE),
			SessionKeys { aura: AuraId::from(sp_core::ed25519::Public::from_raw(ALICE)) },
		)])
		.build()
		.execute_with(|| {
			// We need root origin to create a sufficient asset
			// We set existential deposit to be identical to the one for Balances first
			assert_ok!(Assets::force_create(
				RuntimeHelper::<Runtime>::root_origin(),
				1,
				AccountId::from(ALICE).into(),
				true,
				ExistentialDeposit::get()
			));

			let mut trader = <XcmConfig as xcm_executor::Config>::Trader::new();

			// Set Alice as block author, who will receive fees
			RuntimeHelper::<Runtime>::run_to_block(2, Some(AccountId::from(ALICE)));

			// We are going to buy 400e9 weight
			// Because of the ED being higher in statemine
			// and not to complicate things, we use a little
			// bit more of weight
			let bought = 400_000_000_000u64;

			// lets calculate amount needed
			let amount_needed = WeightToFee::weight_to_fee(&Weight::from_ref_time(bought));

			let asset_multilocation = MultiLocation::new(
				0,
				X2(
					PalletInstance(
						<Runtime as frame_system::Config>::PalletInfo::index::<Assets>().unwrap()
							as u8,
					),
					GeneralIndex(1),
				),
			);

			let asset: MultiAsset = (asset_multilocation, amount_needed).into();

			// Buy weight should return an error, since asset trader not installed
			assert_noop!(trader.buy_weight(bought, asset.into()), XcmError::TooExpensive);

			// not credited since the ED is higher than this value
			assert_eq!(Assets::balance(1, AccountId::from(ALICE)), 0);

			// We also need to ensure the total supply did not increase
			assert_eq!(Assets::total_supply(1), 0);
		});
}
