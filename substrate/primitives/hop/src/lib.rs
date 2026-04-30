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

//! HOP (Hand-Off Protocol) primitives.
//!
//! Contains the runtime API trait for HOP — authorization checks and promotion
//! of ephemeral pool data to on-chain storage.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

sp_api::decl_runtime_apis! {
	/// Runtime API for HOP.
	///
	/// Runtimes that support HOP implement this API so the node can check
	/// authorization and promote near-expiry pool entries to on-chain storage.
	///
	/// Runtimes that **don't** want HOP must still implement this trait to satisfy
	/// the `NodeRuntimeApi` supertrait bound on `polkadot-omni-node`. The canonical
	/// no-op stub is:
	///
	/// ```ignore
	/// sp_api::impl_runtime_apis! {
	///     // ... your existing API impls ...
	///     impl sp_hop::HopRuntimeApi<Block, AccountId> for Runtime {
	///         fn can_account_promote(_: AccountId, _: u32) -> bool { false }
	///         fn create_promotion_extrinsic(
	///             _: alloc::vec::Vec<u8>,
	///             _: sp_runtime::MultiSigner,
	///             _: sp_runtime::MultiSignature,
	///             _: u64,
	///         ) -> <Block as sp_runtime::traits::Block>::Extrinsic {
	///             panic!("HOP not supported by this runtime")
	///         }
	///         fn max_promotion_size() -> u32 { 0 }
	///     }
	/// }
	/// ```
	///
	/// `create_promotion_extrinsic` is unreachable for stub runtimes because the
	/// node calls it only after `can_account_promote` returned `true`, so the panic
	/// arm cannot fire in practice. `sc-hop`'s maintenance task additionally probes
	/// `ApiExt::has_api()` at startup; runtimes that omit the version annotation
	/// are auto-detected and the task degrades to cleanup-only.
	#[api_version(1)]
	pub trait HopRuntimeApi<AccountId> where AccountId: codec::Codec {
		/// Whether `who` may submit a HOP blob of `data_len` bytes for promotion.
		///
		/// Returns `false` for any "not allowed" reason — unknown account, exhausted
		/// quota, oversized payload, etc. The runtime is free to fold those reasons
		/// together; the node only needs the boolean. Transport-level failures (the
		/// runtime panicking, the state being unavailable) surface as `ApiError` from
		/// the `sp_api` machinery, not via this return value.
		fn can_account_promote(who: AccountId, data_len: u32) -> bool;
		/// Construct an unsigned promotion extrinsic carrying the user's submit-time
		/// signer, signature, and timestamp so the runtime pallet can verify consent
		/// on-chain. `submit_timestamp` is bound into the signed payload and bounds
		/// the signature's validity window, preventing replay long after the fact.
		fn create_promotion_extrinsic(
			data: alloc::vec::Vec<u8>,
			signer: sp_runtime::MultiSigner,
			signature: sp_runtime::MultiSignature,
			submit_timestamp: u64,
		) -> Block::Extrinsic;
		/// Maximum data size per promotion extrinsic.
		fn max_promotion_size() -> u32;
	}
}
