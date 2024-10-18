// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use super::*;
use crate::{
	mock::{expect_events, new_tester, AccountId, EthereumRewards, Test, WETH},
	Event as RewardEvent,
};
use frame_support::assert_ok;
use sp_keyring::AccountKeyring as Keyring;
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
