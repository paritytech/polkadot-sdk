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

#[cfg(feature = "bls12-377")]
pub mod bls12_377;
#[cfg(feature = "bls12-381")]
pub mod bls12_381;
#[cfg(feature = "bw6-761")]
pub mod bw6_761;
#[cfg(feature = "ed-on-bls12-377")]
pub mod ed_on_bls12_377;
#[cfg(feature = "ed-on-bls12-381-bandersnatch")]
pub mod ed_on_bls12_381_bandersnatch;

#[cfg(any(
	feature = "bls12-377",
	feature = "bls12-381",
	feature = "bw6-761",
	feature = "ed-on-bls12-377",
	feature = "ed-on-bls12-381-bandersnatch",
))]
mod utils;
