// Copyright (C) Parity Technologies (UK) Ltd.
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

use core::marker::PhantomData;

use crate::OriginCaller;
use frame_support::traits::{PrivilegeCmp, SortedMembers};
use sp_core::Get;
use sp_std::{cmp::Ordering, prelude::*};
use xcm::latest::prelude::*;
use xcm_executor::traits::ConvertLocation;

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

/// Implementation of [SortedMembers] for a single member, where member's `AccountId` is converted
/// by `C` from the [`Location`] specified by `L`.
pub struct MemberByLocation<L, C, AccountId>(PhantomData<(L, C, AccountId)>);
impl<L, C, AccountId> SortedMembers<AccountId> for MemberByLocation<L, C, AccountId>
where
	L: Get<Location>,
	C: ConvertLocation<AccountId>,
	AccountId: Ord,
{
	fn sorted_members() -> Vec<AccountId> {
		C::convert_location(&L::get()).map_or(vec![], |a| vec![a])
	}
}
