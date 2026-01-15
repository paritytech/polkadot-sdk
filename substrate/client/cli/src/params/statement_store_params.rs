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

use clap::Args;

/// The default number of subscription filter worker tasks.
pub const DEFAULT_STATEMENT_STORE_FILTER_WORKERS: usize = 1;

/// Parameters used to configure the statement store.
#[derive(Debug, Clone, Args)]
pub struct StatementStoreParams {
	/// Number of subscription filter worker tasks for the statement store.
	///
	/// Controls the parallelism of statement filtering operations for subscriptions.
	/// Higher values increase concurrency but use more resources.
	#[arg(long, value_name = "COUNT", default_value_t = DEFAULT_STATEMENT_STORE_FILTER_WORKERS)]
	pub statement_store_filter_workers: usize,
}
