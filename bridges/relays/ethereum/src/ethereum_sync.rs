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

// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Parity-Bridge.

// Parity-Bridge is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity-Bridge is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity-Bridge.  If not, see <http://www.gnu.org/licenses/>.

use codec::Encode;
use crate::ethereum_headers::QueuedHeaders;
use crate::ethereum_types::{HeaderId, HeaderStatus, QueuedHeader};
use crate::ethereum_sync_loop::EthereumSyncParams;
use crate::substrate_types::{into_substrate_ethereum_header, into_substrate_ethereum_receipts};

/// Ethereum headers synchronization context.
#[derive(Debug)]
pub struct HeadersSync {
	/// Synchronization parameters.
	params: EthereumSyncParams,
	/// Best header number known to Ethereum node.
	target_header_number: Option<u64>,
	/// Best header known to Substrate node.
	best_header: Option<HeaderId>,
	/// Headers queue.
	headers: QueuedHeaders,
}

impl HeadersSync {
	/// Creates new Ethereum headers synchronizer.
	pub fn new(params: EthereumSyncParams) -> Self {
		HeadersSync {
			params,
			target_header_number: None,
			best_header: None,
			headers: Default::default(),
		}
	}

	/// Returns true if we have synced almost all known headers.
	pub fn is_almost_synced(&self) -> bool {
		match self.target_header_number {
			Some(target_header_number) => self.best_header
				.map(|best| target_header_number.saturating_sub(best.0) < 4)
				.unwrap_or(false),
			None => true,
		}
	}

	/// Returns synchronization status.
	pub fn status(&self) -> (&Option<HeaderId>, &Option<u64>) {
		(&self.best_header, &self.target_header_number)
	}

	/// Returns reference to the headers queue.
	pub fn headers(&self) -> &QueuedHeaders {
		&self.headers
	}

	/// Returns mutable reference to the headers queue.
	pub fn headers_mut(&mut self) -> &mut QueuedHeaders {
		&mut self.headers
	}

	/// Select header that needs to be downloaded from the Ethereum node.
	pub fn select_new_header_to_download(&self) -> Option<u64> {
		// if we haven't received best header from Ethereum node yet, there's nothing we can download
		let target_header_number = self.target_header_number.clone()?;

		// if we haven't received known best header from Substrate node yet, there's nothing we can download
		let best_header = self.best_header.as_ref()?;

		// if there's too many headers in the queue, stop downloading
		let in_memory_headers = self.headers.total_headers();
		if in_memory_headers >= self.params.max_future_headers_to_download {
			return None;
		}

		// we assume that there were no reorgs if we have already downloaded best header
		let best_downloaded_number = std::cmp::max(
			self.headers.best_queued_number(),
			best_header.0,
		);
		if best_downloaded_number == target_header_number {
			return None;
		}

		// download new header
		Some(best_downloaded_number + 1)
	}

	/// Select headers that need to be submitted to the Substrate node.
	pub fn select_headers_to_submit(&self) -> Option<Vec<&QueuedHeader>> {
		let headers_in_submit_status = self.headers.headers_in_status(HeaderStatus::Submitted);
		let headers_to_submit_count = self.params.max_headers_in_submitted_status
			.checked_sub(headers_in_submit_status)?;

		let mut total_size = 0;
		let mut total_headers = 0;
		self.headers.headers(HeaderStatus::Ready, |header| {
			if total_headers == headers_to_submit_count {
				return false;
			}
			if total_headers == self.params.max_headers_in_single_submit {
				return false;
			}

			let encoded_size = into_substrate_ethereum_header(header.header()).encode().len()
				+ into_substrate_ethereum_receipts(header.receipts()).map(|receipts| receipts.encode().len())
					.unwrap_or(0);
			if total_headers != 0 && total_size + encoded_size > self.params.max_headers_size_in_single_submit {
				return false;
			}

			total_size += encoded_size;
			total_headers += 1;

			true
		})
	}

	/// Receive new target header number from the Ethereum node.
	pub fn ethereum_best_header_number_response(&mut self, best_header_number: u64) {
		log::debug!(target: "bridge", "Received best header number from Ethereum: {}", best_header_number);
		self.target_header_number = Some(best_header_number);
	}

	/// Receive new best header from the Substrate node.
	/// Returns true if it is different from the previous block known to us.
	pub fn substrate_best_header_response(&mut self, best_header: HeaderId) -> bool {
		log::debug!(target: "bridge", "Received best known header from Substrate: {:?}", best_header);

		// early return if it is still the same
		if self.best_header == Some(best_header) {
			return false;
		}

		// remember that this header is now known to the Substrate runtime
		self.headers.substrate_best_header_response(&best_header);

		// prune ancient headers
		self.headers.prune(best_header.0.saturating_sub(self.params.prune_depth));

		// finally remember the best header itself
		self.best_header = Some(best_header);

		true
	}

	/// Restart synchronization.
	pub fn restart(&mut self) {
		self.target_header_number = None;
		self.best_header = None;
		self.headers.clear();
	}
}

#[cfg(test)]
mod tests {
	use crate::ethereum_headers::tests::{header, id};
	use crate::ethereum_types::{H256, HeaderStatus};
	use super::*;

	fn side_hash(number: u64) -> H256 {
		H256::from_low_u64_le(1000 + number)
	}

