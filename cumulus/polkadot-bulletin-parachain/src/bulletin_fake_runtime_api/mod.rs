// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Bulletin-local fake runtime API. Reuses the lib's `impl_node_runtime_apis!`
//! macro and layers in `sp_hop::HopRuntimeApi` as an extra trait impl, which
//! the bulletin's `HopExtension` requires as a trait bound on
//! `Self::RuntimeApi`. Defining it here keeps the lib HOP-free; the lib's own
//! fake does not (and need not) implement `HopRuntimeApi`.
//!
//! As with the lib's fake, the impls are unreachable at runtime; the actual
//! runtime is loaded from the chain spec wasm.

use polkadot_omni_node_lib::{fake_runtime_api::utils::imports::*, impl_node_runtime_apis};

#[allow(dead_code)]
type CustomBlock = polkadot_omni_node_lib::BlockU32;

#[allow(missing_docs)]
pub mod aura_sr25519 {
	use super::*;
	#[allow(dead_code)]
	struct FakeRuntime;
	impl_node_runtime_apis!(
		FakeRuntime,
		CustomBlock,
		sp_consensus_aura::sr25519::AuthorityId,
		{
			impl sp_hop::HopRuntimeApi<CustomBlock, AccountId> for FakeRuntime {
				fn can_account_promote(_who: AccountId, _data_len: u32) -> bool {
					false
				}

				fn create_promotion_extrinsic(
					_: Vec<u8>,
					_: sp_runtime::MultiSigner,
					_: sp_runtime::MultiSignature,
					_: u64,
				) -> <CustomBlock as sp_runtime::traits::Block>::Extrinsic {
					panic!("HOP promotion is not supported by this runtime")
				}

				fn max_promotion_size() -> u32 {
					0
				}

				fn is_promoted_on_chain(_hash: [u8; 32]) -> bool {
					false
				}
			}
		}
	);
}

#[allow(missing_docs)]
pub mod aura_ed25519 {
	use super::*;
	#[allow(dead_code)]
	struct FakeRuntime;
	impl_node_runtime_apis!(
		FakeRuntime,
		CustomBlock,
		sp_consensus_aura::ed25519::AuthorityId,
		{
			impl sp_hop::HopRuntimeApi<CustomBlock, AccountId> for FakeRuntime {
				fn can_account_promote(_who: AccountId, _data_len: u32) -> bool {
					false
				}

				fn create_promotion_extrinsic(
					_: Vec<u8>,
					_: sp_runtime::MultiSigner,
					_: sp_runtime::MultiSignature,
					_: u64,
				) -> <CustomBlock as sp_runtime::traits::Block>::Extrinsic {
					panic!("HOP promotion is not supported by this runtime")
				}

				fn max_promotion_size() -> u32 {
					0
				}

				fn is_promoted_on_chain(_hash: [u8; 32]) -> bool {
					false
				}
			}
		}
	);
}
