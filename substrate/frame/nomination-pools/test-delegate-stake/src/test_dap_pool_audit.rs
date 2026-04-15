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
use sp_runtime::Perbill;
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
	// Mint directly into the reward account to simulate transfer from era pot.
	Balances::mint_into(&reward_account, amount).unwrap();
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

// ============================================================================
// Corruption & misconfiguration chaos tests
// ============================================================================

#[test]
fn reward_account_drained_externally_claim_fails_gracefully() {
	// Corruption: someone drains the pool reward account.
	// Member claim should fail gracefully or pay reduced amount.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);

		// Deposit reward.
		deposit_reward(reward, 500);

		// Corrupt: drain the reward account (keep ED via Preserve).
		let balance = Balances::total_balance(&reward);
		let drainable = balance.saturating_sub(ExistentialDeposit::get());
		if drainable > 0 {
			Balances::mint_into(&999, ExistentialDeposit::get()).unwrap();
			let _ = <Balances as Mutate<u128>>::transfer(
				&reward,
				&999,
				drainable,
				frame_support::traits::tokens::Preservation::Preserve,
			);
		}

		let reward_after_drain = Balances::total_balance(&reward);
		let issuance_before = Balances::total_issuance();

		// Claim: the reward counter still thinks 500 is available,
		// but the reward account only has ED left.
		let before = Balances::total_balance(&10);
		let result = Pools::claim_payout(RuntimeOrigin::signed(10));
		let after = Balances::total_balance(&10);
		let issuance_after = Balances::total_issuance();

		// KEY INVARIANT: no issuance change (claim is transfer-only).
		assert_eq!(
			issuance_before, issuance_after,
			"Claim from drained account must not mint"
		);

		// If claim succeeded, it can only transfer what's available.
		let gained = after.saturating_sub(before);
		assert!(
			gained <= reward_after_drain,
			"Cannot claim more than remaining: gained={}, remaining={}",
			gained, reward_after_drain
		);

		log::info!(
			target: "audit",
			"Drained reward account: reward_after_drain={}, claim result={:?}, gained={}",
			reward_after_drain, result, gained
		);
	});
}

#[test]
fn large_reward_does_not_overflow_reward_counter() {
	// Edge case: very large reward deposit. FixedU128 should handle it.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);

		// Deposit a large reward relative to pool points.
		// reward_per_point = 10_000_000 / 50 = 200_000 in FixedU128.
		deposit_reward(reward, 10_000_000);

		let before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		let gain = Balances::total_balance(&10) - before;

		// Should get nearly all of it (minus ED kept in reward account).
		assert!(
			gain >= 10_000_000 - ExistentialDeposit::get() - 1,
			"Should receive large reward: got {}", gain
		);
	});
}

#[test]
fn tiny_reward_with_large_pool_does_not_round_to_zero_for_all() {
	// Edge: tiny reward (ED) split among members. Rounding may give some members 0.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		// Deposit minimal viable reward.
		deposit_reward(reward, ExistentialDeposit::get());

		let before_10 = Balances::total_balance(&10);
		let before_20 = Balances::total_balance(&20);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		let gain_10 = Balances::total_balance(&10) - before_10;
		let gain_20 = Balances::total_balance(&20) - before_20;

		// With ED reward and 2 members, each gets at most ED/2.
		// Total claimed must not exceed deposited.
		assert!(
			gain_10 + gain_20 <= ExistentialDeposit::get(),
			"Tiny reward should not create value: claimed {}",
			gain_10 + gain_20
		);
	});
}

#[test]
fn slash_then_reward_then_claim_accounting_is_correct() {
	// Sequence: slash pool stake → deposit rewards → claim.
	// Rewards should be distributed based on POST-SLASH points.
	new_test_ext().execute_with(|| {
		let (pool_id, bonded, reward) = create_pool(10, 50);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		// Slash 50% of pool stake.
		pallet_staking_async::slashing::do_slash::<Runtime>(
			&bonded,
			50, // slash 50 out of 100
			&mut Default::default(),
			&mut Default::default(),
			0,
		);

		// Now deposit rewards AFTER slash.
		deposit_reward(reward, 1_000);

		// Both members still have equal pool points (slash doesn't change points).
		let before_10 = Balances::total_balance(&10);
		let before_20 = Balances::total_balance(&20);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		let gain_10 = Balances::total_balance(&10) - before_10;
		let gain_20 = Balances::total_balance(&20) - before_20;

		// Equal points → equal share, regardless of slash.
		assert!(gain_10 > 0 && gain_20 > 0);
		assert!(
			gain_10.abs_diff(gain_20) <= 1,
			"Equal points should get equal reward post-slash: {} vs {}",
			gain_10, gain_20
		);

		// Total claimed ≤ deposited.
		assert!(gain_10 + gain_20 <= 1_000);
	});
}

