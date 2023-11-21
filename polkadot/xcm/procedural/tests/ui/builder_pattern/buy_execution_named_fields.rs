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

//! Test error when the `BuyExecution` instruction doesn't take named fields.

use xcm_procedural::Builder;

struct Xcm<Call>(pub Vec<Instruction<Call>>);

#[derive(Builder)]
enum Instruction<Call> {
    BuyExecution(u128),
    UnpaidExecution { weight_limit: (u32, u32) },
    Transact { call: Call },
}

fn main() {}
