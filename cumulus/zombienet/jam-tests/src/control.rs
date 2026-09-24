// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The `parasim-tool` control lane: point a core at a para, or park it, mid-run.
//!
//! `parasim-tool` builds a work package per command and rides it through JAM's control lane. It
//! takes what a JAM node's RPC URL, the service id, the authorizer blob and the tool binary used
//! to be held by a running network; nothing here spawns or owns a process.

use crate::{genesis_build::polkavm_env, para::Para};
use anyhow::Context;
use std::{path::Path, process::Command};

/// Host the AURA authorizer blob in the bootstrap service too, for `parasim-tool`'s sake.
///
/// Only the dynamic-core tests need this. Their `assign-core` / `free-core` commands are work
/// packages `parasim-tool` builds with `auth_code_host: 0`, so a guarantor resolves the
/// authorizer code out of service 0 — which genesis cannot be asked to host a preimage for.
/// The collators name the parachain service instead and need none of it. Idempotent, and off
/// everything's critical path: it can go as soon as `parasim-tool` names `--service` there.
pub fn host_authorizer_for_control_packages(
	rpc_url: &str,
	service_id: u32,
	authorizer_blob: &Path,
	tool: &Path,
) -> anyhow::Result<()> {
	run_step(
		&format!("deploy-authorizer: {}", authorizer_blob.display()),
		parasim_tool(rpc_url, service_id, authorizer_blob, tool).arg("deploy-authorizer"),
	)
}

/// Point `core`'s authorizer queue at `para`'s AURA authorizer, carried by `via`.
///
/// `via` names another para whose core carries the command. It is `None` whenever `core` can
/// carry the command itself, which covers both of the cases the tests use: a core parked by
/// [`free_core`], which still runs this para's own authorizer code, and a core that was never
/// assigned to a para and so still holds the null authorizer genesis left on it.
///
/// Which lane the command travels is `parasim-tool`'s business, not this caller's: it reads
/// who holds the core's assigner privilege, and — for the control lane — whether the carrier
/// is parked or running the named para, checking either way that what it builds matches the
/// hash the carrier core actually holds. It returns only once the core's *pool* holds the new
/// authorizer, so afterwards the core really can carry the para's packages.
pub fn assign_core(
	rpc_url: &str,
	service_id: u32,
	authorizer_blob: &Path,
	tool: &Path,
	para: &Para,
	core: u32,
	via: Option<&Para>,
) -> anyhow::Result<()> {
	let names = para.collator_names()?;
	// With no carrier, `--via-core` is left off rather than filled in: the tool defaults it to
	// the core being assigned, which is exactly what is wanted, and that is not this para's
	// own core — the reassignment test assigns core 1 to a para sitting on core 0.
	let carrier = via.unwrap_or(para);
	let mut command = parasim_tool(rpc_url, service_id, authorizer_blob, tool);
	command
		.args(["--collators", &names])
		.args(["assign-core", &para.id.to_string(), &core.to_string()])
		.args(["--via-para", &carrier.id.to_string()])
		.args(["--via-collators", &carrier.collator_names()?]);
	if let Some(via) = via {
		command.args(["--via-core", &via.core.to_string()]);
	}
	run_step(
		&format!(
			"assign-core: para {} onto core {core} for {names}, carried on core {}",
			para.id,
			via.map_or(core, |via| via.core)
		),
		&mut command,
	)
}

/// Park `core`: keep the AURA authorizer on it under a config naming no para, so its pool
/// drains over the next few blocks and it stops carrying `para`'s work.
///
/// Only a core the parachain service was granted can be parked this way, and the command rides
/// the core itself: it is a control package under the AURA authorizer that is about to go
/// away, signed by `para`'s own collator set. Returns once the pool holds the parked
/// authorizer, which is the moment the drain of the old one starts being visible.
///
/// Parked is not unassigned. The core keeps the same authorizer code, so it keeps taking
/// control packages — which is what leaves [`assign_core`] able to put a para back on it
/// without a second core to carry the command.
pub fn free_core(
	rpc_url: &str,
	service_id: u32,
	authorizer_blob: &Path,
	tool: &Path,
	para: &Para,
	core: u32,
) -> anyhow::Result<()> {
	let names = para.collator_names()?;
	run_step(
		&format!("free-core: core {core}, carried under para {}'s authorizer", para.id),
		parasim_tool(rpc_url, service_id, authorizer_blob, tool)
			.args(["--collators", &names])
			.args(["free-core", &core.to_string()])
			.args(["--via-para", &para.id.to_string()]),
	)
}

/// A `parasim-tool` invocation carrying the arguments every phase-6 command needs.
///
/// `--authorizer-blob`, `--collators` and `--scheme` are what an AURA authorizer hash is built
/// from, so they have to be exactly what the para's collators are started with. A mismatch
/// installs a hash nobody will ever satisfy, and the only symptom is a core that authorizes
/// nothing.
///
/// `--scheme` is spelled out even though sr25519 is the tool's default: it is the parachain
/// template runtime's `AuraId`, and a default that moves in the other repo would silently
/// point every core here at the wrong verifier blob.
fn parasim_tool(rpc_url: &str, service_id: u32, authorizer_blob: &Path, tool: &Path) -> Command {
	let mut command = Command::new(tool);
	command
		.args(["--rpc", rpc_url])
		.args(["--service", &service_id.to_string()])
		.args(["--scheme", "sr25519"])
		.arg("--authorizer-blob")
		.arg(authorizer_blob)
		.envs(polkavm_env());
	command
}

/// Run one setup step, saying what it is about to submit and what came back.
///
/// Every step here changes JAM state through a work package, and a package JAM refuses leaves no
/// trace on the chain — so the command line, the exit status, the output and the wall clock are
/// all part of the record. Both tools read the state back themselves and exit non-zero when the
/// change did not land, which is why a bad status is fatal rather than a warning.
fn run_step(what: &str, command: &mut Command) -> anyhow::Result<()> {
	log::info!("{what}: running {command:?}");
	let started = std::time::Instant::now();
	let stdout = capture_step(what, command)?;
	log::info!("{what}: ok in {:?}\n{stdout}", started.elapsed());
	Ok(())
}

/// Run one step and hand back its stdout, for the reads whose value the caller parses.
///
/// Quieter than [`run_step`] because a state read happens on a poll loop: the caller logs the
/// value it extracted instead, and the full transcript stays at `debug`. A non-zero exit is still
/// fatal, and still carries the whole transcript.
fn capture_step(what: &str, command: &mut Command) -> anyhow::Result<String> {
	let started = std::time::Instant::now();
	let output = command.output().with_context(|| format!("{what}: running {command:?}"))?;
	let elapsed = started.elapsed();
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let stderr = String::from_utf8_lossy(&output.stderr);

	anyhow::ensure!(
		output.status.success(),
		"{what} failed ({}) after {elapsed:?}:\n{stdout}{stderr}",
		output.status,
	);
	if !stderr.trim().is_empty() {
		log::info!("{what}: stderr\n{stderr}");
	}
	log::debug!("{what}: ok in {elapsed:?}\n{stdout}");
	Ok(stdout)
}
