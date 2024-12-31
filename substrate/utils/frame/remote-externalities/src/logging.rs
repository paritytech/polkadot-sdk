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

use std::{
	future::Future,
	io::{self, IsTerminal},
	time::Instant,
};

use spinners::{Spinner, Spinners};

use super::Result;

// A simple helper to time an async operation with a nice spinner, start message, and end message.
//
// The spinner is only displayed when stdout is a terminal.
pub(super) fn with_elapsed<F, R, EndMsg>(f: F, start_msg: &str, end_msg: EndMsg) -> Result<R>
where
	F: FnOnce() -> Result<R>,
	EndMsg: FnOnce(&R) -> String,
{
	if io::stdout().is_terminal() {
		let (start, mut sp) = start(start_msg);
		let r = f()?;

		Ok(end(r, start, &mut sp, end_msg))
	} else {
		f()
	}
}

// A simple helper to time a operation with a nice spinner, start message, and end message.
//
// The spinner is only displayed when stdout is a terminal.
pub(super) async fn with_elapsed_async<F, Fut, R, EndMsg>(
	f: F,
	start_msg: &str,
	end_msg: EndMsg,
) -> Result<R>
where
	F: FnOnce() -> Fut,
	Fut: Future<Output = Result<R>>,
	EndMsg: FnOnce(&R) -> String,
{
	if io::stdout().is_terminal() {
		let (start, mut sp) = start(start_msg);
		let r = f().await?;

		Ok(end(r, start, &mut sp, end_msg))
	} else {
		f().await
	}
}

fn start(start_msg: &str) -> (Instant, Spinner) {
	let start = Instant::now();
	let sp = Spinner::new(Spinners::Dots, format!("⏳ {start_msg}"));

	(start, sp)
}

fn end<T, EndMsg>(val: T, start: Instant, sp: &mut Spinner, end_msg: EndMsg) -> T
where
	EndMsg: FnOnce(&T) -> String,
{
	sp.stop_with_message(format!("✅ {} in {:.2}s", end_msg(&val), start.elapsed().as_secs_f32()));

	val
}
