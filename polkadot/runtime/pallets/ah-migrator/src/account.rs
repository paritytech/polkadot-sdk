// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Account/Balance data migrator module.

/*
provider refs:
- crowdloans: fundraising system account / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/polkadot/runtime/common/src/crowdloan/mod.rs#L416
- parachains_assigner_on_demand / on_demand: pallet's account https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/polkadot/runtime/parachains/src/on_demand/mod.rs#L407
- balances: user account / existential deposit
- session: initial validator set on Genesis / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L466
- delegated-staking: delegators and agents (users)

consumer refs:
- balances:
-- might hold on account mutation / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/balances/src/lib.rs#L1007
-- on migration to new logic for every migrating account / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/balances/src/lib.rs#L877
- session:
-- for user setting the keys / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L812
-- initial validator set on Genesis / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/session/src/lib.rs#L461
- recovery: user on recovery claim / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/recovery/src/lib.rs#L610
- staking:
-- for user bonding / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/staking/src/pallet/mod.rs#L1036
-- virtual bond / agent key / https://github.com/paritytech/polkadot-sdk/blob/ace62f120fbc9ec617d6bab0a5180f0be4441537/substrate/frame/staking/src/pallet/impls.rs#L1948

sufficient refs:
- must be zero since only assets pallet might hold such reference
*/

use crate::*;
use frame_system::Account as SystemAccount;

impl<T: Config> Pallet<T> {
	pub fn migrate_accounts() {
		for (account, account_info) in SystemAccount::<T>::iter() {
			assert!(account_info.consumers < 3, "Account has more than 2 consumers");
			assert!(account_info.providers < 3, "Account has more than 2 providers");
			assert!(account_info.sufficients == 0, "Account has more than 0 sufficient");
		}
	}
}
