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

//! Stock, pure Aura collators.
//!
//! This includes the [`basic`] collator, which only builds on top of the most recently
//! included parachain block, as well as the [`lookahead`] collator, which prospectively
//! builds on parachain blocks which have not yet been included in the relay chain.

pub mod basic;
pub mod lookahead;
