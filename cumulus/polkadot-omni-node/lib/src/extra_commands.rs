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
use clap::{ArgMatches, Args, Command, FromArgMatches, Subcommand};
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

/// Enum representing all supported extra subcommands.
///
/// This is used internally to help dispatch logic and augment the CLI automatically
/// using the `clap` derive system
#[derive(Debug, Subcommand)]
pub enum ExtraCommands {
    /// Export the chain spec to JSON.
    #[command(name = "export-chain-spec")]
    ExportChainSpec(ExportChainSpecCmd),
}


/// No-op subcommand handler. Use this when a binary does not expose any extra subcommands.
///
/// Acts as the default `ExtraSubcommand` implementation when no extras are provided.
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
pub struct ExtraSubcommands;

impl ExtraSubcommand for ExtraSubcommands {
    fn augment_command(cmd: Command) -> Command {
        let base = cmd.version(env!("CARGO_PKG_VERSION"));
        ExtraCommands::augment_subcommands(base)
    }

    fn maybe_run(
        _name: &str,
        matches: &ArgMatches,
        chain_spec_loader: &dyn LoadSpec,
    ) -> Option<sc_cli::Result<()>> {
        match ExtraCommands::from_arg_matches(matches) {
            Ok(ExtraCommands::ExportChainSpec(cmd)) => {
                let spec = chain_spec_loader
                    .load_spec(&cmd.chain)
                    .map_err(|e| sc_cli::Error::Application(Box::new(std::io::Error::other(e))))
                    .ok()?;
                Some(cmd.run(spec))
            }
            Err(e) => {
                eprintln!("Error parsing subcommand: {e}");
                None
            }
        }
    }

}
