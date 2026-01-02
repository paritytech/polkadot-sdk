// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Staging Primitives.

use scale_info::TypeInfo;
use sp_api::__private::{Decode, Encode};
use sp_application_crypto::RuntimeDebug;
use sp_core::DecodeWithMemTracking;
use sp_staking::SessionIndex;
use crate::ValidatorIndex;
use alloc::vec::Vec;

/// A reward tally line represent the collected statistics about
/// approvals voting for a given validator, how much successful approvals
/// was collected and how many times the given validator no-showed
#[derive(
    RuntimeDebug,
    Copy,
    Clone,
    PartialEq,
    Encode,
    Decode,
    DecodeWithMemTracking,
    TypeInfo,
)]
pub struct ApprovalStatisticsTallyLine {
    ///  represents the validator to which the statistics belongs to
    pub validator_index: ValidatorIndex,

    /// how many times the validator had sent useful approval votes
    /// that contribute the successful approval of a candidate
    pub approvals_usage: u32,

    /// how many times the validator was supposed to send a vote but
    /// no showed
    pub no_shows: u32,
}

/// ApprovalRewards is the set of tallies where each tally represents
/// a given validator and its approval voting statistics
#[derive(
    RuntimeDebug,
    Clone,
    PartialEq,
    Encode,
    Decode,
    DecodeWithMemTracking,
    TypeInfo,
)]
pub struct ApprovalStatistics(pub SessionIndex, pub ValidatorIndex, pub Vec<ApprovalStatisticsTallyLine>);

impl ApprovalStatistics {
    pub fn signing_payload(&self) -> Vec<u8> {
        const MAGIC: [u8; 4] = *b"APST"; // for "approval statistics"
        (MAGIC, self.0, self.1.clone()).encode()
    }
}
