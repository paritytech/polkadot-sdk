// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use crate::{
	finality_loop::SyncInfo, finality_proofs::FinalityProofsBuf, Error, FinalitySyncPipeline,
	HeadersToRelay, SourceClient, SourceHeader, TargetClient,
};

use bp_header_chain::FinalityProof;
use num_traits::Saturating;
use std::cmp::Ordering;

/// Unjustified headers container. Ordered by header number.
pub type UnjustifiedHeaders<H> = Vec<H>;

#[derive(Debug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct JustifiedHeader<P: FinalitySyncPipeline> {
	pub header: P::Header,
	pub proof: P::FinalityProof,
}

impl<P: FinalitySyncPipeline> JustifiedHeader<P> {
	pub fn number(&self) -> P::Number {
		self.header.number()
	}
}

/// Finality proof that has been selected by the `read_missing_headers` function.
pub enum JustifiedHeaderSelector<P: FinalitySyncPipeline> {
	/// Mandatory header and its proof has been selected. We shall submit proof for this header.
	Mandatory(JustifiedHeader<P>),
	/// Regular header and its proof has been selected. We may submit this proof, or proof for
	/// some better header.
	Regular(UnjustifiedHeaders<P::Header>, JustifiedHeader<P>),
	/// We haven't found any missing header with persistent proof at the target client.
	None(UnjustifiedHeaders<P::Header>),
}

impl<P: FinalitySyncPipeline> JustifiedHeaderSelector<P> {
	/// Selects last header with persistent justification, missing from the target and matching
	/// the `headers_to_relay` criteria.
	pub(crate) async fn new<SC: SourceClient<P>, TC: TargetClient<P>>(
		source_client: &SC,
		info: &SyncInfo<P>,
		headers_to_relay: HeadersToRelay,
		free_headers_interval: Option<P::Number>,
	) -> Result<Self, Error<P, SC::Error, TC::Error>> {
		let mut unjustified_headers = Vec::new();
		let mut maybe_justified_header = None;

		let mut header_number = info.best_number_at_target + 1.into();
		while header_number <= info.best_number_at_source {
			let (header, maybe_proof) = source_client
				.header_and_finality_proof(header_number)
				.await
				.map_err(Error::Source)?;

			match (header.is_mandatory(), maybe_proof) {
				(true, Some(proof)) => {
					log::trace!(target: "bridge", "Header {:?} is mandatory", header_number);
					return Ok(Self::Mandatory(JustifiedHeader { header, proof }))
				},
				(true, None) => return Err(Error::MissingMandatoryFinalityProof(header.number())),
				(false, Some(proof))
					if need_to_relay::<P>(
						info,
						headers_to_relay,
						free_headers_interval,
						&header,
					) =>
				{
					log::trace!(target: "bridge", "Header {:?} has persistent finality proof", header_number);
					unjustified_headers.clear();
					maybe_justified_header = Some(JustifiedHeader { header, proof });
				},
				_ => {
					unjustified_headers.push(header);
				},
			}

			header_number = header_number + 1.into();
		}

		log::trace!(
			target: "bridge",
			"Read {} {} headers. Selected finality proof for header: {:?}",
			info.num_headers(),
			P::SOURCE_NAME,
			maybe_justified_header.as_ref().map(|justified_header| &justified_header.header),
		);

		Ok(match maybe_justified_header {
			Some(justified_header) => Self::Regular(unjustified_headers, justified_header),
			None => Self::None(unjustified_headers),
		})
	}

	/// Returns selected mandatory header if we have seen one. Otherwise returns `None`.
	pub fn select_mandatory(self) -> Option<JustifiedHeader<P>> {
		match self {
			JustifiedHeaderSelector::Mandatory(header) => Some(header),
			_ => None,
		}
	}

