// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! What backs the parachain: a relay chain, or the Parachain Service on JAM (`--cfg jam`).
//!
//! Both modules define the same names, so the rest of the runtime is written once against them:
//! the consensus hook, the `OnSetCode`, the Aura wiring of the timestamp and of the session
//! pallet, and the Aura authorities the runtime APIs and the authority-discovery migration read.
//! Each also configures the pallets only its backing composes.

#[cfg(not(jam))]
mod relay;
#[cfg(not(jam))]
pub use relay::*;

#[cfg(jam)]
mod jam;
#[cfg(jam)]
pub use jam::*;
