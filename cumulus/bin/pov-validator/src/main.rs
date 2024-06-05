// This file is part of Cumulus.

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

use clap::Parser;
use codec::{Decode, Encode};
use polkadot_node_primitives::{BlockData, PoV, POV_BOMB_LIMIT, VALIDATION_CODE_BOMB_LIMIT};
use polkadot_parachain_primitives::primitives::ValidationParams;
use polkadot_primitives::{BlockNumber as RBlockNumber, Hash as RHash, HeadData};
use sc_executor::WasmExecutor;
use sp_core::traits::{CallContext, CodeExecutor, RuntimeCode, WrappedRuntimeCode};
use std::{fs, path::PathBuf, time::Instant};
use tracing::level_filters::LevelFilter;

/// Tool for validating a `PoV` locally.
#[derive(Parser)]
struct Cli {
	/// The path to the validation code that should be used to validate the `PoV`.
	#[arg(long)]
	validation_code: PathBuf,

	/// The path to the `PoV` to validate.
	#[arg(long)]
	pov: PathBuf,
}

fn main() -> anyhow::Result<()> {
	let _ = tracing_subscriber::fmt()
		.with_env_filter(
			tracing_subscriber::EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into()),
		)
		.with_writer(std::io::stderr)
		.try_init();

	let cli = Cli::parse();

	let validation_code = fs::read(&cli.validation_code).map_err(|error| {
		tracing::error!(%error, path = %cli.validation_code.display(), "Failed to read validation code");
		anyhow::anyhow!("Failed to read validation code")
	})?;

	let validation_code =
		sp_maybe_compressed_blob::decompress(&validation_code, VALIDATION_CODE_BOMB_LIMIT)
			.map_err(|error| {
				tracing::error!(%error, "Failed to decompress validation code");
				anyhow::anyhow!("Failed to decompress validation code")
			})?;

	let pov_file = fs::read(&cli.pov).map_err(|error| {
		tracing::error!(%error, path = %cli.pov.display(), "Failed to read PoV");
		anyhow::anyhow!("Failed to read PoV")
	})?;

	let executor = WasmExecutor::<sp_io::SubstrateHostFunctions>::builder()
		.with_allow_missing_host_functions(true)
		.build();

	let runtime_code = RuntimeCode {
		code_fetcher: &WrappedRuntimeCode(validation_code.into()),
		heap_pages: None,
		// The hash is used for caching, which we need here, but we only use one wasm file. So, the
		// actual hash is not that important.
		hash: vec![1, 2, 3],
	};

	// We are calling `Core_version` to get the wasm file compiled. We don't care about the result.
	let _ = executor
		.call(
			&mut sp_io::TestExternalities::default().ext(),
			&runtime_code,
			"Core_version",
			&[],
			CallContext::Offchain,
		)
		.0;

	let pov_file_ptr = &mut &pov_file[..];
	let pov = PoV::decode(pov_file_ptr).map_err(|error| {
		tracing::error!(%error, "Failed to decode `PoV`");
		anyhow::anyhow!("Failed to decode `PoV`")
	})?;
	let head_data = HeadData::decode(pov_file_ptr).map_err(|error| {
		tracing::error!(%error, "Failed to `HeadData`");
		anyhow::anyhow!("Failed to decode `HeadData`")
	})?;
	let relay_parent_storage_root = RHash::decode(pov_file_ptr).map_err(|error| {
		tracing::error!(%error, "Failed to relay storage root");
		anyhow::anyhow!("Failed to decode relay storage root")
	})?;
	let relay_parent_number = RBlockNumber::decode(pov_file_ptr).map_err(|error| {
		tracing::error!(%error, "Failed to relay block number");
		anyhow::anyhow!("Failed to decode relay block number")
	})?;

	let pov = sp_maybe_compressed_blob::decompress(&pov.block_data.0, POV_BOMB_LIMIT).map_err(
		|error| {
			tracing::error!(%error, "Failed to decompress `PoV`");
			anyhow::anyhow!("Failed to decompress `PoV`")
		},
	)?;

	let validation_params = ValidationParams {
		relay_parent_number,
		relay_parent_storage_root,
		parent_head: head_data,
		block_data: BlockData(pov.into()),
	};

	tracing::info!("Starting validation");

	let start = Instant::now();

	let res = executor
		.call(
			&mut sp_io::TestExternalities::default().ext(),
			&runtime_code,
			"validate_block",
			&validation_params.encode(),
			CallContext::Offchain,
		)
		.0;

	let duration = start.elapsed();

	match res {
		Ok(_) => tracing::info!("Validation was successful"),
		Err(error) => tracing::error!(%error, "Validation failed"),
	}

	tracing::info!("Validation took {}ms", duration.as_millis());

	Ok(())
}
