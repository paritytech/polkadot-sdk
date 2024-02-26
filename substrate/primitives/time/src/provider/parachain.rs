// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{Instant, InstantProvider};
use sp_runtime::traits::Get;

/// Adapter for converting a `Get<I>` into an `InstantProvider<I>`.
pub struct TimestampInstantProvider<I, N>(core::marker::PhantomData<(I, N)>);

impl<I: Instant, N: Get<I>> InstantProvider<I> for TimestampInstantProvider<I, N> {
	fn now() -> I {
		N::get()
	}
}
