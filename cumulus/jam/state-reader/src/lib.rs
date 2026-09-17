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

//! JAM chain-state read side of the additional-data channel, extracted from
//! `cumulus-primitives-additional-data`.
//!
//! The generic additional-data machinery (the [`AdditionalData`] map, the finalizer registry and
//! the `finalize` host function) lives in `sp-additional-data`. This crate holds the parts specific
//! to *reading JAM chain state* into that channel:
//!
//! - [`JAM_PROOF_KEY`] — the map key under which the JAM read-proof is carried,
//! - [`JamStateReader`] + [`JamStateExt`] — the externalities extension the read host function
//!   dispatches through,
//! - [`jam_state::jam_state_read`] — the host function a parachain runtime calls to read JAM
//!   storage dynamically during block execution.
//!
//! A read [`JAM_PROOF_KEY`] entry pairs with an `sp-additional-data` finalizer registered under
//! the same key, so the JAM read-proof is both served (here) and committed to (in the generic
//! digest).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod jam;
// The reader's `jam-state-helpers` dependency cannot compile for `wasm32v1-none` (its transitive
// `parachain-service-interface` hard-codes riscv-only `no_std`), and non-riscv WASM runtimes read
// relay state, never JAM state, so the reader is only needed on riscv and the host node. Mirrors
// the Cargo.toml target gate.
#[cfg(not(target_family = "wasm"))]
pub mod jam_proof;
// The commit side pairs with [`JamProofReader`]: authoring commits the carried proof's hash under
// `JAM_PROOF_KEY` so the digest recomputed on import matches. `std`-only like every additional-data
// finalizer consumer (the omni-node); riscv runtimes do not need the type (their `additional-data`
// digest uses the reader's `proof_size` instead).
#[cfg(feature = "std")]
pub mod jam_proof_finalizer;
#[cfg(feature = "std")]
pub use jam::JamStateExt;
pub use jam::{jam_state, JamStateReader, JAM_PROOF_KEY};
#[cfg(not(target_family = "wasm"))]
pub use jam_proof::JamProofReader;
#[cfg(feature = "std")]
pub use jam_proof_finalizer::JamProofFinalizer;
