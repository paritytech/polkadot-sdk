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

use crate::substrate_types::{into_substrate_ethereum_header, into_substrate_ethereum_receipts};
use crate::sync_types::{HeaderId, HeadersSyncPipeline, QueuedHeader, SourceHeader};
use codec::Encode;

pub use web3::types::{Address, Bytes, CallRequest, H256, U128, U256, U64};

/// When header is just received from the Ethereum node, we check that it has
/// both number and hash fields filled.
pub const HEADER_ID_PROOF: &'static str = "checked on retrieval; qed";

/// When receipt is just received from the Ethereum node, we check that it has
/// gas_used field filled.
pub const RECEIPT_GAS_USED_PROOF: &'static str = "checked on retrieval; qed";

/// Ethereum transaction hash type.
pub type TransactionHash = H256;

/// Ethereum transaction type.
pub type Transaction = web3::types::Transaction;

/// Ethereum header type.
pub type Header = web3::types::Block<H256>;

/// Ethereum header with transactions type.
pub type HeaderWithTransactions = web3::types::Block<Transaction>;

/// Ethereum transaction receipt type.
pub type Receipt = web3::types::TransactionReceipt;

/// Ethereum header ID.
pub type EthereumHeaderId = HeaderId<H256, u64>;

/// Queued ethereum header ID.
pub type QueuedEthereumHeader = QueuedHeader<EthereumHeadersSyncPipeline>;

/// A raw Ethereum transaction that's been signed.
pub type SignedRawTx = Vec<u8>;

/// Ethereum synchronization pipeline.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct EthereumHeadersSyncPipeline;

impl HeadersSyncPipeline for EthereumHeadersSyncPipeline {
	const SOURCE_NAME: &'static str = "Ethereum";
	const TARGET_NAME: &'static str = "Substrate";

	type Hash = H256;
	type Number = u64;
	type Header = Header;
	type Extra = Vec<Receipt>;
	type Completion = ();

	fn estimate_size(source: &QueuedHeader<Self>) -> usize {
		into_substrate_ethereum_header(source.header()).encode().len()
			+ into_substrate_ethereum_receipts(source.extra())
				.map(|extra| extra.encode().len())
				.unwrap_or(0)
	}
}

impl SourceHeader<H256, u64> for Header {
	fn id(&self) -> EthereumHeaderId {
		HeaderId(
			self.number.expect(HEADER_ID_PROOF).as_u64(),
			self.hash.expect(HEADER_ID_PROOF),
		)
	}

	fn parent_id(&self) -> EthereumHeaderId {
		HeaderId(self.number.expect(HEADER_ID_PROOF).as_u64() - 1, self.parent_hash)
	}
}
