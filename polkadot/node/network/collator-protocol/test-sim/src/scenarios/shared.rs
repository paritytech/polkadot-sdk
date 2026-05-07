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

//! Helpers shared across scenarios.

use crate::{contract::Query, harness::AnswerQuery};

/// A responder that panics on every query. Pushed onto the tail of a
/// [`crate::harness::LayeredResponder`] to surface any unexpected query family that earlier
/// layers declined.
pub struct PanicResponder;

impl AnswerQuery for PanicResponder {
	fn answer(&mut self, query: Query) {
		panic!("PanicResponder: unhandled query reached the tail of the responder chain: {:?}", query);
	}
}
