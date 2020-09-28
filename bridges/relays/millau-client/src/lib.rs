// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Types used to connect to the Millau-Substrate chain.

use relay_substrate_client::Chain;

/// Millau chain definition.
#[derive(Debug, Clone, Copy)]
pub struct Millau;

impl Chain for Millau {
	type BlockNumber = millau_runtime::BlockNumber;
	type Hash = millau_runtime::Hash;
	type Header = millau_runtime::Header;
	type AccountId = millau_runtime::AccountId;
	type Index = millau_runtime::Index;
	type SignedBlock = millau_runtime::SignedBlock;
	type Call = rialto_runtime::Call;
}
