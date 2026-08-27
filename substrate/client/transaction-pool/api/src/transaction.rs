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

//! The in-pool transaction of a full node's transaction pool.
//!
//! This lives in the API crate rather than next to the pool implementation so that
//! [`crate::FullClientTransactionPool`] can name it as an associated type. Components that only
//! hold a pool then need nothing but this crate to spell out the pool's trait object.

use crate::InPoolTransaction;
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::transaction_validity::{
	TransactionLongevity as Longevity, TransactionPriority as Priority, TransactionSource,
	TransactionTag as Tag,
};
use std::{fmt, time::Instant};

/// A transaction source that includes a timestamp indicating when the transaction was submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedTransactionSource {
	/// The original source of the transaction.
	pub source: TransactionSource,

	/// The time at which the transaction was submitted.
	pub timestamp: Option<Instant>,
}

impl From<TimedTransactionSource> for TransactionSource {
	fn from(value: TimedTransactionSource) -> Self {
		value.source
	}
}

impl TimedTransactionSource {
	/// Creates a new instance with an internal `TransactionSource::InBlock` source and an optional
	/// timestamp.
	pub fn new_in_block(with_timestamp: bool) -> Self {
		Self { source: TransactionSource::InBlock, timestamp: with_timestamp.then(Instant::now) }
	}
	/// Creates a new instance with an internal `TransactionSource::External` source and an optional
	/// timestamp.
	pub fn new_external(with_timestamp: bool) -> Self {
		Self { source: TransactionSource::External, timestamp: with_timestamp.then(Instant::now) }
	}
	/// Creates a new instance with an internal `TransactionSource::Local` source and an optional
	/// timestamp.
	pub fn new_local(with_timestamp: bool) -> Self {
		Self { source: TransactionSource::Local, timestamp: with_timestamp.then(Instant::now) }
	}
	/// Creates a new instance with an given source and an optional timestamp.
	pub fn from_transaction_source(source: TransactionSource, with_timestamp: bool) -> Self {
		Self { source, timestamp: with_timestamp.then(Instant::now) }
	}
}

/// Immutable transaction
#[derive(PartialEq, Eq, Clone)]
pub struct Transaction<Hash, Extrinsic> {
	/// Raw extrinsic representing that transaction.
	pub data: Extrinsic,
	/// Number of bytes encoding of the transaction requires.
	pub bytes: usize,
	/// Transaction hash (unique)
	pub hash: Hash,
	/// Transaction priority (higher = better)
	pub priority: Priority,
	/// At which block the transaction becomes invalid?
	pub valid_till: Longevity,
	/// Tags required by the transaction.
	pub requires: Vec<Tag>,
	/// Tags that this transaction provides.
	pub provides: Vec<Tag>,
	/// Should that transaction be propagated.
	pub propagate: bool,
	/// Timed source of that transaction.
	pub source: TimedTransactionSource,
}

impl<Hash, Extrinsic> AsRef<Extrinsic> for Transaction<Hash, Extrinsic> {
	fn as_ref(&self) -> &Extrinsic {
		&self.data
	}
}

impl<Hash, Extrinsic> InPoolTransaction for Transaction<Hash, Extrinsic> {
	type Transaction = Extrinsic;
	type Hash = Hash;

	fn data(&self) -> &Extrinsic {
		&self.data
	}

	fn hash(&self) -> &Hash {
		&self.hash
	}

	fn priority(&self) -> &Priority {
		&self.priority
	}

	fn longevity(&self) -> &Longevity {
		&self.valid_till
	}

	fn requires(&self) -> &[Tag] {
		&self.requires
	}

	fn provides(&self) -> &[Tag] {
		&self.provides
	}

	fn is_propagable(&self) -> bool {
		self.propagate
	}
}

impl<Hash: Clone, Extrinsic: Clone> Transaction<Hash, Extrinsic> {
	/// Explicit transaction clone.
	///
	/// Transaction should be cloned only if absolutely necessary && we want
	/// every reason to be commented. That's why we `Transaction` is not `Clone`,
	/// but there's explicit `duplicate` method.
	pub fn duplicate(&self) -> Self {
		Self {
			data: self.data.clone(),
			bytes: self.bytes,
			hash: self.hash.clone(),
			priority: self.priority,
			source: self.source.clone(),
			valid_till: self.valid_till,
			requires: self.requires.clone(),
			provides: self.provides.clone(),
			propagate: self.propagate,
		}
	}
}

impl<Hash, Extrinsic> fmt::Debug for Transaction<Hash, Extrinsic>
where
	Hash: fmt::Debug,
	Extrinsic: fmt::Debug,
{
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		let join_tags = |tags: &[Tag]| {
			tags.iter()
				.map(|tag| HexDisplay::from(tag).to_string())
				.collect::<Vec<_>>()
				.join(", ")
		};

		write!(fmt, "Transaction {{ ")?;
		write!(fmt, "hash: {:?}, ", &self.hash)?;
		write!(fmt, "priority: {:?}, ", &self.priority)?;
		write!(fmt, "valid_till: {:?}, ", &self.valid_till)?;
		write!(fmt, "bytes: {:?}, ", &self.bytes)?;
		write!(fmt, "propagate: {:?}, ", &self.propagate)?;
		write!(fmt, "source: {:?}, ", &self.source)?;
		write!(fmt, "requires: [{}], ", join_tags(&self.requires))?;
		write!(fmt, "provides: [{}], ", join_tags(&self.provides))?;
		write!(fmt, "data: {:?}", &self.data)?;
		write!(fmt, "}}")?;
		Ok(())
	}
}
