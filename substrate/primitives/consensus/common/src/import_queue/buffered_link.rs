// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Provides the `buffered_link` utility.
//!
//! The buffered link is a channel that allows buffering the method calls on `Link`.
//!
//! # Example
//!
//! ```
//! use sp_consensus::import_queue::Link;
//! # use sp_consensus::import_queue::buffered_link::buffered_link;
//! # use sp_test_primitives::Block;
//! # struct DummyLink; impl Link<Block> for DummyLink {}
//! # let mut my_link = DummyLink;
//! let (mut tx, mut rx) = buffered_link::<Block>();
//! tx.blocks_processed(0, 0, vec![]);
//!
//! // Calls `my_link.blocks_processed(0, 0, vec![])` when polled.
//! let _fut = futures::future::poll_fn(move |cx| {
//! 	rx.poll_actions(cx, &mut my_link);
//! 	std::task::Poll::Pending::<()>
//! });
//! ```
//!

use futures::{prelude::*, channel::mpsc};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::{pin::Pin, task::Context, task::Poll};
use crate::import_queue::{Origin, Link, BlockImportResult, BlockImportError};

/// Wraps around an unbounded channel from the `futures` crate. The sender implements `Link` and
/// can be used to buffer commands, and the receiver can be used to poll said commands and transfer
/// them to another link.
pub fn buffered_link<B: BlockT>() -> (BufferedLinkSender<B>, BufferedLinkReceiver<B>) {
	let (tx, rx) = mpsc::unbounded();
	let tx = BufferedLinkSender { tx };
	let rx = BufferedLinkReceiver { rx };
	(tx, rx)
}

/// See [`buffered_link`].
pub struct BufferedLinkSender<B: BlockT> {
	tx: mpsc::UnboundedSender<BlockImportWorkerMsg<B>>,
}

impl<B: BlockT> BufferedLinkSender<B> {
	/// Returns true if the sender points to nowhere.
	///
	/// Once `true` is returned, it is pointless to use the sender anymore.
	pub fn is_closed(&self) -> bool {
		self.tx.is_closed()
	}
}

impl<B: BlockT> Clone for BufferedLinkSender<B> {
	fn clone(&self) -> Self {
		BufferedLinkSender {
			tx: self.tx.clone(),
		}
	}
}

/// Internal buffered message.
enum BlockImportWorkerMsg<B: BlockT> {
	BlocksProcessed(usize, usize, Vec<(Result<BlockImportResult<NumberFor<B>>, BlockImportError>, B::Hash)>),
	JustificationImported(Origin, B::Hash, NumberFor<B>, bool),
	RequestJustification(B::Hash, NumberFor<B>),
	FinalityProofImported(Origin, (B::Hash, NumberFor<B>), Result<(B::Hash, NumberFor<B>), ()>),
	RequestFinalityProof(B::Hash, NumberFor<B>),
}

impl<B: BlockT> Link<B> for BufferedLinkSender<B> {
	fn blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportResult<NumberFor<B>>, BlockImportError>, B::Hash)>
	) {
		let _ = self.tx.unbounded_send(BlockImportWorkerMsg::BlocksProcessed(imported, count, results));
	}

	fn justification_imported(
		&mut self,
		who: Origin,
		hash: &B::Hash,
		number: NumberFor<B>,
		success: bool
	) {
		let msg = BlockImportWorkerMsg::JustificationImported(who, hash.clone(), number, success);
		let _ = self.tx.unbounded_send(msg);
	}

	fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let _ = self.tx.unbounded_send(BlockImportWorkerMsg::RequestJustification(hash.clone(), number));
	}

	fn finality_proof_imported(
		&mut self,
		who: Origin,
		request_block: (B::Hash, NumberFor<B>),
		finalization_result: Result<(B::Hash, NumberFor<B>), ()>,
	) {
		let msg = BlockImportWorkerMsg::FinalityProofImported(who, request_block, finalization_result);
		let _ = self.tx.unbounded_send(msg);
	}

	fn request_finality_proof(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let _ = self.tx.unbounded_send(BlockImportWorkerMsg::RequestFinalityProof(hash.clone(), number));
	}
}

/// See [`buffered_link`].
pub struct BufferedLinkReceiver<B: BlockT> {
	rx: mpsc::UnboundedReceiver<BlockImportWorkerMsg<B>>,
}

impl<B: BlockT> BufferedLinkReceiver<B> {
	/// Polls for the buffered link actions. Any enqueued action will be propagated to the link
	/// passed as parameter.
	///
	/// This method should behave in a way similar to `Future::poll`. It can register the current
	/// task and notify later when more actions are ready to be polled. To continue the comparison,
	/// it is as if this method always returned `Poll::Pending`.
	pub fn poll_actions(&mut self, cx: &mut Context, link: &mut dyn Link<B>) {
		loop {
			let msg = if let Poll::Ready(Some(msg)) = Stream::poll_next(Pin::new(&mut self.rx), cx) {
				msg
			} else {
				break
			};

			match msg {
				BlockImportWorkerMsg::BlocksProcessed(imported, count, results) =>
					link.blocks_processed(imported, count, results),
				BlockImportWorkerMsg::JustificationImported(who, hash, number, success) =>
					link.justification_imported(who, &hash, number, success),
				BlockImportWorkerMsg::RequestJustification(hash, number) =>
					link.request_justification(&hash, number),
				BlockImportWorkerMsg::FinalityProofImported(who, block, result) =>
					link.finality_proof_imported(who, block, result),
				BlockImportWorkerMsg::RequestFinalityProof(hash, number) =>
					link.request_finality_proof(&hash, number),
			}
		}
	}

	/// Close the channel.
	pub fn close(&mut self) {
		self.rx.close()
	}
}

#[cfg(test)]
mod tests {
	use sp_test_primitives::Block;

	#[test]
	fn is_closed() {
		let (tx, rx) = super::buffered_link::<Block>();
		assert!(!tx.is_closed());
		drop(rx);
		assert!(tx.is_closed());
	}
}