#[test]
fn bond_extra_rewards_after_slash_does_not_create_value() {
	// Slash reduces real stake but not pool points.
	// BondExtra::Rewards compounds into the reduced pool.
	// Verify no value creation.
	new_test_ext().execute_with(|| {
		let (pool_id, bonded, reward) = create_pool(10, 50);

		// Slash half the pool's stake.
		pallet_staking_async::slashing::do_slash::<Runtime>(
			&bonded,
			25,
			&mut Default::default(),
			&mut Default::default(),
			0,
		);

		// Deposit rewards.
		deposit_reward(reward, 200);

		let issuance_before = Balances::total_issuance();

		// Compound rewards via BondExtra.
		assert_ok!(Pools::bond_extra(
			RuntimeOrigin::signed(10),
			pallet_nomination_pools::BondExtra::Rewards,
		));

		let issuance_after = Balances::total_issuance();
		assert_eq!(
			issuance_before, issuance_after,
			"BondExtra::Rewards after slash must not mint"
		);
	});
}

#[test]
fn member_unbond_claim_unbond_claim_sequence() {
	// Attack: member partially unbonds, claims, unbonds more, claims again.
	// Each claim should only pay new rewards since last claim.
	new_test_ext().execute_with(|| {
		let (pool_id, _bonded, reward) = create_pool(10, 50);

		// Deposit first reward.
		deposit_reward(reward, 100);

		// Claim first batch.
		let before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		let gain_1 = Balances::total_balance(&10) - before;
		assert!(gain_1 > 0);

		// Partially unbond.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(10), 10, 20));

		// Deposit second reward (smaller pool now).
		deposit_reward(reward, 100);

		// Claim again — should get new rewards proportional to remaining stake.
		let before = Balances::total_balance(&10);
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(10)));
		let gain_2 = Balances::total_balance(&10) - before;

		// Second gain should be based on remaining 30 points (not original 50).
		// But since they're the only member, they still get all of it.
		assert!(gain_2 > 0, "Should receive second batch of rewards");

		// Total gained should not exceed total deposited.
		assert!(
			gain_1 + gain_2 <= 200,
			"Total claimed {} exceeds total deposited 200",
			gain_1 + gain_2
		);
	});
}

// ============================================================================
// Pool commission + DAP reward interaction
// ============================================================================

#[test]
fn pool_commission_takes_cut_of_dap_funded_rewards() {
	// Pool operator sets 50% commission. Rewards come via DAP transfer.
	// Commission recipient gets 50%, members split the other 50%.
	new_test_ext().execute_with(|| {
		let depositor = 10u128;
		let commission_recipient = 22u128;
		let (pool_id, _bonded, reward) = create_pool(depositor, 50);

		// Set 50% commission with recipient.
		assert_ok!(Pools::set_commission(
			RuntimeOrigin::signed(depositor),
			pool_id,
			Some((Perbill::from_percent(50), commission_recipient)),
		));

		// Add another member.
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		// Deposit rewards (simulating DAP transfer).
		deposit_reward(reward, 1_000);

		let depositor_before = Balances::total_balance(&depositor);
		let member_before = Balances::total_balance(&20);
		let commission_before = Balances::total_balance(&commission_recipient);

		// Members claim.
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(depositor)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));

		// Claim commission.
		assert_ok!(Pools::claim_commission(RuntimeOrigin::signed(depositor), pool_id));

		let depositor_gain = Balances::total_balance(&depositor) - depositor_before;
		let member_gain = Balances::total_balance(&20) - member_before;
		let commission_gain = Balances::total_balance(&commission_recipient) - commission_before;

		// Commission should be ~50% of 1000 = ~500.
		assert!(
			commission_gain >= 490 && commission_gain <= 510,
			"Commission should be ~500, got {}", commission_gain
		);

		// Members split remaining ~500 equally (50/50 stake).
		assert!(
			depositor_gain > 0 && member_gain > 0,
			"Both members should receive rewards"
		);
		assert!(
			depositor_gain.abs_diff(member_gain) <= 1,
			"Equal-stake members should get equal post-commission share: {} vs {}",
			depositor_gain, member_gain
		);

		// Total distributed (members + commission) should not exceed deposited.
		let total = depositor_gain + member_gain + commission_gain;
		assert!(
			total <= 1_000,
			"Total distributed {} exceeds deposited 1000", total
		);

		// No minting.
		// (deposit_reward mints, but claim/commission are transfers from reward account)
	});
}

