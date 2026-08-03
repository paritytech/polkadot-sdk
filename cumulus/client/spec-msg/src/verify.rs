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

//! Requester-side verification of `/spec-msg/exchange` responses.
//!
//! Responses are untrusted. Everything is derived, nothing declared: hash
//! the payloads into leaves and append them to the start frontier, let the
//! extension proof yield the stream's root, let the tree proof yield a
//! `StreamsRoot` — and compare against the root the *request* named. A
//! mismatch anywhere means a bad response; the caller discards it and the
//! peer. The trust-free hints (`base`, `leaf_version`, `start_peaks`)
//! cannot subvert this — a lie only fails the end-to-end comparison.
//!
//! The receiver-side fetch loop and the inherent provider consume these
//! functions; they are here so the serving tests and the consumers verify
//! with the same code.

use cumulus_primitives_spec_messaging::{
	hash_leaf, EventRequest, EventResponse, MessagePosition, MessagesRequest, MessagesResponse,
	MmrError, MmrFrontier, SpecHasher, TreeError, LEAF_VERSION,
};

/// Why a response was rejected. Any variant means: discard the response
/// and the peer that sent it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum VerifyError {
	/// The response's `base` differs from the request's `start`.
	#[error("response base does not match the requested start")]
	BaseMismatch,
	/// The caller-provided frontier does not match the request, or the
	/// server's `start_peaks` hint contradicts it.
	#[error("own frontier does not match the request or the start peaks")]
	FrontierMismatch,
	/// `start_peaks` is not a consistent frontier for `base`.
	#[error("start peaks are not a consistent frontier")]
	MalformedPeaks,
	/// The extension proof is structurally invalid.
	#[error("extension proof invalid: {0:?}")]
	Extension(MmrError),
	/// The inclusion proof is structurally invalid.
	#[error("inclusion proof invalid: {0:?}")]
	Inclusion(MmrError),
	/// The tree proof is structurally invalid.
	#[error("tree proof invalid: {0:?}")]
	Tree(TreeError),
	/// Everything was well-formed but the derived root is not the one the
	/// request named.
	#[error("response does not verify under the requested root")]
	RootMismatch,
}

/// What a verified [`MessagesResponse`] pins down.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedMessages {
	/// Frontier after appending the served payloads — the requester's
	/// resume state: the next chunk is requested from `end.leaf_count`.
	pub end: MmrFrontier,
	/// The stream's leaf count under `under` — proven, not hinted:
	/// `end.leaf_count < head` means more chunks remain under this root.
	pub head: u64,
}

/// What a verified [`EventResponse`] pins down.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedEvent {
	/// The served leaf's position, proven under `under` (for head reads:
	/// derived from the proof, an old leaf cannot pass as the head).
	pub position: MessagePosition,
	/// For head reads (`at: None`): the stream's full frontier under
	/// `under` — the read context that register/gap machinery extends from.
	pub frontier: Option<MmrFrontier>,
}

/// Verifies `response` against the root `request` named.
///
/// `own` is the channel receiver's tracked frontier at `request.start`
/// (cross-checked against the `start_peaks` hint before any hashing);
/// `None` verifies trust-free from `start_peaks` — the no-frontier consumer
/// (event subscribers fetching a range).
pub fn verify_messages_response(
	request: &MessagesRequest,
	response: &MessagesResponse,
	own: Option<&MmrFrontier>,
) -> Result<VerifiedMessages, VerifyError> {
	if response.base != request.start {
		return Err(VerifyError::BaseMismatch);
	}
	let mut frontier = match own {
		Some(own) => {
			if own.leaf_count != request.start.0 || !own.is_consistent() {
				return Err(VerifyError::FrontierMismatch);
			}
			if own.peaks.as_slice() != response.start_peaks.as_slice() {
				return Err(VerifyError::FrontierMismatch);
			}
			own.clone()
		},
		None => {
			let peaks = response
				.start_peaks
				.clone()
				.try_into()
				.map_err(|_| VerifyError::MalformedPeaks)?;
			let frontier = MmrFrontier { leaf_count: response.base.0, peaks };
			if !frontier.is_consistent() {
				return Err(VerifyError::MalformedPeaks);
			}
			frontier
		},
	};

	// `leaf_version` is trust-free: versions are hash-disjoint domains, so
	// only the correct one reproduces a committed root.
	for payload in &response.payloads {
		frontier.append_leaf(hash_leaf::<SpecHasher>(response.leaf_version, payload));
	}

	let root = response.extension.verify(&frontier).map_err(VerifyError::Extension)?;
	let streams_root =
		response.tree_proof.verify(&request.stream, &root).map_err(VerifyError::Tree)?;
	if streams_root != request.under {
		return Err(VerifyError::RootMismatch);
	}

	let head = if response.extension.is_empty() {
		frontier.leaf_count
	} else {
		response.extension.leaf_count
	};
	Ok(VerifiedMessages { end: frontier, head })
}

/// Verifies `response` against the root `request` named.
///
/// For `at: None` the head-ness comes with the check: `under` fixes the
/// stream's leaf count, so a stale leaf served as the head yields a
/// different stream root and fails the comparison.
pub fn verify_event_response(
	request: &EventRequest,
	response: &EventResponse,
) -> Result<VerifiedEvent, VerifyError> {
	let leaf = hash_leaf::<SpecHasher>(LEAF_VERSION, &response.payload);
	let (position, frontier, root) = match request.at {
		Some(position) => {
			let root =
				response.inclusion.verify_leaf(position, leaf).map_err(VerifyError::Inclusion)?;
			(position, None, root)
		},
		None => {
			let (position, frontier) =
				response.inclusion.verify_head(leaf).map_err(VerifyError::Inclusion)?;
			let root = frontier.root();
			(position, Some(frontier), root)
		},
	};

	let streams_root =
		response.tree_proof.verify(&request.stream, &root).map_err(VerifyError::Tree)?;
	if streams_root != request.under {
		return Err(VerifyError::RootMismatch);
	}
	Ok(VerifiedEvent { position, frontier })
}
