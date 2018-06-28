// Copyright 2017 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use super::*;

use std::{fmt, sync::Arc};
use extrinsic_pool::api;
use test_client;
use parking_lot::Mutex;

type Extrinsic = u64;
type Hash = u64;

#[derive(Default)]
struct DummyTxPool {
	submitted: Mutex<Vec<Extrinsic>>,
}

#[derive(Debug)]
struct Error;
impl api::Error for Error {}
impl ::std::error::Error for Error {
	fn description(&self) -> &str { "Error" }
}
impl fmt::Display for Error {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		fmt::Debug::fmt(self, fmt)
	}
}

impl<BlockHash> api::ExtrinsicPool<Extrinsic, BlockHash, u64> for DummyTxPool {
	type Error = Error;

	/// Submit extrinsic for inclusion in block.
	fn submit(&self, _block: BlockHash, xt: Vec<Extrinsic>) -> ::std::result::Result<Vec<Hash>, Self::Error> {
		let mut submitted = self.submitted.lock();
		if submitted.len() < 1 {
			let hashes = xt.iter().map(|_xt| 1).collect();
			submitted.extend(xt);
			Ok(hashes)
		} else {
			Err(Error)
		}
	}
}

#[test]
fn submit_transaction_should_not_cause_error() {
	let p = Author {
		client: Arc::new(test_client::new()),
		pool: Arc::new(DummyTxPool::default()),
	};

	assert_matches!(
		AuthorApi::submit_extrinsic(&p, u64::encode(&5).into()),
		Ok(1)
	);
	assert!(
		AuthorApi::submit_extrinsic(&p, u64::encode(&5).into()).is_err()
	);
}

#[test]
fn submit_rich_transaction_should_not_cause_error() {
	let p = Author {
		client: Arc::new(test_client::new()),
		pool: Arc::new(DummyTxPool::default()),
	};

	assert_matches!(
		AuthorApi::submit_rich_extrinsic(&p, 5),
		Ok(1)
	);
	assert!(
		AuthorApi::submit_rich_extrinsic(&p, 5).is_err()
	);
}
