// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! JAM-side pieces of the parachain runtime.
//!
//! The counterpart of [`crate::relay_chain`]: everything a runtime needs on JAM
//! (parachain service) that does not exist on the relay chain.
//!
//! The module is compiled on the riscv runtime (`cfg(jam)`) and, so the host tests can exercise
//! the pure upgrade decision, on host test builds (`cfg(test)`). The submodules that need the JAM
//! host calls stay `cfg(jam)`.

// `slot` reads the block's own `JamParent` digest and no JAM host call, so it compiles on host
// test builds too.
#[cfg(any(test, jam))]
pub mod slot;
#[cfg(any(test, jam))]
pub mod upgrade;
