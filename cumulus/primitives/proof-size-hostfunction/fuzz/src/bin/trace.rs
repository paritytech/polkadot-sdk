// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Reads one SCALE-encoded `Script` from stdin and writes its version 1 trace to stdout. Built
//! on master with `--no-default-features`, it is the reference side of `tests/v1_compat.rs`.

use codec::DecodeAll;
use cumulus_proof_size_hostfunction_fuzz::{trace, Script};
use std::io::{Read, Write};

fn main() {
	let mut input = Vec::new();
	std::io::stdin().read_to_end(&mut input).expect("stdin is readable");
	let script =
		Script::decode_all(&mut &input[..]).expect("stdin holds exactly one encoded `Script`");
	let mut stdout = std::io::stdout().lock();
	for line in trace(&script) {
		writeln!(stdout, "{line}").expect("stdout is writable");
	}
}
