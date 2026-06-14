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

use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

#[derive(
	Clone,
	codec::Encode,
	codec::MaxEncodedLen,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct CommitmentSet<const N: u32>(BoundedVec<(ParaId, Hash), ConstU32<N>>);

impl<const N: u32> codec::Decode for CommitmentSet<N> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner = BoundedVec::<(ParaId, Hash), ConstU32<N>>::decode(input)?;

		for pair in inner.windows(2) {
			if pair[0].0 >= pair[1].0 {
				return Err(codec::Error::from(
					"CommitmentSet entries must be sorted by increasing ParaId",
				));
			}
		}

		Ok(Self(inner))
	}
}

// impl<const N: u32> CommitmentSet<N> {
//     pub fn try_from_iter(it: impl IntoIterator<Item= (ParaId, Hash)>) -> Result<Self,
// SpecMsgError> {         Ok(())
//     }
// }
