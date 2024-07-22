// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Tests for the [`SingleAssetExchangeAdapter`] type.

use super::mock::*;

/// Scenario:
/// Account #3 wants to use the local liquidity pool between two custom assets,
/// 1 and 2.
#[test]
fn maximal_exchange() {}

#[test]
fn not_maximal_exchange() {}

#[test]
fn maximal_quote() {}

#[test]
fn not_maximal_quote() {}

#[test]
fn no_asset_in_give() {}

#[test]
fn more_than_one_asset_in_give() {}

#[test]
fn no_asset_in_want() {}

#[test]
fn more_than_one_asset_in_want() {}

#[test]
fn give_asset_does_not_match() {}

#[test]
fn want_asset_does_not_match() {}

#[test]
fn exchange_fails() {}
