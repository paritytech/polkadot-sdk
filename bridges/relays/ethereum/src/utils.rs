// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

use backoff::ExponentialBackoff;
use std::time::Duration;

/// Max delay after connection-unrelated error happened before we'll try the
/// same request again.
const MAX_BACKOFF_INTERVAL: Duration = Duration::from_secs(60);

/// Macro that returns (client, Err(error)) tuple from function if result is Err(error).
#[macro_export]
macro_rules! bail_on_error {
	($result: expr) => {
		match $result {
			(client, Ok(result)) => (client, result),
			(client, Err(error)) => return (client, Err(error)),
			}
	};
}

/// Macro that returns (client, Err(error)) tuple from function if result is Err(error).
#[macro_export]
macro_rules! bail_on_arg_error {
	($result: expr, $client: ident) => {
		match $result {
			Ok(result) => result,
			Err(error) => return ($client, Err(error)),
			}
	};
}

/// Error type that can signal connection errors.
pub trait MaybeConnectionError {
	/// Returns true if error (maybe) represents connection error.
	fn is_connection_error(&self) -> bool;
}

/// Stringified error that may be either connection-related or not.
#[derive(Debug)]
pub enum StringifiedMaybeConnectionError {
	/// The error is connection-related error.
	Connection(String),
	/// The error is connection-unrelated error.
	NonConnection(String),
}

impl StringifiedMaybeConnectionError {
	/// Create new stringified connection error.
	pub fn new(is_connection_error: bool, error: String) -> Self {
		if is_connection_error {
			StringifiedMaybeConnectionError::Connection(error)
		} else {
			StringifiedMaybeConnectionError::NonConnection(error)
		}
	}
}

impl MaybeConnectionError for StringifiedMaybeConnectionError {
	fn is_connection_error(&self) -> bool {
		match *self {
			StringifiedMaybeConnectionError::Connection(_) => true,
			StringifiedMaybeConnectionError::NonConnection(_) => false,
		}
	}
}

impl ToString for StringifiedMaybeConnectionError {
	fn to_string(&self) -> String {
		match *self {
			StringifiedMaybeConnectionError::Connection(ref err) => err.clone(),
			StringifiedMaybeConnectionError::NonConnection(ref err) => err.clone(),
		}
	}
}

/// Exponential backoff for connection-unrelated errors retries.
pub fn retry_backoff() -> ExponentialBackoff {
	let mut backoff = ExponentialBackoff::default();
	// we do not want relayer to stop
	backoff.max_elapsed_time = None;
	backoff.max_interval = MAX_BACKOFF_INTERVAL;
	backoff
}

/// Compact format of IDs vector.
pub fn format_ids<Id: std::fmt::Debug>(mut ids: impl ExactSizeIterator<Item = Id>) -> String {
	const NTH_PROOF: &str = "we have checked len; qed";
	match ids.len() {
		0 => "<nothing>".into(),
		1 => format!("{:?}", ids.next().expect(NTH_PROOF)),
		2 => {
			let id0 = ids.next().expect(NTH_PROOF);
			let id1 = ids.next().expect(NTH_PROOF);
			format!("[{:?}, {:?}]", id0, id1)
		}
		len => {
			let id0 = ids.next().expect(NTH_PROOF);
			let id_last = ids.last().expect(NTH_PROOF);
			format!("{}:[{:?} ... {:?}]", len, id0, id_last)
		}
	}
}
