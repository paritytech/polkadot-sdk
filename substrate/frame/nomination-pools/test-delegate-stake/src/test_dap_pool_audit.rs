// Security audit tests for nomination pools + DAP reward flow.
//
// These tests focus on the pool reward distribution layer:
// - Reward counter correctness when rewards come via transfer (DAP) vs mint
// - Member join/leave timing around rewards
// - Double-claim prevention
// - Reward account ED preservation
// - Issuance conservation through the full pool claim path
//
// We simulate DAP payout by transferring directly to the pool reward account,
// which is what payout_stakers does in transfer mode.

use frame_support::{
	assert_ok,
	traits::fungible::{Inspect, Mutate},
};
use crate::mock::*;
use pallet_nomination_pools::LastPoolId;
use sp_staking::StakingInterface;

/// Create a pool with depositor, return (pool_id, bonded_account, reward_account).
fn create_pool(
	depositor: u128,
	deposit: u128,
) -> (u32, u128, u128) {
	assert_ok!(Pools::create(
		RuntimeOrigin::signed(depositor),
		deposit,
		depositor,
		depositor,
		depositor,
	));
	let pool_id = LastPoolId::<Runtime>::get();
	let bonded = pallet_nomination_pools::Pallet::<Runtime>::generate_bonded_account(pool_id);
	let reward = pallet_nomination_pools::Pallet::<Runtime>::generate_reward_account(pool_id);
	(pool_id, bonded, reward)
}

/// Simulate a DAP-style reward deposit to the pool's reward account.
/// In production, this comes from payout_stakers transferring from the era pot.
fn deposit_reward(reward_account: u128, amount: u128) {
	// Mint into a temp account and transfer to reward account.
	// This simulates the transfer from era pot without needing full exposure setup.
	let temp = 999_999u128;
	Balances::mint_into(&temp, amount).unwrap();
	assert_ok!(<Balances as Mutate<u128>>::transfer(
		&temp,
		&reward_account,
		amount,
		frame_support::traits::tokens::Preservation::Expendable,
	));
}

#[test]
fn pool_member_claim_bounded_by_reward_account_balance() {
	// Sum of all member claims must not exceed what's in the reward account.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		deposit_reward(reward, 1_000);

		let reward_available = Balances::total_balance(&reward)
			.saturating_sub(ExistentialDeposit::get());

		let depositor_before = Balances::total_balance(&10);
		let member_before = Balances::total_balance(&20);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		let depositor_gain = Balances::total_balance(&10) - depositor_before;
		let member_gain = Balances::total_balance(&20) - member_before;
		let total_claimed = depositor_gain + member_gain;

		assert!(
			total_claimed <= reward_available,
			"Total claimed {} exceeds available reward {}",
			total_claimed, reward_available
		);

		// Equal stake → equal rewards (within 1 unit rounding).
		assert!(depositor_gain > 0 && member_gain > 0);
		assert!(
			depositor_gain.abs_diff(member_gain) <= 1,
			"Equal-stake members should get equal rewards: {} vs {}",
			depositor_gain, member_gain
		);
	});
}

#[test]
fn member_joining_after_reward_deposit_gets_no_retroactive_rewards() {
	// Attack: join pool RIGHT AFTER rewards are deposited.
	// New member's reward counter is set to current pool counter,
	// so they get no share of pre-existing rewards.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);

		// Deposit rewards while only depositor is in pool.
		deposit_reward(reward, 1_000);

		// New member joins AFTER rewards deposited.
		assert_ok!(Pools::join(RuntimeOrigin::signed(21), 50, pool_id));

		// New member claims — should get NOTHING.
		let before = Balances::total_balance(&21);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(21)));
		assert_eq!(
			Balances::total_balance(&21), before,
			"New member should get 0 retroactive rewards"
		);

		// Original depositor gets full rewards.
		let depositor_before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert!(
			Balances::total_balance(&10) > depositor_before,
			"Depositor should receive all rewards"
		);
	});
}

#[test]
fn member_leaving_before_reward_forfeits_unclaimed() {
	// Member unbonds and withdraws before rewards arrive.
	// Their share of future rewards goes to remaining members.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		// Member 20 unbonds.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, 50));

		// Advance past bonding duration so withdrawal is possible.
		<Staking as StakingInterface>::set_era(BondingDuration::get());
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 0));

		// Now rewards arrive — only depositor should get them.
		deposit_reward(reward, 1_000);

		let depositor_before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		let depositor_gain = Balances::total_balance(&10) - depositor_before;

		// Depositor gets nearly all rewards (minor rounding loss possible).
		let reward_available = 1_000u128.saturating_sub(ExistentialDeposit::get());
		assert!(
			depositor_gain >= reward_available - 1,
			"Solo depositor should get almost all rewards: got {}, available {}",
			depositor_gain, reward_available
		);
	});
}

#[test]
fn double_claim_in_same_era_returns_zero() {
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);

		deposit_reward(reward, 500);

		// First claim.
		let before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		let first_gain = Balances::total_balance(&10) - before;
		assert!(first_gain > 0);

		// Second claim — zero.
		let before_second = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_eq!(
			Balances::total_balance(&10), before_second,
			"Second claim should yield nothing"
		);
	});
}