	/// Tries to improve previously selected header using ephemeral
	/// justifications stream.
	pub fn select(
		self,
		info: &SyncInfo<P>,
		headers_to_relay: HeadersToRelay,
		free_headers_interval: Option<P::Number>,
		buf: &FinalityProofsBuf<P>,
	) -> Option<JustifiedHeader<P>> {
		let (unjustified_headers, maybe_justified_header) = match self {
			JustifiedHeaderSelector::Mandatory(justified_header) => return Some(justified_header),
			JustifiedHeaderSelector::Regular(unjustified_headers, justified_header) =>
				(unjustified_headers, Some(justified_header)),
			JustifiedHeaderSelector::None(unjustified_headers) => (unjustified_headers, None),
		};

		let mut finality_proofs_iter = buf.buf().iter().rev();
		let mut maybe_finality_proof = finality_proofs_iter.next();

		let mut unjustified_headers_iter = unjustified_headers.iter().rev();
		let mut maybe_unjustified_header = unjustified_headers_iter.next();

		while let (Some(finality_proof), Some(unjustified_header)) =
			(maybe_finality_proof, maybe_unjustified_header)
		{
			match finality_proof.target_header_number().cmp(&unjustified_header.number()) {
				Ordering::Equal
					if need_to_relay::<P>(
						info,
						headers_to_relay,
						free_headers_interval,
						&unjustified_header,
					) =>
				{
					log::trace!(
						target: "bridge",
						"Managed to improve selected {} finality proof {:?} to {:?}.",
						P::SOURCE_NAME,
						maybe_justified_header.as_ref().map(|justified_header| justified_header.number()),
						finality_proof.target_header_number()
					);
					return Some(JustifiedHeader {
						header: unjustified_header.clone(),
						proof: finality_proof.clone(),
					})
				},
				Ordering::Equal => {
					maybe_finality_proof = finality_proofs_iter.next();
					maybe_unjustified_header = unjustified_headers_iter.next();
				},
				Ordering::Less => maybe_unjustified_header = unjustified_headers_iter.next(),
				Ordering::Greater => {
					maybe_finality_proof = finality_proofs_iter.next();
				},
			}
		}

		log::trace!(
			target: "bridge",
			"Could not improve selected {} finality proof {:?}.",
			P::SOURCE_NAME,
			maybe_justified_header.as_ref().map(|justified_header| justified_header.number())
		);
		maybe_justified_header
	}
}

