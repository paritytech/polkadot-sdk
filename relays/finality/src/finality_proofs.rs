// Copyright 2019-2023 Parity Technologies (UK) Ltd.
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

use crate::{base::SourceClientBase, FinalityPipeline};

use bp_header_chain::FinalityProof;
use futures::{FutureExt, Stream, StreamExt};
use std::pin::Pin;

/// Finality proofs container. Ordered by target header number.
pub type FinalityProofs<P> =
	Vec<(<P as FinalityPipeline>::Number, <P as FinalityPipeline>::FinalityProof)>;

/// Source finality proofs stream that may be restarted.
pub struct FinalityProofsStream<P: FinalityPipeline, SC: SourceClientBase<P>> {
	/// The underlying stream.
	stream: Option<Pin<Box<SC::FinalityProofsStream>>>,
}

impl<P: FinalityPipeline, SC: SourceClientBase<P>> FinalityProofsStream<P, SC> {
	pub fn new() -> Self {
		Self { stream: None }
	}

	fn next(&mut self) -> Option<<SC::FinalityProofsStream as Stream>::Item> {
		let stream = match &mut self.stream {
			Some(stream) => stream,
			None => return None,
		};

		match stream.next().now_or_never() {
			Some(Some(finality_proof)) => Some(finality_proof),
			Some(None) => {
				self.stream = None;
				None
			},
			None => None,
		}
	}

	pub async fn ensure_stream(&mut self, source_client: &SC) -> Result<(), SC::Error> {
		if self.stream.is_none() {
			log::warn!(target: "bridge", "{} finality proofs stream is being started / restarted",
				P::SOURCE_NAME);

			let stream = source_client.finality_proofs().await.map_err(|error| {
				log::error!(
					target: "bridge",
					"Failed to subscribe to {} justifications: {:?}",
					P::SOURCE_NAME,
					error,
				);

				error
			})?;
			self.stream = Some(Box::pin(stream));
		}

		Ok(())
	}
}

/// Source finality proofs buffer.
pub struct FinalityProofsBuf<P: FinalityPipeline> {
	/// Proofs buffer.
	buf: FinalityProofs<P>,
}

impl<P: FinalityPipeline> FinalityProofsBuf<P> {
	pub fn new(buf: FinalityProofs<P>) -> Self {
		Self { buf }
	}

	pub fn buf(&self) -> &FinalityProofs<P> {
		&self.buf
	}

	pub fn fill<SC: SourceClientBase<P>>(&mut self, stream: &mut FinalityProofsStream<P, SC>) {
		let mut proofs_count = 0;
		let mut first_header_number = None;
		let mut last_header_number = None;
		while let Some(finality_proof) = stream.next() {
			let target_header_number = finality_proof.target_header_number();
			first_header_number.get_or_insert(target_header_number);
			last_header_number = Some(target_header_number);
			proofs_count += 1;

			self.buf.push((target_header_number, finality_proof));
		}

		if proofs_count != 0 {
			log::trace!(
				target: "bridge",
				"Read {} finality proofs from {} finality stream for headers in range [{:?}; {:?}]",
				proofs_count,
				P::SOURCE_NAME,
				first_header_number,
				last_header_number,
			);
		}
	}

	pub fn prune(&mut self, until_hdr_num: P::Number, buf_limit: usize) {
		let kept_hdr_idx = self
			.buf
			.binary_search_by_key(&until_hdr_num, |(hdr_num, _)| *hdr_num)
			.map(|idx| idx + 1)
			.unwrap_or_else(|idx| idx);
		let buf_limit_idx = self.buf.len().saturating_sub(buf_limit);

		self.buf = self.buf.split_off(std::cmp::max(kept_hdr_idx, buf_limit_idx));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	impl<P: FinalityPipeline, SC: SourceClientBase<P>> FinalityProofsStream<P, SC> {
		fn from_stream(stream: SC::FinalityProofsStream) -> Self {
			Self { stream: Some(Box::pin(stream)) }
		}
	}

	#[test]
	fn finality_proofs_buf_fill_works() {
		// when stream is currently empty, nothing is changed
		let mut finality_proofs_buf =
			FinalityProofsBuf::<TestFinalitySyncPipeline> { buf: vec![(1, TestFinalityProof(1))] };
		let mut stream =
			FinalityProofsStream::<TestFinalitySyncPipeline, TestSourceClient>::from_stream(
				Box::pin(futures::stream::pending()),
			);
		finality_proofs_buf.fill(&mut stream);
		assert_eq!(finality_proofs_buf.buf, vec![(1, TestFinalityProof(1))]);
		assert!(stream.stream.is_some());

		// when stream has entry with target, it is added to the recent proofs container
		let mut stream =
			FinalityProofsStream::<TestFinalitySyncPipeline, TestSourceClient>::from_stream(
				Box::pin(
					futures::stream::iter(vec![TestFinalityProof(4)])
						.chain(futures::stream::pending()),
				),
			);
		finality_proofs_buf.fill(&mut stream);
		assert_eq!(
			finality_proofs_buf.buf,
			vec![(1, TestFinalityProof(1)), (4, TestFinalityProof(4))]
		);
		assert!(stream.stream.is_some());

		// when stream has ended, we'll need to restart it
		let mut stream =
			FinalityProofsStream::<TestFinalitySyncPipeline, TestSourceClient>::from_stream(
				Box::pin(futures::stream::empty()),
			);
		finality_proofs_buf.fill(&mut stream);
		assert_eq!(
			finality_proofs_buf.buf,
			vec![(1, TestFinalityProof(1)), (4, TestFinalityProof(4))]
		);
		assert!(stream.stream.is_none());
	}

	#[test]
	fn finality_proofs_buf_prune_works() {
		let original_finality_proofs_buf: FinalityProofs<TestFinalitySyncPipeline> = vec![
			(10, TestFinalityProof(10)),
			(13, TestFinalityProof(13)),
			(15, TestFinalityProof(15)),
			(17, TestFinalityProof(17)),
			(19, TestFinalityProof(19)),
		]
		.into_iter()
		.collect();

		// when there's proof for justified header in the vec
		let mut finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline> {
			buf: original_finality_proofs_buf.clone(),
		};
		finality_proofs_buf.prune(10, 1024);
		assert_eq!(&original_finality_proofs_buf[1..], finality_proofs_buf.buf,);

		// when there are no proof for justified header in the vec
		let mut finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline> {
			buf: original_finality_proofs_buf.clone(),
		};
		finality_proofs_buf.prune(11, 1024);
		assert_eq!(&original_finality_proofs_buf[1..], finality_proofs_buf.buf,);

		// when there are too many entries after initial prune && they also need to be pruned
		let mut finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline> {
			buf: original_finality_proofs_buf.clone(),
		};
		finality_proofs_buf.prune(10, 2);
		assert_eq!(&original_finality_proofs_buf[3..], finality_proofs_buf.buf,);

		// when last entry is pruned
		let mut finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline> {
			buf: original_finality_proofs_buf.clone(),
		};
		finality_proofs_buf.prune(19, 2);
		assert_eq!(&original_finality_proofs_buf[5..], finality_proofs_buf.buf,);

		// when post-last entry is pruned
		let mut finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline> {
			buf: original_finality_proofs_buf.clone(),
		};
		finality_proofs_buf.prune(20, 2);
		assert_eq!(&original_finality_proofs_buf[5..], finality_proofs_buf.buf,);
	}
}
