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

use crate::LOG_TARGET;
use fatality::Nested;
use polkadot_node_subsystem::{ChainApiError, SubsystemError};
use polkadot_node_subsystem_util::runtime;
use polkadot_primitives::Hash;

pub type Result<T> = std::result::Result<T, Error>;
pub type FatalResult<T> = std::result::Result<T, FatalError>;

#[fatality::fatality(splitable)]
pub enum Error {
	#[fatal]
	#[error("Oneshot for receiving ancestors from chain API got cancelled")]
	CanceledAncestors,
	#[fatal]
	#[error("Oneshot for receiving finalized block number from chain API got cancelled")]
	CanceledFinalizedBlockNumber,
	#[fatal]
	#[error("Oneshot for receiving finalized block hash from chain API got cancelled")]
	CanceledFinalizedBlockHash,
	#[error("Finalized block hash for {0} not found")]
	FinalizedBlockNotFound(u32),
	#[error(transparent)]
	ChainApi(#[from] ChainApiError),
	#[fatal(forward)]
	#[error("Error while accessing runtime information {0}")]
	Runtime(#[from] runtime::Error),
	#[fatal]
	#[error("Receiving message from overseer failed: {0}")]
	SubsystemReceive(#[source] SubsystemError),
}

/// Utility for eating top level errors and log them.
///
/// We basically always want to try and continue on error. This utility function is meant to
/// consume top-level errors by simply logging them
pub fn log_error(result: Result<()>) -> FatalResult<()> {
	match result.into_nested()? {
		Ok(()) => Ok(()),
		Err(jfyi) => {
			jfyi.log();
			Ok(())
		},
	}
}

impl JfyiError {
	/// Log a `JfyiError`.
	pub fn log(self) {
		gum::warn!(target: LOG_TARGET, error = ?self);
	}
}
