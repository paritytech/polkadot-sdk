// This file is part of Substrate.

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

use crate::weights::{SubstrateWeight as StakingWeight, WeightInfo as _};

use core::marker::PhantomData;
use frame_support::weights::Weight;

pub trait WeightInfo {
	fn step() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn step() -> Weight {
		StakingWeight::<T>::v13_mmb_step()
	}
}

impl WeightInfo for () {
	fn step() -> Weight {
		Weight::default()
	}
}