	#[test]
	fn select_new_header_to_download_works() {
		let mut eth_sync = HeadersSync::new(Default::default());

		// both best && target headers are unknown
		assert_eq!(eth_sync.select_new_header_to_download(), None);

		// best header is known, target header is unknown
		eth_sync.best_header = Some(HeaderId(0, Default::default()));
		assert_eq!(eth_sync.select_new_header_to_download(), None);

		// target header is known, best header is unknown
		eth_sync.best_header = None;
		eth_sync.target_header_number = Some(100);
		assert_eq!(eth_sync.select_new_header_to_download(), None);

		// when our best block has the same number as the target
		eth_sync.best_header = Some(HeaderId(100, Default::default()));
		assert_eq!(eth_sync.select_new_header_to_download(), None);

		// when we actually need a new header
		eth_sync.target_header_number = Some(101);
		assert_eq!(eth_sync.select_new_header_to_download(), Some(101));

		// when there are too many headers scheduled for submitting
		for i in 1..1000 {
			eth_sync.headers.header_response(header(i).header().clone());
		}
		assert_eq!(eth_sync.select_new_header_to_download(), None);
	}

	#[test]
	fn sync_without_reorgs_works() {
		let mut eth_sync = HeadersSync::new(Default::default());
		eth_sync.params.max_headers_in_submitted_status = 1;

		// ethereum reports best header #102
		eth_sync.ethereum_best_header_number_response(102);

		// substrate reports that it is at block #100
		eth_sync.substrate_best_header_response(id(100));

		// block #101 is downloaded first
		assert_eq!(eth_sync.select_new_header_to_download(), Some(101));
		eth_sync.headers.header_response(header(101).header().clone());

		// now header #101 is ready to be submitted
		assert_eq!(eth_sync.headers.header(HeaderStatus::MaybeReceipts), Some(&header(101)));
		eth_sync.headers.maybe_receipts_response(&id(101), false);
		assert_eq!(eth_sync.headers.header(HeaderStatus::Ready), Some(&header(101)));
		assert_eq!(eth_sync.select_headers_to_submit(), Some(vec![&header(101)]));

		// and header #102 is ready to be downloaded
		assert_eq!(eth_sync.select_new_header_to_download(), Some(102));
		eth_sync.headers.header_response(header(102).header().clone());

		// receive submission confirmation
		eth_sync.headers.headers_submitted(vec![id(101)]);

		// we have nothing to submit because previous header hasn't been confirmed yet
		// (and we allow max 1 submit transaction in the wild)
		assert_eq!(eth_sync.headers.header(HeaderStatus::MaybeReceipts), Some(&header(102)));
		eth_sync.headers.maybe_receipts_response(&id(102), false);
		assert_eq!(eth_sync.headers.header(HeaderStatus::Ready), Some(&header(102)));
		assert_eq!(eth_sync.select_headers_to_submit(), None);

		// substrate reports that it has imported block #101
		eth_sync.substrate_best_header_response(id(101));

		// and we are ready to submit #102
		assert_eq!(eth_sync.select_headers_to_submit(), Some(vec![&header(102)]));
		eth_sync.headers.headers_submitted(vec![id(102)]);

		// substrate reports that it has imported block #102
		eth_sync.substrate_best_header_response(id(102));

		// and we have nothing to download
		assert_eq!(eth_sync.select_new_header_to_download(), None);
	}

	#[test]
	fn sync_with_orphan_headers_work() {
		let mut eth_sync = HeadersSync::new(Default::default());

		// ethereum reports best header #102
		eth_sync.ethereum_best_header_number_response(102);

		// substrate reports that it is at block #100, but it isn't part of best chain
		eth_sync.substrate_best_header_response(HeaderId(100, side_hash(100)));

		// block #101 is downloaded first
		assert_eq!(eth_sync.select_new_header_to_download(), Some(101));
		eth_sync.headers.header_response(header(101).header().clone());

		// we can't submit header #101, because its parent status is unknown
		assert_eq!(eth_sync.select_headers_to_submit(), None);

		// instead we are trying to determine status of its parent (#100)
		assert_eq!(eth_sync.headers.header(HeaderStatus::MaybeOrphan), Some(&header(101)));

		// and the status is still unknown
		eth_sync.headers.maybe_orphan_response(&id(100), false);

		// so we consider #101 orphaned now && will download its parent - #100
		assert_eq!(eth_sync.headers.header(HeaderStatus::Orphan), Some(&header(101)));
		eth_sync.headers.header_response(header(100).header().clone());

		// we can't submit header #100, because its parent status is unknown
		assert_eq!(eth_sync.select_headers_to_submit(), None);

		// instead we are trying to determine status of its parent (#99)
		assert_eq!(eth_sync.headers.header(HeaderStatus::MaybeOrphan), Some(&header(100)));

		// and the status is known, so we move previously orphaned #100 and #101 to ready queue
		eth_sync.headers.maybe_orphan_response(&id(99), true);

		// and we are ready to submit #100
		assert_eq!(eth_sync.headers.header(HeaderStatus::MaybeReceipts), Some(&header(100)));
		eth_sync.headers.maybe_receipts_response(&id(100), false);
		assert_eq!(eth_sync.select_headers_to_submit(), Some(vec![&header(100)]));
		eth_sync.headers.headers_submitted(vec![id(100)]);

		// and we are ready to submit #101
		assert_eq!(eth_sync.headers.header(HeaderStatus::MaybeReceipts), Some(&header(101)));
		eth_sync.headers.maybe_receipts_response(&id(101), false);
		assert_eq!(eth_sync.select_headers_to_submit(), Some(vec![&header(101)]));
		eth_sync.headers.headers_submitted(vec![id(101)]);
	}

	#[test]
	fn pruning_happens_on_substrate_best_header_response() {
		let mut eth_sync = HeadersSync::new(Default::default());
		eth_sync.params.prune_depth = 50;
		eth_sync.substrate_best_header_response(id(100));
		assert_eq!(eth_sync.headers.prune_border(), 50);
	}
}
