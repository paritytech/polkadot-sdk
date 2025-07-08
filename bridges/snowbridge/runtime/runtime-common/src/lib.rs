// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! # Runtime Common
//!
//! Common traits and types shared by runtimes.
#![cfg_attr(not(feature = "std"), no_std)]
pub mod v2;
pub use v2::register_token::{ForeignAssetOwner, LocalAssetOwner};
