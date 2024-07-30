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
// limitations under the License.

use frame_support::*;
use static_assertions::assert_type_eq_all;

pub trait Animal {
	type Locomotion;
	type Diet;
	type SleepingStrategy;
	type Environment;

	fn animal_name() -> &'static str;
}

pub type RunsOnFourLegs = (usize, usize, usize, usize);
pub type RunsOnTwoLegs = (usize, usize);
pub type Swims = isize;
pub type Diurnal = bool;
pub type Nocturnal = Option<bool>;
pub type Omnivore = char;
pub type Land = ((), ());
pub type Sea = ((), (), ());
pub type Carnivore = (char, char);

pub struct FourLeggedAnimal {}

#[register_default_impl(FourLeggedAnimal)]
impl Animal for FourLeggedAnimal {
	type Locomotion = RunsOnFourLegs;
	type Diet = Omnivore;
	type SleepingStrategy = Diurnal;
	type Environment = Land;

	fn animal_name() -> &'static str {
		"A Four-Legged Animal"
	}
}

pub struct AquaticMammal {}

// without omitting the `as X`
#[derive_impl(FourLeggedAnimal as Animal)]
impl Animal for AquaticMammal {
	type Locomotion = (Swims, RunsOnFourLegs);
	type Environment = (Land, Sea);
}

assert_type_eq_all!(<AquaticMammal as Animal>::Locomotion, (Swims, RunsOnFourLegs));
assert_type_eq_all!(<AquaticMammal as Animal>::Environment, (Land, Sea));
assert_type_eq_all!(<AquaticMammal as Animal>::Diet, Omnivore);
assert_type_eq_all!(<AquaticMammal as Animal>::SleepingStrategy, Diurnal);

pub struct Lion {}

// test omitting the `as X`
#[derive_impl(FourLeggedAnimal)]
impl Animal for Lion {
	type Diet = Carnivore;
	type SleepingStrategy = Nocturnal;

	fn animal_name() -> &'static str {
		"Lion"
	}
}

assert_type_eq_all!(<Lion as Animal>::Diet, Carnivore);
assert_type_eq_all!(<Lion as Animal>::SleepingStrategy, Nocturnal);
assert_type_eq_all!(<Lion as Animal>::Environment, Land);
assert_type_eq_all!(<Lion as Animal>::Locomotion, RunsOnFourLegs);

fn main() {}
