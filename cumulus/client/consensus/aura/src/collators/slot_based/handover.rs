// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Splits resubmittable bundles by the author the PVF expects to sign them.

use crate::LOG_TARGET;
use sp_runtime::traits::Header as HeaderT;

/// Resubmittable headers split by the author the PVF expects to sign each bundle.
pub(super) struct Classified<A, H> {
	pub tip: Vec<H>,
	pub handover: Option<(A, Vec<H>)>,
}

/// `bundles` pairs each whole bundle with the author expected at its first header's parent,
/// `None` when that read failed.
pub(super) fn classify_bundles<A: Clone + PartialEq, H: HeaderT>(
	bundles: impl IntoIterator<Item = (Option<A>, Vec<H>)>,
	tip_author: Option<&A>,
	is_ours: impl Fn(&A) -> bool,
) -> Classified<A, H> {
	let mut tip = Vec::new();
	let mut handover: Option<(A, Vec<H>)> = None;
	for (author, bundle) in bundles {
		let first = bundle.first().map(|header| header.hash());
		let Some(author) = author else {
			tracing::debug!(target: LOG_TARGET, ?first, "Dropping bundle: authority read failed.");
			continue;
		};
		if Some(&author) == tip_author {
			tip.extend(bundle);
		} else if !is_ours(&author) {
			tracing::debug!(
				target: LOG_TARGET,
				?first,
				"Dropping bundle: prior-set slot author's key is not in the keystore.",
			);
		} else {
			match &mut handover {
				None => handover = Some((author, bundle)),
				Some((current, headers)) if *current == author => headers.extend(bundle),
				Some(_) => tracing::debug!(
					target: LOG_TARGET,
					?first,
					"Dropping bundle: a different prior-set author already owns the handover.",
				),
			}
		}
	}
	Classified { tip, handover }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collators::slot_based::resubmittable_segment::split_bundles;
	use cumulus_primitives_core::BlockBundleInfo;
	use cumulus_test_client::runtime::Block as TestBlock;
	use sp_runtime::traits::Block as BlockT;

	type TestHeader = <TestBlock as BlockT>::Header;

	const TIP: u32 = 1;
	const PRIOR: u32 = 2;
	const FOREIGN: u32 = 3;

	fn header(number: u32, index: u8) -> TestHeader {
		let mut header = TestHeader::new(
			number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		header
			.digest_mut()
			.push(BlockBundleInfo { index, is_last: false }.to_digest_item());
		header
	}

	fn numbers(headers: &[TestHeader]) -> Vec<u32> {
		headers.iter().map(|h| *h.number()).collect()
	}

	/// Resolve each bundle's author from its first header's number, as the parent-state read does.
	fn classify(
		headers: Vec<TestHeader>,
		author_of: impl Fn(u32) -> Option<u32>,
	) -> Classified<u32, TestHeader> {
		let bundles = split_bundles(headers)
			.into_iter()
			.map(|bundle| (author_of(*bundle[0].number()), bundle))
			.collect::<Vec<_>>();
		classify_bundles(bundles, Some(&TIP), |a| *a != FOREIGN)
	}

	#[test]
	fn mid_bundle_rotation_keeps_bundles_whole() {
		// The rotation lands in block 2, so per-header parents would read pre, pre, post, post,
		// post; the bundle's first parent decides for the whole bundle.
		let headers = vec![header(1, 0), header(2, 1), header(3, 2), header(4, 0), header(5, 1)];
		let out = classify(headers, |first| Some(if first == 1 { PRIOR } else { TIP }));

		let (author, handover) = out.handover.expect("first bundle is the prior set's; qed");
		assert_eq!(author, PRIOR);
		assert_eq!(numbers(&handover), vec![1, 2, 3]);
		assert_eq!(numbers(&out.tip), vec![4, 5]);
	}

	#[test]
	fn unreadable_and_foreign_bundles_are_dropped() {
		let headers = vec![header(1, 0), header(2, 0), header(3, 0)];
		let out = classify(headers, |first| match first {
			1 => None,
			2 => Some(FOREIGN),
			_ => Some(TIP),
		});
		assert!(out.handover.is_none());
		assert_eq!(numbers(&out.tip), vec![3]);

		let steady = classify(vec![header(1, 0), header(2, 1), header(3, 0)], |_| Some(TIP));
		assert!(steady.handover.is_none());
		assert_eq!(numbers(&steady.tip), vec![1, 2, 3]);
	}
}
