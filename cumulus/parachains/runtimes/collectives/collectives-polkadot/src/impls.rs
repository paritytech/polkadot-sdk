// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use crate::OriginCaller;
use frame_support::{
	dispatch::{DispatchError, DispatchResultWithPostInfo},
	log,
	traits::{Currency, Get, Imbalance, OnUnbalanced, OriginTrait, PrivilegeCmp},
	weights::Weight,
};
use pallet_alliance::{ProposalIndex, ProposalProvider};
use parachains_common::impls::NegativeImbalance;
use sp_std::{cmp::Ordering, marker::PhantomData, prelude::*};
use xcm::latest::{Fungibility, Junction, Parent};

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

type ProposalOf<T, I> = <T as pallet_collective::Config<I>>::Proposal;

type HashOf<T> = <T as frame_system::Config>::Hash;

/// Type alias to conveniently refer to the `Currency::Balance` associated type.
pub type BalanceOf<T> =
	<pallet_balances::Pallet<T> as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Implements `OnUnbalanced::on_unbalanced` to teleport slashed assets to relay chain treasury account.
pub struct ToParentTreasury<TreasuryAccount, PalletAccount, T>(
	PhantomData<(TreasuryAccount, PalletAccount, T)>,
);

impl<TreasuryAccount, PalletAccount, T> OnUnbalanced<NegativeImbalance<T>>
	for ToParentTreasury<TreasuryAccount, PalletAccount, T>
where
	T: pallet_balances::Config + pallet_xcm::Config + frame_system::Config,
	<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId: From<AccountIdOf<T>>,
	[u8; 32]: From<<T as frame_system::Config>::AccountId>,
	TreasuryAccount: Get<AccountIdOf<T>>,
	PalletAccount: Get<AccountIdOf<T>>,
	BalanceOf<T>: Into<Fungibility>,
{
	fn on_unbalanced(amount: NegativeImbalance<T>) {
		let amount = match amount.drop_zero() {
			Ok(..) => return,
			Err(amount) => amount,
		};
		let imbalance = amount.peek();
		let pallet_acc: AccountIdOf<T> = PalletAccount::get();
		let treasury_acc: AccountIdOf<T> = TreasuryAccount::get();

		<pallet_balances::Pallet<T>>::resolve_creating(&pallet_acc, amount);

		let result = <pallet_xcm::Pallet<T>>::teleport_assets(
			<<T as frame_system::Config>::RuntimeOrigin>::signed(pallet_acc.into()),
			Box::new(Parent.into()),
			Box::new(
				Junction::AccountId32 { network: None, id: treasury_acc.into() }
					.into_location()
					.into(),
			),
			Box::new((Parent, imbalance).into()),
			0,
		);

		if let Err(err) = result {
			log::warn!("Failed to teleport slashed assets: {:?}", err);
		}
	}
}

/// Proposal provider for alliance pallet.
/// Adapter from collective pallet to alliance proposal provider trait.
pub struct AllianceProposalProvider<T, I = ()>(PhantomData<(T, I)>);

impl<T, I> ProposalProvider<AccountIdOf<T>, HashOf<T>, ProposalOf<T, I>>
	for AllianceProposalProvider<T, I>
where
	T: pallet_collective::Config<I> + frame_system::Config,
	I: 'static,
{
	fn propose_proposal(
		who: AccountIdOf<T>,
		threshold: u32,
		proposal: Box<ProposalOf<T, I>>,
		length_bound: u32,
	) -> Result<(u32, u32), DispatchError> {
		pallet_collective::Pallet::<T, I>::do_propose_proposed(
			who,
			threshold,
			proposal,
			length_bound,
		)
	}

	fn vote_proposal(
		who: AccountIdOf<T>,
		proposal: HashOf<T>,
		index: ProposalIndex,
		approve: bool,
	) -> Result<bool, DispatchError> {
		pallet_collective::Pallet::<T, I>::do_vote(who, proposal, index, approve)
	}

	fn close_proposal(
		proposal_hash: HashOf<T>,
		proposal_index: ProposalIndex,
		proposal_weight_bound: Weight,
		length_bound: u32,
	) -> DispatchResultWithPostInfo {
		pallet_collective::Pallet::<T, I>::do_close(
			proposal_hash,
			proposal_index,
			proposal_weight_bound,
			length_bound,
		)
	}

	fn proposal_of(proposal_hash: HashOf<T>) -> Option<ProposalOf<T, I>> {
		pallet_collective::Pallet::<T, I>::proposal_of(proposal_hash)
	}
}

/// Used to compare the privilege of an origin inside the scheduler.
pub struct EqualOrGreatestRootCmp;

impl PrivilegeCmp<OriginCaller> for EqualOrGreatestRootCmp {
	fn cmp_privilege(left: &OriginCaller, right: &OriginCaller) -> Option<Ordering> {
		if left == right {
			return Some(Ordering::Equal)
		}
		match (left, right) {
			// Root is greater than anything.
			(OriginCaller::system(frame_system::RawOrigin::Root), _) => Some(Ordering::Greater),
			_ => None,
		}
	}
}
