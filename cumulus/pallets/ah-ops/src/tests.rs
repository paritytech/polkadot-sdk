// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use frame_support::{
	hypothetically,
	traits::{fungible::Mutate, InspectLockableCurrency, LockableCurrency, WithdrawReasons},
};
use sp_runtime::AccountId32;
use std::str::FromStr;

use crate::mock::Runtime as AssetHub;

#[test]
fn sovereign_account_translation() {
	// https://docs.google.com/document/d/1DXYWPXEwi0DkDfG8Fb2ZTI4DQBAz87DBCIW7yQIVrj0
	let bifrost_cases = [
		// Bifrost Polkadot #1
		(
			// para 2030
			"13YMK2eeopZtUNpeHnJ1Ws2HqMQG6Ts9PGCZYGyFbSYoZfcm",
			// sibl 2030
			"13cKp89TtYknbyYnqnF6dWN75q5ZosvFSuqzoEVkUAaNR47A",
			None,
		),
		// Bifrost Polkadot #2
		(
			// para 2030 index 0
			"14vtfeKAVKh1Jzb3s7e43SqZ3zB5MLsdCxZPoKDxeoCFKLu5",
			// sibl 2030 index 0
			"5ETehspFKFNpBbe5DsfuziN6BWq5Qwp1J8qcTQQoAxwa7BsS",
			// derivation proof (para 2030, index 0)
			Some(("13YMK2eeopZtUNpeHnJ1Ws2HqMQG6Ts9PGCZYGyFbSYoZfcm", 0u16)),
		),
		// Bifrost Polkadot #3
		(
			// para 2030 index 1
			"14QkQ7wVVDRrhbC1UqHsFwKFUns1SRud94CXMWGHWB8Jhtro",
			// sibl 2030 index 1
			"5DNWZkkAxLhqF8tevcbRGyARAVM7abukftmqvoDFUN5dDDDz",
			// derivation proof (para 2030, index 1)
			Some(("13YMK2eeopZtUNpeHnJ1Ws2HqMQG6Ts9PGCZYGyFbSYoZfcm", 1u16)),
		),
		// Bifrost Polkadot #4
		(
			// para 2030 index 2
			"13hLwqcVHqjiJMbZhR9LtfdhoxmTdssi7Kp8EJaW2yfk3knK",
			// sibl 2030 index 2
			"5EmiwjDYiackJma1GW3aBbQ74rLfWh756UKDb7Cm83XDkUUZ",
			// derivation proof (para 2030, index 2)
			Some(("13YMK2eeopZtUNpeHnJ1Ws2HqMQG6Ts9PGCZYGyFbSYoZfcm", 2u16)),
		),
		// Bifrost Kusama #1
		(
			// para 2001
			"5Ec4AhPV91i9yNuiWuNunPf6AQCYDhFTTA4G5QCbtqYApH9E",
			// sibl 2001
			"5Eg2fntJDju46yds4uKzu2zuQssqw7JZWohhLMj6mZZjg2pK",
			None,
		),
		// Bifrost Kusama #2
		(
			// para 2001 index 0
			"5E78xTBiaN3nAGYtcNnqTJQJqYAkSDGggKqaDfpNsKyPpbcb",
			// sibl 2001 index 0
			"5CzXNqgBZT5yMpMETdfH55saYNKQoJBXsSfnu4d2s1ejYFir",
			// derivation proof (para 2001, index 0)
			Some(("5Ec4AhPV91i9yNuiWuNunPf6AQCYDhFTTA4G5QCbtqYApH9E", 0u16)),
		),
		// Bifrost Kusama #3
		(
			// para 2001 index 1
			"5HXi9pzWnTQzk7VKzY6VQn92KfWCcA5NbSm53uKHrYU1VsjP",
			// sibl 2001 index 1
			"5GcexD4YNqcKTbW1YWDRczQzpxic61byeNeLaHgqQHk8pxQJ",
			// derivation proof (para 2001, index 1)
			Some(("5Ec4AhPV91i9yNuiWuNunPf6AQCYDhFTTA4G5QCbtqYApH9E", 1u16)),
		),
		// Bifrost Kusama #4
		(
			// para 2001 index 2
			"5CkKS3YMx64TguUYrMERc5Bn6Mn2aKMUkcozUFREQDgHS3Tv",
			// sibl 2001 index 2
			"5FoYMVucmT552GDMWfYNxcF2XnuuvLbJHt7mU6DfDCpUAS2Y",
			// derivation proof (para 2001, index 2)
			Some(("5Ec4AhPV91i9yNuiWuNunPf6AQCYDhFTTA4G5QCbtqYApH9E", 2u16)),
		),
		// Bifrost Kusama #5
		(
			// para 2001 index 3
			"5Crxhmiw5CQq3Mnfcu3dR3yJ3YpjbxjqaeDFtNNtqgmcnN4S",
			// sibl 2001 index 3
			"5FP39fgPYhJw3vcLwSMqMnwBuEVGexUMG6JQLPR9yPVhq6Wy",
			// derivation proof (para 2001, index 3)
			Some(("5Ec4AhPV91i9yNuiWuNunPf6AQCYDhFTTA4G5QCbtqYApH9E", 3u16)),
		),
		// Bifrost Kusama #5
		(
			// para 2001 index 3
			"5DAZP4gZKZafGv42uoWNTMau4tYuDd2XteJLGL4upermhQpn",
			// sibl 2001 index 3
			"5ExtLdYnjHLJbngU1QpumjPieCGaCXwwkH1JrFBQ9GATuNGv",
			// derivation proof (para 2001, index 4)
			Some(("5Ec4AhPV91i9yNuiWuNunPf6AQCYDhFTTA4G5QCbtqYApH9E", 4u16)),
		),
	];

	for (from, to, derivation) in bifrost_cases {
		let from = AccountId32::from_str(from).unwrap();
		let to = AccountId32::from_str(to).unwrap();

		println!("Translating {from}/{derivation:?} -> {to}");
		if let Some((parent, index)) = derivation {
			let parent = AccountId32::from_str(parent).unwrap();
			let (got_to, _) =
				crate::Pallet::<AssetHub>::try_rc_sovereign_derived_to_ah(&from, &parent, index)
					.unwrap();
			assert_eq!(got_to, to);
		} else {
			let (got_to, _) =
				crate::Pallet::<AssetHub>::try_translate_rc_sovereign_to_ah(&from).unwrap();
			assert_eq!(got_to, to);
		}
	}
}

