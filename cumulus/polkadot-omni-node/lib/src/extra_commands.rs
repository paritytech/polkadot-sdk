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
use crate::cli::ExtraSubcommand;
use clap::ArgMatches;
use clap::Args;
use clap::{Command, FromArgMatches};
use sc_cli::ExportChainSpecCmd;


/// Running without any extra additional subcommands
pub struct NoExtraSubcommand;

impl ExtraSubcommand for NoExtraSubcommand {
    fn augment_command(cmd: Command) -> Command {
        cmd
    }
    fn try_run(_name: &str, _matches: &clap::ArgMatches, _disk_chain_spec_loader: &dyn LoadSpec) -> Option<sc_cli::Result<()>> {
        None
    }
}

/// Augmenting the CLI with additional ExportChainSpec Command
pub struct ExportChainSpec;

impl ExtraSubcommand for ExportChainSpec {
    fn augment_command(cmd: Command) -> Command {
        cmd.subcommand(ExportChainSpecCmd::augment_args_for_update(Command::new("export-chain-spec")))
    }

    fn try_run(
        name: &str,
        matches: &ArgMatches,
        chain_spec_loader: &dyn LoadSpec,
    ) -> Option<sc_cli::Result<()>> {
        if name != "export-chain-spec" {
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
