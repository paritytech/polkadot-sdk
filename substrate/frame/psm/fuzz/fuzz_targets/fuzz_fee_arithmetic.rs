#![no_main]

use frame_support::traits::fungibles::Mutate;
use libfuzzer_sys::fuzz_target;
use pallet_psm::mock::{
	new_test_ext, Assets, Psm, RuntimeOrigin, Test, ALICE, PUSD_UNIT, USDC_ASSET_ID,
};
use pallet_psm::PsmDebt;
use sp_runtime::Permill;

const MIN_SWAP: u128 = PUSD_UNIT * 100;

fuzz_target!(|input: (u16, u32, u32)| {
	let (amount_multiplier, mint_fee_parts, redeem_fee_parts) = input;

	let mint_fee = Permill::from_parts(mint_fee_parts % 1_000_001);
	let redeem_fee = Permill::from_parts(redeem_fee_parts % 1_000_001);
	let amount = (amount_multiplier as u128 + 1).saturating_mul(MIN_SWAP);

	new_test_ext().execute_with(|| {
		let _ = Psm::set_minting_fee(RuntimeOrigin::root(), USDC_ASSET_ID, mint_fee);
		let _ = Psm::set_redemption_fee(RuntimeOrigin::root(), USDC_ASSET_ID, redeem_fee);

		let _ = Assets::mint_into(USDC_ASSET_ID, &ALICE, amount.saturating_mul(2));

		if Psm::mint(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, amount).is_ok() {
			let debt_after_mint = PsmDebt::<Test>::get(USDC_ASSET_ID);
			let reserve_after_mint = pallet_psm::Pallet::<Test>::get_reserve(USDC_ASSET_ID);

			assert!(
				reserve_after_mint >= debt_after_mint,
				"reserve ({}) < debt ({}) after mint",
				reserve_after_mint,
				debt_after_mint
			);
			assert_eq!(debt_after_mint, amount, "debt should equal external_amount deposited");
			assert_eq!(
				reserve_after_mint, debt_after_mint,
				"reserve should equal debt after clean mint"
			);

			let pusd_balance = Assets::balance(pallet_psm::mock::PUSD_ASSET_ID, ALICE);
			let expected_fee = mint_fee.mul_ceil(amount);
			let expected_pusd = amount.saturating_sub(expected_fee);
			assert_eq!(pusd_balance, expected_pusd, "pUSD to user does not match expected");

			if !mint_fee.is_zero() && pusd_balance >= MIN_SWAP {
				let _ = Psm::redeem(RuntimeOrigin::signed(ALICE), USDC_ASSET_ID, pusd_balance);
				let residual_debt = PsmDebt::<Test>::get(USDC_ASSET_ID);
				assert!(
					residual_debt > 0,
					"non-zero mint fee should leave residual debt after redeeming user pUSD"
				);
			}

			pallet_psm::Pallet::<Test>::do_try_state()
				.expect("PSM invariant violated after fee arithmetic fuzz");
		}
	});
});
