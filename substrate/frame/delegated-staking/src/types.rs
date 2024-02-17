// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Basic types used in delegated staking.

use super::*;

/// The type of pot account being created.
#[derive(Encode, Decode)]
pub(crate) enum AccountType {
    /// Funds that are withdrawn from the staking ledger but not claimed by the `delegator` yet.
    UnclaimedWithdrawal,
    /// A proxy delegator account created for a nominator who migrated to a `delegate` account.
    ///
    /// Funds for unmigrated `delegator` accounts of the `delegate` are kept here.
    ProxyDelegator,
}

#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Delegation<T: Config> {
    // The target of delegation.
    pub delegate: T::AccountId,
    // The amount delegated.
    pub amount: BalanceOf<T>,
}

/// Ledger of all delegations to a `Delegate`.
///
/// This keeps track of the active balance of the `delegate` that is made up from the funds that are
/// currently delegated to this `delegate`. It also tracks the pending slashes yet to be applied
/// among other things.
#[derive(Default, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct DelegationLedger<T: Config> {
    /// Where the reward should be paid out.
    pub payee: T::AccountId,
    /// Sum of all delegated funds to this `delegate`.
    #[codec(compact)]
    pub total_delegated: BalanceOf<T>,
    /// Amount that is bonded and held.
    // FIXME(ank4n) (can we remove it)
    #[codec(compact)]
    pub hold: BalanceOf<T>,
    /// Slashes that are not yet applied.
    #[codec(compact)]
    pub pending_slash: BalanceOf<T>,
    /// Whether this `delegate` is blocked from receiving new delegations.
    pub blocked: bool,
    /// The `delegate` account associated with the ledger.
    #[codec(skip)]
    pub delegate: Option<T::AccountId>,
}



impl<T: Config> DelegationLedger<T> {
    pub fn new(reward_destination: &T::AccountId) -> Self {
        DelegationLedger {
            payee: reward_destination.clone(),
            total_delegated: Zero::zero(),
            hold: Zero::zero(),
            pending_slash: Zero::zero(),
            blocked: false,
            delegate: None,
        }
    }

    /// consumes self and returns a new copy with the key.
    fn with_key(self, key: &T::AccountId) -> Self {
        DelegationLedger { delegate: Some(key.clone()), ..self }
    }

    fn get(key: &T::AccountId) -> Option<Self> {
        <Delegates<T>>::get(key).map(|d| d.with_key(key))
    }

    /// Balance that is stakeable.
    pub fn delegated_balance(&self) -> BalanceOf<T> {
        // do not allow to stake more than unapplied slash
        self.total_delegated.saturating_sub(self.pending_slash)
    }

    /// Balance that is delegated but not bonded.
    ///
    /// Can be funds that are unbonded but not withdrawn.
    pub fn unbonded_balance(&self) -> BalanceOf<T> {
        self.total_delegated.saturating_sub(self.hold)
    }

    pub fn save(self, key: &T::AccountId) -> Self {
        let new_val = self.with_key(key);
        <Delegates<T>>::insert(key, &new_val);

        new_val
    }
}
