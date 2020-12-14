// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.
//
// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Block announcement validation.

use crate::BlockStatus;
use sp_runtime::{generic::BlockId, traits::Block};
use std::{error::Error, future::Future, pin::Pin, sync::Arc};
use futures::FutureExt as _;

/// A type which provides access to chain information.
pub trait Chain<B: Block> {
	/// Retrieve the status of the block denoted by the given [`BlockId`].
	fn block_status(&self, id: &BlockId<B>) -> Result<BlockStatus, Box<dyn Error + Send>>;
}

impl<T: Chain<B>, B: Block> Chain<B> for Arc<T> {
	fn block_status(&self, id: &BlockId<B>) -> Result<BlockStatus, Box<dyn Error + Send>> {
		(&**self).block_status(id)
	}
}

/// Result of `BlockAnnounceValidator::validate`.
#[derive(Debug, PartialEq, Eq)]
pub enum Validation {
	/// Valid block announcement.
	Success {
		/// Is this the new best block of the node?
		is_new_best: bool,
	},
	/// Invalid block announcement.
	Failure {
		/// Should we disconnect from this peer?
		///
		/// This should be used if the peer for example send junk to spam us.
		disconnect: bool,
	},
}

/// Type which checks incoming block announcements.
pub trait BlockAnnounceValidator<B: Block> {
	/// Validate the announced header and its associated data.
	///
	/// # Note
	///
	/// Returning [`Validation::Failure`] will lead to a decrease of the
	/// peers reputation as it sent us invalid data.
	fn validate(
		&mut self,
		header: &B::Header,
		data: &[u8],
	) -> Pin<Box<dyn Future<Output = Result<Validation, Box<dyn Error + Send>>> + Send>>;
}

/// Default implementation of `BlockAnnounceValidator`.
#[derive(Debug)]
pub struct DefaultBlockAnnounceValidator;

impl<B: Block> BlockAnnounceValidator<B> for DefaultBlockAnnounceValidator {
	fn validate(
		&mut self,
		_: &B::Header,
		data: &[u8],
	) -> Pin<Box<dyn Future<Output = Result<Validation, Box<dyn Error + Send>>> + Send>> {
		let is_empty = data.is_empty();

		async move {
			if !is_empty {
				log::debug!(
					target: "sync",
					"Received unknown data alongside the block announcement.",
				);
				Ok(Validation::Failure { disconnect: true })
			} else {
				Ok(Validation::Success { is_new_best: false })
			}
		}.boxed()
	}
}
