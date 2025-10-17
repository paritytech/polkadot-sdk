// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod mpt;
mod node_codec;
mod storage_proof;
#[cfg(test)]
mod test;

pub use alloy_consensus::{Receipt, ReceiptEnvelope, ReceiptWithBloom, RlpDecodableReceipt};
pub use alloy_primitives::Log;
pub use alloy_rlp::{Decodable, Encodable};
pub use storage_proof::{EIP1186Layout, MemoryDB, StorageProof};
