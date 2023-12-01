// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! API trait of the chain spec.

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_chain_spec::Properties;

#[rpc(client, server)]
pub trait ChainSpecApi {
	/// Get the chain name, as present in the chain specification.
	#[method(name = "chainSpec_v1_chainName")]
	fn chain_spec_v1_chain_name(&self) -> RpcResult<String>;

	/// Get the chain's genesis hash.
	#[method(name = "chainSpec_v1_genesisHash")]
	fn chain_spec_v1_genesis_hash(&self) -> RpcResult<String>;

	/// Get the properties of the chain, as present in the chain specification.
	///
	/// # Note
	///
	/// The json whitespaces are not guaranteed to persist.
	#[method(name = "chainSpec_v1_properties")]
	fn chain_spec_v1_properties(&self) -> RpcResult<Properties>;
}
