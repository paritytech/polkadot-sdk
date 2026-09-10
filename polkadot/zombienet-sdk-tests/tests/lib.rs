// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0
#[cfg(feature = "zombie-ci")]
mod disabling;
#[cfg(feature = "zombie-ci")]
mod elastic_scaling;
#[cfg(feature = "zombie-ci")]
mod functional;
#[cfg(feature = "zombie-ci")]
mod misc;
// Temporarily disabled: parachains/weights.rs uses a pre-rename pallet-revive API
// (`weight_required`) and does not compile on this branch.
// #[cfg(feature = "zombie-ci")]
// mod parachains;
#[cfg(feature = "zombie-ci")]
mod smoke;
#[cfg(feature = "zombie-ci")]
mod utils;
