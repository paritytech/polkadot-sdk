// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use serde::{Deserialize, Serialize};
use sp_core::Bytes;
use sp_statement_store::{InvalidReason, RejectionReason};

/// Subscription notification event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum SubscribeEvent {
	/// Statements admitted before the filter was attached
	ReplayStatements {
		/// Filter that produced this replay batch
		#[serde(rename = "filterId")]
		filter_id: String,
		/// SCALE-encoded statements included in this replay batch
		statements: Vec<Bytes>,
	},
	/// Replay completion marker
	ReplayDone {
		/// Filter whose replay completed
		#[serde(rename = "filterId")]
		filter_id: String,
	},
	/// Statements admitted after matching filters were attached
	NewStatements {
		/// Statement entries included in this notification
		statements: Vec<NewStatementEntry>,
	},
	/// Terminal notification
	Stop,
}

/// Statement item included in a `newStatements` notification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewStatementEntry {
	/// SCALE-encoded statement bytes
	pub statement: Bytes,
	/// Filters that matched this statement
	#[serde(rename = "filterIds")]
	pub filter_ids: Vec<String>,
}

/// Response returned by `statement_unstable_add_filter`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AddFilterResponse {
	/// Filter was added and the string contains its filter id
	Ok(String),
	/// Filter could not be added because the subscription reached its limit
	LimitReached(LimitReachedResult),
}

/// Response payload for a limit-reached add-filter result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LimitReachedResult {
	/// Machine-readable result tag
	pub result: LimitReachedTag,
}

/// Result tag returned when the filter limit is reached
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LimitReachedTag {
	/// The subscription cannot accept another filter
	LimitReached,
}

impl AddFilterResponse {
	/// Returns the limit-reached response
	pub fn limit_reached() -> Self {
		AddFilterResponse::LimitReached(LimitReachedResult {
			result: LimitReachedTag::LimitReached,
		})
	}
}

/// Statement submission outcome
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SubmitOutcome {
	/// The statement was accepted and was not already present in the store
	New,
	/// The statement is already present in the store
	Known,
	/// The statement was valid but the store rejected it
	Rejected(RejectionReason),
	/// The statement failed validation
	Invalid(InvalidReason),
}
