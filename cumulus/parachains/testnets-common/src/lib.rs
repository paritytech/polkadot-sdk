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

#![cfg_attr(not(feature = "std"), no_std)]

/// Since the parachains-common package is now published to crates.io, SP runtimes for testnets
/// will be adapted to use this package, and their config removed from the published common
/// package. Only the configs specific to rococo, westend and wococo will be moved here, and the
/// truly common logic will still be sourced from the parachains-common package.
///
/// In practice this just means that instead of using e.g. `[parachains_common::westend::*]`, now
/// the westend configs will be in `[testnets_common::westend::*]`.
///
/// TODO: edit all runtimes to remove the testnet configs as part of PR #1737
/// <https://github.com/paritytech/polkadot-sdk/pull/1737>
pub mod rococo;
pub mod westend;
pub mod wococo;
