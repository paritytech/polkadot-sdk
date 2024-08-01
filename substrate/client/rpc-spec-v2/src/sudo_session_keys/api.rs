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

//! API trait of the `sudo_sessionKeys` method.

use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::MethodResult;

#[rpc(client, server)]
pub trait SudoSessionKeys {
	/// This RPC method calls into `SessionKeys_generate_session_keys` runtime function.
	///
	/// The `SessionKeys_generate_session_keys` runtime function generates a series of keys,
	/// inserts those keys into the keystore, and returns all the public keys concatenated.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "sudo_sessionKeys_unstable_generate")]
	fn sudo_session_keys_unstable_generate(&self, seed: Option<String>) -> RpcResult<MethodResult>;
}
