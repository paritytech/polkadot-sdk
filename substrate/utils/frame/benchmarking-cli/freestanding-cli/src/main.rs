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

//! Entry point for the free standing `benchmark pallet` command runner.

use clap::Parser;
use frame_benchmarking_cli::PalletCmd;
use sc_cli::Result;
use sp_runtime::traits::BlakeTwo256;

#[cfg(feature = "extended-host-functions")]
type HostFunctions = sp_statement_store::runtime_api::HostFunctions;
#[cfg(not(feature = "extended-host-functions"))]
type HostFunctions = ();

fn main() -> Result<()> {
	PalletCmd::parse().run_with_spec::<BlakeTwo256, HostFunctions>(None)
}
