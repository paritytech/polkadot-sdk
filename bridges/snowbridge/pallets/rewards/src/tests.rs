// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use super::*;
use crate::{
	mock::{expect_events, new_tester, AccountId, EthereumRewards, Test, WETH},
	Event as RewardEvent,
};
use frame_support::assert_ok;
use sp_keyring::AccountKeyring as Keyring;
use crate::mock::RuntimeOrigin;
use sp_core::H256;
use frame_support::assert_err;
#[test]
fn test_deposit() {
	new_tester().execute_with(|| {
		// Check a new deposit works
		let relayer: AccountId = Keyring::Bob.into();
		let result = EthereumRewards::deposit(relayer.clone().into(), 2 * WETH);
		assert_ok!(result);
		assert_eq!(<RewardsMapping<Test>>::get(relayer.clone()), 2 * WETH);
		//expect_events(vec![RewardEvent::RewardDeposited {
		//   account_id: relayer.clone(), value: 2 * WETH
		//}]);

		// Check accumulation works
		let result2 = EthereumRewards::deposit(relayer.clone().into(), 3 * WETH);
		assert_ok!(result2);
		assert_eq!(<RewardsMapping<Test>>::get(relayer), 5 * WETH);

		// Check another relayer deposit works.
		let another_relayer: AccountId = Keyring::Ferdie.into();
		let result3 = EthereumRewards::deposit(another_relayer.clone().into(), 1 * WETH);
		assert_ok!(result3);
		assert_eq!(<RewardsMapping<Test>>::get(another_relayer), 1 * WETH);
	});
}

#[test]
fn test_claim() {
	new_tester().execute_with(|| {
		let relayer: AccountId = Keyring::Bob.into();
		let message_id = H256::random();

		let result =
			EthereumRewards::claim(RuntimeOrigin::signed(relayer.clone()), relayer.clone(), 3 * WETH, message_id);
		// No rewards yet
		assert_err!(result, Error::<Test>::InsufficientFunds);

		// Deposit rewards
		let result2 =
			EthereumRewards::deposit(relayer.clone(), 3 * WETH);
		assert_ok!(result2);

		// Claim some rewards
		let result3 =
			EthereumRewards::claim(RuntimeOrigin::signed(relayer.clone()), relayer.clone(), 2 * WETH, message_id);
		assert_ok!(result3);

		// Claim some rewards than available
		let result4 =
			EthereumRewards::claim(RuntimeOrigin::signed(relayer.clone()), relayer.clone(), 2 * WETH, message_id);
		assert_err!(result4, Error::<Test>::InsufficientFunds);

		// Claim the remaining balance
		let result5 =
			EthereumRewards::claim(RuntimeOrigin::signed(relayer.clone()), relayer, 1 * WETH, message_id);
		assert_ok!(result5);
	});
}