/// Returns true if we want to relay header `header_number`.
fn need_to_relay<P: FinalitySyncPipeline>(
	info: &SyncInfo<P>,
	headers_to_relay: HeadersToRelay,
	free_headers_interval: Option<P::Number>,
	header: &P::Header,
) -> bool {
	match headers_to_relay {
		HeadersToRelay::All => true,
		HeadersToRelay::Mandatory => header.is_mandatory(),
		HeadersToRelay::Free =>
			header.is_mandatory() ||
				free_headers_interval
					.map(|free_headers_interval| {
						header.number().saturating_sub(info.best_number_at_target) >=
							free_headers_interval
					})
					.unwrap_or(false),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	#[test]
	fn select_better_recent_finality_proof_works() {
		let info = SyncInfo {
			best_number_at_source: 10,
			best_number_at_target: 5,
			is_using_same_fork: true,
		};

		// if there are no unjustified headers, nothing is changed
		let finality_proofs_buf =
			FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![TestFinalityProof(5)]);
		let justified_header =
			JustifiedHeader { header: TestSourceHeader(false, 2, 2), proof: TestFinalityProof(2) };
		let selector = JustifiedHeaderSelector::Regular(vec![], justified_header.clone());
		assert_eq!(
			selector.select(&info, HeadersToRelay::All, None, &finality_proofs_buf),
			Some(justified_header)
		);

		// if there are no buffered finality proofs, nothing is changed
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![]);
		let justified_header =
			JustifiedHeader { header: TestSourceHeader(false, 2, 2), proof: TestFinalityProof(2) };
		let selector = JustifiedHeaderSelector::Regular(
			vec![TestSourceHeader(false, 5, 5)],
			justified_header.clone(),
		);
		assert_eq!(
			selector.select(&info, HeadersToRelay::All, None, &finality_proofs_buf),
			Some(justified_header)
		);

		// if there's no intersection between recent finality proofs and unjustified headers,
		// nothing is changed
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![
			TestFinalityProof(1),
			TestFinalityProof(4),
		]);
		let justified_header =
			JustifiedHeader { header: TestSourceHeader(false, 2, 2), proof: TestFinalityProof(2) };
		let selector = JustifiedHeaderSelector::Regular(
			vec![TestSourceHeader(false, 9, 9), TestSourceHeader(false, 10, 10)],
			justified_header.clone(),
		);
		assert_eq!(
			selector.select(&info, HeadersToRelay::All, None, &finality_proofs_buf),
			Some(justified_header)
		);

		// if there's intersection between recent finality proofs and unjustified headers, but there
		// are no proofs in this intersection, nothing is changed
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![
			TestFinalityProof(7),
			TestFinalityProof(11),
		]);
		let justified_header =
			JustifiedHeader { header: TestSourceHeader(false, 2, 2), proof: TestFinalityProof(2) };
		let selector = JustifiedHeaderSelector::Regular(
			vec![
				TestSourceHeader(false, 8, 8),
				TestSourceHeader(false, 9, 9),
				TestSourceHeader(false, 10, 10),
			],
			justified_header.clone(),
		);
		assert_eq!(
			selector.select(&info, HeadersToRelay::All, None, &finality_proofs_buf),
			Some(justified_header)
		);

		// if there's intersection between recent finality proofs and unjustified headers and
		// there's a proof in this intersection:
		// - this better (last from intersection) proof is selected;
		// - 'obsolete' unjustified headers are pruned.
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![
			TestFinalityProof(7),
			TestFinalityProof(9),
		]);
		let justified_header =
			JustifiedHeader { header: TestSourceHeader(false, 2, 2), proof: TestFinalityProof(2) };
		let selector = JustifiedHeaderSelector::Regular(
			vec![
				TestSourceHeader(false, 8, 8),
				TestSourceHeader(false, 9, 9),
				TestSourceHeader(false, 10, 10),
			],
			justified_header,
		);
		assert_eq!(
			selector.select(&info, HeadersToRelay::All, None, &finality_proofs_buf),
			Some(JustifiedHeader {
				header: TestSourceHeader(false, 9, 9),
				proof: TestFinalityProof(9)
			})
		);

		// when only free headers needs to be relayed and there are no free headers
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![
			TestFinalityProof(7),
			TestFinalityProof(9),
		]);
		let selector = JustifiedHeaderSelector::None(vec![
			TestSourceHeader(false, 8, 8),
			TestSourceHeader(false, 9, 9),
			TestSourceHeader(false, 10, 10),
		]);
		assert_eq!(
			selector.select(&info, HeadersToRelay::Free, Some(7), &finality_proofs_buf),
			None,
		);

		// when only free headers needs to be relayed, mandatory header may be selected
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![
			TestFinalityProof(6),
			TestFinalityProof(9),
		]);
		let selector = JustifiedHeaderSelector::None(vec![
			TestSourceHeader(false, 8, 8),
			TestSourceHeader(true, 9, 9),
			TestSourceHeader(false, 10, 10),
		]);
		assert_eq!(
			selector.select(&info, HeadersToRelay::Free, Some(7), &finality_proofs_buf),
			Some(JustifiedHeader {
				header: TestSourceHeader(true, 9, 9),
				proof: TestFinalityProof(9)
			})
		);

		// when only free headers needs to be relayed and there is free header
		let finality_proofs_buf = FinalityProofsBuf::<TestFinalitySyncPipeline>::new(vec![
			TestFinalityProof(7),
			TestFinalityProof(9),
			TestFinalityProof(14),
		]);
		let selector = JustifiedHeaderSelector::None(vec![
			TestSourceHeader(false, 7, 7),
			TestSourceHeader(false, 10, 10),
			TestSourceHeader(false, 14, 14),
		]);
		assert_eq!(
			selector.select(&info, HeadersToRelay::Free, Some(7), &finality_proofs_buf),
			Some(JustifiedHeader {
				header: TestSourceHeader(false, 14, 14),
				proof: TestFinalityProof(14)
			})
		);
	}
}
