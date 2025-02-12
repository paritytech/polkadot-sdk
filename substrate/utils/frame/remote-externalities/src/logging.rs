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

// A simple helper to time a operation with a nice spinner, start message, and end message.
//
// The spinner is only displayed when stdout is a terminal.
pub(super) fn with_elapsed<F, R, EndMsg>(f: F, start_msg: &str, end_msg: EndMsg) -> Result<R>
where
	F: FnOnce() -> Result<R>,
	EndMsg: FnOnce(&R) -> String,
{
	let timer = Instant::now();
	let mut maybe_sp = start(start_msg);

	Ok(end(f()?, timer, maybe_sp.as_mut(), end_msg))
}

// A simple helper to time an async operation with a nice spinner, start message, and end message.
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
	let timer = Instant::now();
	let mut maybe_sp = start(start_msg);

	Ok(end(f().await?, timer, maybe_sp.as_mut(), end_msg))
}

fn start(start_msg: &str) -> Option<Spinner> {
	let msg = format!("⏳ {start_msg}");

	if io::stdout().is_terminal() {
		Some(Spinner::new(Spinners::Dots, msg))
	} else {
		println!("{msg}");

		None
	}
}

fn end<T, EndMsg>(val: T, timer: Instant, maybe_sp: Option<&mut Spinner>, end_msg: EndMsg) -> T
where
	EndMsg: FnOnce(&T) -> String,
{
	let msg = format!("✅ {} in {:.2}s", end_msg(&val), timer.elapsed().as_secs_f32());

	if let Some(sp) = maybe_sp {
		sp.stop_with_message(msg);
	} else {
		println!("{msg}");
	}

	val
}