#[test]
fn translate_sovereign_acc_good() {
	sp_io::TestExternalities::new(Default::default()).execute_with(move || {
		let balance = 1000000000000000000;
		let lock = balance / 20;
		const LID: [u8; 8] = *b"lockID00";

		// Test for Para 2030 index 0 (Bifrost Polkadot derived 0)
		let from =
			AccountId32::from_str("14vtfeKAVKh1Jzb3s7e43SqZ3zB5MLsdCxZPoKDxeoCFKLu5").unwrap();
		let to = AccountId32::from_str("5ETehspFKFNpBbe5DsfuziN6BWq5Qwp1J8qcTQQoAxwa7BsS").unwrap();
		let parent =
			AccountId32::from_str("13YMK2eeopZtUNpeHnJ1Ws2HqMQG6Ts9PGCZYGyFbSYoZfcm").unwrap();
		let derivation_proof = Some((parent, 0u16));

		// Works if the account does not exist
		hypothetically!({
			crate::Pallet::<AssetHub>::do_migrate_parachain_sovereign_derived_acc(
				&from,
				&to,
				derivation_proof.clone(),
			)
			.unwrap();
			// Also twice
			crate::Pallet::<AssetHub>::do_migrate_parachain_sovereign_derived_acc(
				&from,
				&to,
				derivation_proof.clone(),
			)
			.unwrap();
		});

		// But also if it exists
		<AssetHub as crate::Config>::Currency::mint_into(&from, balance).unwrap();
		hypothetically!({
			crate::Pallet::<AssetHub>::do_migrate_parachain_sovereign_derived_acc(
				&from,
				&to,
				derivation_proof.clone(),
			)
			.unwrap();
			// Also twice
			crate::Pallet::<AssetHub>::do_migrate_parachain_sovereign_derived_acc(
				&from,
				&to,
				derivation_proof.clone(),
			)
			.unwrap();

			// Balance was moved
			assert_eq!(<AssetHub as crate::Config>::Currency::free_balance(&from), 0);
			assert_eq!(<AssetHub as crate::Config>::Currency::free_balance(&to), balance);
		});

		// Can also have locks
		<AssetHub as crate::Config>::Currency::set_lock(LID, &from, lock, WithdrawReasons::FEE);
		hypothetically!({
			crate::Pallet::<AssetHub>::do_migrate_parachain_sovereign_derived_acc(
				&from,
				&to,
				derivation_proof.clone(),
			)
			.unwrap();

			// Balance was moved
			assert_eq!(<AssetHub as crate::Config>::Currency::free_balance(&from), 0);
			assert_eq!(<AssetHub as crate::Config>::Currency::free_balance(&to), balance);

			// Lock was moved
			assert_eq!(<AssetHub as crate::Config>::Currency::balance_locked(LID, &from), 0);
			assert_eq!(<AssetHub as crate::Config>::Currency::balance_locked(LID, &to), lock);
		});
	});
}