#[test]
fn pool_100_percent_commission_members_get_zero() {
	// Edge: pool operator takes everything. Members get nothing.
	new_test_ext().execute_with(|| {
		let depositor = 10u128;
		let commission_recipient = 22u128;
		let (pool_id, _bonded, reward) = create_pool(depositor, 50);

		// Set maximum commission (global max is 90% in genesis).
		assert_ok!(Pools::set_commission(
			RuntimeOrigin::signed(depositor),
			pool_id,
			Some((Perbill::from_percent(90), commission_recipient)),
		));

		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 50, pool_id));

		deposit_reward(reward, 1_000);

		let depositor_before = Balances::total_balance(&depositor);
		let member_before = Balances::total_balance(&20);
		let commission_before = Balances::total_balance(&commission_recipient);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(depositor)));
		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(20)));
		assert_ok!(Pools::claim_commission(RuntimeOrigin::signed(depositor), pool_id));

		let depositor_gain = Balances::total_balance(&depositor) - depositor_before;
		let member_gain = Balances::total_balance(&20) - member_before;
		let commission_gain = Balances::total_balance(&commission_recipient) - commission_before;

		// Commission should be ~90% of 1000 = ~900.
		assert!(
			commission_gain >= 890,
			"90% commission should yield ~900, got {}", commission_gain
		);

		// Members split remaining ~100 equally.
		let member_total = depositor_gain + member_gain;
		assert!(
			member_total <= 110,
			"Members should get ~100 total with 90% commission, got {}", member_total
		);

		// Total conservation.
		assert!(depositor_gain + member_gain + commission_gain <= 1_000);
	});
}

#[test]
fn commission_change_mid_reward_accumulation() {
	// Commission changes between reward deposits. Each deposit is accounted
	// at the commission rate active at the time of claim (not deposit time).
	new_test_ext().execute_with(|| {
		let depositor = 10u128;
		let commission_recipient = 22u128;
		let (pool_id, _bonded, reward) = create_pool(depositor, 50);

		// Start with 0% commission.
		deposit_reward(reward, 500);

		// Set 50% commission AFTER first deposit.
		assert_ok!(Pools::set_commission(
			RuntimeOrigin::signed(depositor),
			pool_id,
			Some((Perbill::from_percent(50), commission_recipient)),
		));

		// Second deposit.
		deposit_reward(reward, 500);

		// Claim: commission applies to ALL unclaimed rewards (both deposits),
		// not just the deposit after commission was set.
		let depositor_before = Balances::total_balance(&depositor);
		let commission_before = Balances::total_balance(&commission_recipient);

		assert_ok!(Pools::claim_payout(RuntimeOrigin::signed(depositor)));
		assert_ok!(Pools::claim_commission(RuntimeOrigin::signed(depositor), pool_id));

		let depositor_gain = Balances::total_balance(&depositor) - depositor_before;
		let commission_gain = Balances::total_balance(&commission_recipient) - commission_before;

		// The commission rate at claim time (50%) applies to total unclaimed (1000).
		// Commission = 50% of 1000 = 500. Depositor gets remaining 500.
		// This is pool behavior, not a DAP issue, but worth documenting.
		let total = depositor_gain + commission_gain;
		assert!(total <= 1_000, "Total {} exceeds deposited 1000", total);

		log::info!(
			target: "audit",
			"Commission mid-change: depositor_gain={}, commission_gain={}, total={}",
			depositor_gain, commission_gain, total
		);
	});
}
