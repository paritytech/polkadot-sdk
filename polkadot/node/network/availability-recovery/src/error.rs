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

//! The `Error` and `Result` types used by the subsystem.

use crate::LOG_TARGET;
use fatality::{fatality, Nested};
use futures::channel::oneshot;
use polkadot_node_network_protocol::request_response::incoming;
use polkadot_node_subsystem::{ChainApiError, RecoveryError, SubsystemError};
use polkadot_primitives::Hash;

/// Error type used by the Availability Recovery subsystem.
#[fatality(splitable)]
pub enum Error {
	#[fatal]
	#[error("Spawning subsystem task failed: {0}")]
	SpawnTask(#[source] SubsystemError),

	/// Receiving subsystem message from overseer failed.
	#[fatal]
	#[error("Receiving message from overseer failed: {0}")]
	SubsystemReceive(#[source] SubsystemError),

	#[fatal]
	#[error("failed to query full data from store")]
	CanceledQueryFullData(#[source] oneshot::Canceled),

	#[fatal]
	#[error("failed to query session info")]
	CanceledSessionInfo(#[source] oneshot::Canceled),

	#[error("`SessionInfo` is `None` at {0}")]
	SessionInfoUnavailable(Hash),

	#[error("failed to query client features from runtime")]
	RequestClientFeatures(#[source] polkadot_node_subsystem_util::runtime::Error),

	#[error("failed to send response")]
	CanceledResponseSender,

	#[error(transparent)]
	Runtime(#[from] polkadot_node_subsystem::errors::RuntimeApiError),

	#[error(transparent)]
	Erasure(#[from] polkadot_erasure_coding::Error),

	// #[error(transparent)]
	// Util(#[from] polkadot_node_subsystem_util::Error),
	#[fatal]
	#[error("Oneshot for receiving response from Chain API got cancelled")]
	ChainApiSenderDropped(#[source] oneshot::Canceled),

	#[error("Retrieving response from Chain API unexpectedly failed with error: {0}")]
	ChainApi(#[from] ChainApiError),

	#[error("Cannot find block number for given relay parent")]
	BlockNumberNotFound,

	#[fatal(forward)]
	#[error("Error during recovery: {0}")]
	Recovery(#[from] RecoveryError),

	#[fatal(forward)]
	#[error("Retrieving next incoming request failed: {0}")]
	IncomingRequest(#[from] incoming::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Utility for eating top level errors and log them.
///
/// We basically always want to try and continue on error, unless the error is fatal for the entire
/// subsystem.
pub fn log_error(result: Result<()>) -> std::result::Result<(), FatalError> {
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