#[test]
fn reward_account_preserves_ed_after_all_claims() {
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		deposit_reward(reward, 1_000);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		assert!(
			Balances::total_balance(&reward) >= ExistentialDeposit::get(),
			"Reward account must keep ED: {}",
			Balances::total_balance(&reward)
		);
	});
}

#[test]
fn multiple_reward_deposits_accumulate_correctly() {
	// Multiple payouts (simulating multi-era) accumulate in reward account.
	// Single claim gets the full accumulated amount.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);

		// 3 separate reward deposits.
		deposit_reward(reward, 100);
		deposit_reward(reward, 200);
		deposit_reward(reward, 300);

		let before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		let gain = Balances::total_balance(&10) - before;

		// Should get all 600 minus any rounding/ED.
		assert!(
			gain >= 595,
			"Should receive ~600 accumulated rewards, got {}",
			gain
		);
	});
}

#[test]
fn proportional_distribution_with_unequal_stakes() {
	// Members with different stakes get proportional rewards.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 20); // 20 stake
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 80, pool_id)); // 80 stake

		deposit_reward(reward, 1_000);

		let before_10 = Balances::total_balance(&10);
		let before_20 = Balances::total_balance(&20);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		let gain_10 = Balances::total_balance(&10) - before_10;
		let gain_20 = Balances::total_balance(&20) - before_20;

		// Member 20 has 4x the stake, should get ~4x the reward.
		// 20/100 * 1000 ≈ 200, 80/100 * 1000 ≈ 800.
		assert!(
			gain_20 > gain_10 * 3,
			"80% stake should get >3x reward of 20% stake: {} vs {}",
			gain_20, gain_10
		);

		let total = gain_10 + gain_20;
		assert!(total <= 1_000, "Total claimed {} exceeds deposited 1000", total);
	});
}

#[test]
fn external_deposit_to_reward_account_distributes_to_members() {
	// Design property: any balance increase in reward account is distributable.
	// An external party sending tokens directly creates claimable rewards.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		// External donation directly to reward account.
		assert_ok!(<Balances as Mutate<u128>>::transfer(
			&22, &reward, 40,
			frame_support::traits::tokens::Preservation::Expendable,
		));

		let before_10 = Balances::total_balance(&10);
		let before_20 = Balances::total_balance(&20);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		let gain_10 = Balances::total_balance(&10) - before_10;
		let gain_20 = Balances::total_balance(&20) - before_20;

		assert!(gain_10 > 0 && gain_20 > 0, "Both members should get donation share");
		assert!(
			gain_10.abs_diff(gain_20) <= 1,
			"Equal-stake members get equal share: {} vs {}", gain_10, gain_20
		);
	});
}

#[test]
fn issuance_conservation_through_full_pool_claim_path() {
	// Deposit → claim chain is all transfers. No issuance change after initial funding.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		let issuance_before = Balances::total_issuance();
		deposit_reward(reward, 1_000);
		let issuance_after_deposit = Balances::total_issuance();

		// deposit_reward mints to temp and transfers. Net issuance change = +1000.
		assert_eq!(issuance_after_deposit - issuance_before, 1_000);

		// Claims are transfers — no issuance change.
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));
		assert_eq!(
			Balances::total_issuance(), issuance_after_deposit,
			"Claims must not change issuance"
		);
	});
}

#[test]
fn bond_extra_from_rewards_does_not_inflate_pool() {
	// Member uses BondExtra::Rewards to compound. Verify no extra tokens created.
	new_test_ext().execute_with(|| {
		let (pool_id, bonded, reward) = create_pool(10, 50);

		deposit_reward(reward, 500);

		let issuance_before = Balances::total_issuance();
		let pool_stake_before = <Staking as StakingInterface>::stake(&bonded)
			.map(|s| s.total)
			.unwrap_or(0);

		// Bond extra from rewards (compounds rewards into stake).
		assert_ok!(Pools::bond_extra(
			RuntimeOrigin::signed(10),
			pallet_nomination_pools::BondExtra::Rewards,
		));

		let issuance_after = Balances::total_issuance();
		let pool_stake_after = <Staking as StakingInterface>::stake(&bonded)
			.map(|s| s.total)
			.unwrap_or(0);

		// Pool stake increased (rewards compounded).
		assert!(
			pool_stake_after > pool_stake_before,
			"Pool stake should increase from compounded rewards"
		);

		// Issuance unchanged (rewards transferred from reward account, not minted).
		assert_eq!(issuance_before, issuance_after, "BondExtra::Rewards must not mint");
	});
}

#[test]
fn slash_does_not_affect_unclaimed_rewards() {
	// Pool gets slashed on bonded stake. Unclaimed rewards in reward account are safe.
	new_test_ext().execute_with(|| {
		let (pool_id, bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		// Deposit rewards.
		deposit_reward(reward, 1_000);
		let reward_before_slash = Balances::total_balance(&reward);

		// Slash the pool's bonded stake (not the reward account).
		pallet_staking_async::slashing::do_slash::<Runtime>(
			&bonded,
			30,
			&mut Default::default(),
			&mut Default::default(),
			0,
		);

		// Reward account is untouched by slash.
		let reward_after_slash = Balances::total_balance(&reward);
		assert_eq!(
			reward_before_slash, reward_after_slash,
			"Slash must not affect reward account"
		);

		// Members can still claim full rewards.
		let before_10 = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert!(
			Balances::total_balance(&10) > before_10,
			"Member should still claim rewards after pool slash"
		);
	});
}
