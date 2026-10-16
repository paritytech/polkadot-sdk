// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Transaction indexing host calls (indices 347-348).

#[cfg(not(substrate_runtime))]
use super::with_externalities;
#[cfg(not(substrate_runtime))]
use sp_jam_runtime_interface::FunctionContext;

sp_jam_runtime_interface::host_calls! {
	/// Indexes the specified transaction for the given `extrinsic` and the 32-byte
	/// `context_hash`.
	#[index = 347]
	fn ext_transaction_index_index(ctx: &mut dyn FunctionContext, extrinsic: u32, size: u32, context_hash: *const u8) {
		let context_hash = ctx.read_array::<32>(context_hash)?;
		with_externalities(|ext| ext.storage_index_transaction(extrinsic, &context_hash, size));
		Ok(())
	}

	/// Renews the transaction index entry for the given `extrinsic` using the provided
	/// 32-byte `context_hash`.
	#[index = 348]
	fn ext_transaction_index_renew(ctx: &mut dyn FunctionContext, extrinsic: u32, context_hash: *const u8) {
		let context_hash = ctx.read_array::<32>(context_hash)?;
		with_externalities(|ext| ext.storage_renew_transaction_index(extrinsic, &context_hash));
		Ok(())
	}
}

/// Indexes the specified transaction for the given `extrinsic` and `context_hash`.
pub fn index(extrinsic: u32, size: u32, context_hash: [u8; 32]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer comes from a live 32-byte array.
	unsafe {
		ext_transaction_index_index(extrinsic, size, context_hash.as_ptr())
	};
	#[cfg(not(substrate_runtime))]
	with_externalities(|ext| ext.storage_index_transaction(extrinsic, &context_hash, size))
}

/// Renews the transaction index entry for the given `extrinsic` using the provided
/// `context_hash`.
pub fn renew(extrinsic: u32, context_hash: [u8; 32]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer comes from a live 32-byte array.
	unsafe {
		ext_transaction_index_renew(extrinsic, context_hash.as_ptr())
	};
	#[cfg(not(substrate_runtime))]
	with_externalities(|ext| ext.storage_renew_transaction_index(extrinsic, &context_hash))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::jam::TestExternalities;

	#[test]
	fn index_and_renew_do_not_panic() {
		let mut ext = TestExternalities::default();
		ext.execute_with(|| {
			index(1, 100, [7u8; 32]);
			renew(1, [7u8; 32]);
		});
	}
}
