// Copyright Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! # XCM Cookbook
//!
//! A collection of XCM recipes.
//!
//! Each recipe is tested and explains all the code necessary to run it -- they're not just snippets
//! to copy and paste.

/// Configuring a parachain that only uses the Relay Chain native token.
/// In the case of Polkadot, this recipe will show you how to launch a parachain with no native
/// token -- dealing only on DOT.
pub mod relay_token_transactor;
