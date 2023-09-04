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

use frame_support::{*, pallet_prelude::inject_runtime_type};
use static_assertions::assert_type_eq_all;

pub trait Config {
    type RuntimeCall;
}

type RuntimeCall = u32;

struct Pallet;

#[register_default_impl(Pallet)]
impl Config for Pallet {
    #[inject_runtime_type]
    type RuntimeCall = ();
}

struct SomePallet;

#[derive_impl(Pallet)] // Injects type RuntimeCall = RuntimeCall;
impl Config for SomePallet {}

assert_type_eq_all!(<SomePallet as Config>::RuntimeCall, u32);

fn main() {}
