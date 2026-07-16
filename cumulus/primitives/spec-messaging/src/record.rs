// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The per-block consumption record.
//!
//! Blocks never emit `Requires` signals and never see a `StreamsRoot`.
//! Processing the messaging inherent writes this record — which streams the
//! block touched is what the inherent *did*, never state inference — and the
//! `validate_block` wrapper stitches the intervals across the bundle and
//! synthesizes the candidate's requires entries via POV-carried lifts (see
//! [`crate::lift`]).

use alloc::{collections::BTreeMap, vec::Vec};
use polkadot_parachain_primitives::primitives::Id as ParaId;

use crate::{
	mmr::{MmrFrontier, MmrRoot},
	stream_id::StreamId,
};

/// Per stream and block: consumption entered the block at `start` and left
/// it at `end`.
///
/// Why this exists: the candidate's lift binds only the LAST state to a
/// committed root. The intervals stretch that guarantee back over the whole
/// bundle — each block must start where the previous one ended, or a
/// POV-carried advance proof shows the jump moved forward.
///
/// Channel streams cannot gap: consumption is a stored frontier every block
/// continues from, so `start == previous end` holds by construction.
/// Register/event reads pick their read context freely, so contexts CAN
/// jump — and without the chain, a block could act on reads against a
/// fabricated context and hide behind a later, genuine one. The fabricated
/// context breaks the chain instead.
///
/// Channels: the frontier's root before / the frontier after the block's
/// incoming messages. Reads: `end` = the context the block's reads were
/// verified against, `start` = its root (nothing advances). `end` is a full
/// frontier because the next gap check, or the lift, extends from it.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct Interval {
	/// Stream state the block's consumption started from.
	pub start: MmrRoot,
	/// Stream state the block's consumption ended at.
	pub end: MmrFrontier,
}

/// A block's consumption record: one [`Interval`] per stream the block
/// touched.
///
/// Written per block to a transient outbox (the same storage family that
/// carries `UpwardMessages`) and exposed through the `consumption_record()`
/// runtime API. Two callers, one definition: the node reads it via ordinary
/// API dispatch (authoring, acknowledgement checks, diagnostics); the
/// `validate_block` wrapper calls the API's implementation directly in-wasm
/// after executing each block.
///
/// Grouped by source, mirroring the lifts' transport
/// ([`crate::lift::LiftsBySource`]) and the one requires entry per source it
/// feeds; per source sorted by the [`StreamId`]'s canonical encoding (`Ord`
/// on `StreamId` is that order) and unique — the messaging inherent carries
/// at most one item per stream, the STF rejects duplicates.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct ConsumptionRecord {
	/// Touched streams with their intervals, grouped by source chain.
	pub entries: BTreeMap<ParaId, Vec<(StreamId, Interval)>>,
}
