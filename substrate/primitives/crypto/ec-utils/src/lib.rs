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

//! Elliptic Curves host functions which may be used to handle some of the *Arkworks*
//! computationally expensive operations.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

mod utils;

use ark_scale::ark_serialize::{Compress, Validate};
use sp_runtime_interface::runtime_interface;
use sp_std::vec::Vec;
use utils::*;

/// TODO
// #[cfg(feature = "bls12-377")]
pub mod bls12_377;
/// TODO
// #[cfg(feature = "bls12-381")]
pub mod bls12_381;
/// TODO
// #[cfg(feature = "bw6_761")]
pub mod bw6_761;
/// TODO
// #[cfg(feature = "ed-on-bls12-377")]
pub mod ed_on_bls12_377;
/// TODO
// #[cfg(feature = "ed-on-bls12-381-bandersnatch")]
pub mod ed_on_bls12_381_bandersnatch;

pub(crate) const SCALE_USAGE: u8 = ark_scale::make_usage(Compress::No, Validate::No);
pub(crate) type ArkScale<T> = ark_scale::ArkScale<T, SCALE_USAGE>;
pub(crate) type ArkScaleProjective<T> = ark_scale::hazmat::ArkScaleProjective<T>;
