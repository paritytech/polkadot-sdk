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

//! # FRAME
//!
//! ```no_compile
//!   ______   ______    ________   ___ __ __   ______
//!  /_____/\ /_____/\  /_______/\ /__//_//_/\ /_____/\
//!  \::::_\/_\:::_ \ \ \::: _  \ \\::\| \| \ \\::::_\/_
//!   \:\/___/\\:(_) ) )_\::(_)  \ \\:.      \ \\:\/___/\
//!    \:::._\/ \: __ `\ \\:: __  \ \\:.\-/\  \ \\::___\/_
//!     \:\ \    \ \ `\ \ \\:.\ \  \ \\. \  \  \ \\:\____/\
//!      \_\/     \_\/ \_\/ \__\/\__\/ \__\/ \__\/ \_____\/
//! ```
//!
//! > **F**ramework for **R**untime **A**ggregation of **M**odularized **E**ntities: Substrate's
//! > State Transition Function (Runtime) Framework.
//!
//! ## Documentation
//!
//! See [`polkadot_sdk::frame`](../polkadot_sdk_docs/polkadot_sdk/frame_runtime/index.html).
//!
//! ## Warning: Experimental
//!
//! This crate and all of its content is experimental, and should not yet be used in production.
//!
//! ## Underlying dependencies
//!
//! This crate is an amalgamation of multiple other crates that are often used together to compose a
//! pallet. It is not necessary to use it, and it may fall short for certain purposes.
//!
//! In short, this crate only re-exports types and traits from multiple sources. All of these
//! sources are listed (and re-exported again) in [`deps`].
//!
//! ## Usage
//!
//! Please note that this crate can only be imported as `polkadot-sdk-frame` or `frame`.

#![cfg_attr(not(feature = "std"), no_std)]

// NOTE that we put the actual code into another module to keep the no-std capability of this create
// even when the `experimental` feature is disabled.
#[cfg(feature = "experimental")]
pub mod experimental;

#[cfg(feature = "experimental")]
pub use experimental::*;
