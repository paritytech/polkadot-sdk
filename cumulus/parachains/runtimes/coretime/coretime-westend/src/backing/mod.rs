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

//! What backs the Coretime chain: a relay chain, or the Parachain Service on JAM (`--cfg jam`).
//!
//! Both modules define the same names, so the rest of the runtime is written once against them:
//! the session keys, the consensus hook, the `OnSetCode`, the clock the runtime reads as the relay
//! block number, the Aura, session and authorship wiring, the XCM transport, the provider of the
//! cores the broker sells, the backing's migrations and the authority query of the runtime API.
//! Each also configures the pallets only its backing composes.

#[cfg(not(jam))]
mod relay;
#[cfg(not(jam))]
pub use relay::*;

#[cfg(jam)]
mod jam;
#[cfg(jam)]
pub use jam::*;
