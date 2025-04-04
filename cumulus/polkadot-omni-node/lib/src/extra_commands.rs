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

//! Optional/Additional CLI options of the omni-node-lib to be used by other binaries. See [`ExtraSubcommands`].
use crate::chain_spec::LoadSpec;
use clap::{ArgMatches, Args, Command, FromArgMatches};
use sc_cli::{ExportChainSpecCmd, Result as CliResult};

/// A trait for adding and executing extra CLI commands.
///
/// This enables binaries to opt-in to additional commands (like `export-chain-spec`)
/// without having to modify the main CLI parser. The trait can be composed using tuples:
///
/// Example:
///
/// run::<MyConfig, (ExportChainSpecCmd, MyOtherCmd)>(...)
///
/// See `polkadot-parachain/src/main.rs` for usage.
pub trait ExtraSubcommand: Sized {
    /// Augments the CLI definition with an additional subcommand.
    ///
    /// This allows the subcommand to be shown in `--help` and parsed by Clap.
    fn augment_command(cmd: Command) -> Command;

    /// Tries to run the subcommand by matching on the name and arguments.
    ///
    /// If this subcommand is not recognized, returns `None`.
    fn maybe_run(
        name: &str,
        matches: &ArgMatches,
        chain_spec_loader: &dyn LoadSpec,
    ) -> Option<CliResult<()>>;
}


/// A no-op subcommand handler, for runtimes without extra subcommands.
pub struct NoExtraSubcommand;

impl ExtraSubcommand for NoExtraSubcommand {
    fn augment_command(cmd: Command) -> Command {
        cmd
    }
    fn maybe_run(_name: &str, _matches: &ArgMatches, _disk_chain_spec_loader: &dyn LoadSpec) -> Option<sc_cli::Result<()>> {
        None
    }
}

/// Provides the `export-chain-spec` subcommand.
pub struct ExportChainSpec;

impl ExtraSubcommand for ExportChainSpec {
    fn augment_command(cmd: Command) -> Command {
        cmd.subcommand(
            ExportChainSpecCmd::augment_args_for_update(Command::new("export-chain-spec"))
        )
    }

    fn maybe_run(
        name: &str,
        matches: &ArgMatches,
        chain_spec_loader: &dyn LoadSpec,
    ) -> Option<sc_cli::Result<()>> {
        let binding = ExportChainSpecCmd::augment_args_for_update(Command::new(""));
        let expected_name = binding.get_name();

        if name != expected_name {
            return None;
        }

        let cmd = ExportChainSpecCmd::from_arg_matches(matches)
            .map_err(|e| sc_cli::Error::Cli(e.into()))
            .ok()?;

        let spec = chain_spec_loader
            .load_spec(&cmd.chain)
            .map_err(|e| sc_cli::Error::Application(Box::new(std::io::Error::other(e))))
            .ok()?;

        Some(cmd.run(spec))
    }
}
