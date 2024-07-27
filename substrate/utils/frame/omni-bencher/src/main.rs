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

mod command;

use clap::Parser;
use env_logger::Env;
use sc_cli::Result;

fn main() -> Result<()> {
	env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
	log::warn!("The FRAME omni-bencher is not yet battle tested - double check the results.",);

	command::Command::parse().run()
}
