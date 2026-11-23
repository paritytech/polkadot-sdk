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

//! The work reports JAM has in flight for our service, read every tick as a monitor.

use crate::nodes::jam::LOG_TARGET;
use codec::{Decode, DecodeAll};
use jam_interface::{HeaderHash, JamStateSource, ServiceId, WorkPackageHash, WorkReport};
use sp_runtime::traits::Header as HeaderT;
use std::time::{Duration, Instant};

/// How long the non-blocking in-flight monitor's reads may take before a tick gives up on them.
///
/// Comfortably under a parachain slot: the monitor exists to log the pipeline, and a tick that
/// waited on it would have made it a dependency of authoring, which it explicitly is not.
const MONITOR_BUDGET: Duration = Duration::from_secs(2);

/// One work package JAM currently has in flight for our service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InFlightReport<Header: HeaderT> {
	pub(crate) wp_hash: WorkPackageHash,
	pub(crate) source: ReportSource,
	/// The parachain block the report's work digest says its package carries, read best-effort.
	/// `None` means the digest is in a shape this collator cannot read, which is never an error
	/// here — see [`decode_digest_head`].
	pub(crate) head: Option<Header::Hash>,
}

/// Read the parachain head out of a work digest's output, best-effort.
///
/// The work output belongs to the parachain service and its shape moves with the service — a newer
/// service appends the block's number to it — so only the prefix every shape shares is parsed: the
/// para id and the head data, which is the block's header hash.
///
/// Anything that does not parse comes back as `None`. This is a logging aid: it must tolerate an
/// unknown format, a failed refine's error output, and outright garbage, and it must never fail a
/// tick or feed a decision.
pub(crate) fn decode_digest_head<Hash: Decode>(output: &[u8]) -> Option<Hash> {
	let mut input = output;
	let _para_id = u32::decode(&mut input).ok()?;
	let head_data = Vec::<u8>::decode(&mut input).ok()?;
	// Every shape of the output carries the parent head hash behind the head data. Requiring it
	// stops a short byte string that happens to decode as a vector from being read as a head.
	if input.len() < 32 {
		return None;
	}
	Hash::decode_all(&mut &head_data[..]).ok()
}

/// Which of JAM's two in-flight state entries a report was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReportSource {
	/// `C(10)`: reported by guarantors, data still being made available.
	Availability,
	/// `C(14)`: available, queued for accumulation behind its dependencies.
	ReadyQueue,
}

/// The monitor read, bounded and infallible.
///
/// It can neither fail a tick nor hold one up past its own budget: whatever goes wrong is logged
/// and comes back as "nothing seen". That is the whole contract the builder gives the monitor —
/// building never blocks on JAM state, and the pipeline view is a debugging instrument, not an
/// input to any decision.
///
/// The read is taken at the cached JAM tip rather than at the package anchor: it belongs to no
/// package, so the freshest block is the right one to ask (and with `ANCHOR_OFFSET` at zero the
/// two are the same block anyway).
pub(crate) async fn monitor_in_flight<Header: HeaderT, Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	at: HeaderHash,
	service_id: ServiceId,
) -> Vec<InFlightReport<Header>> {
	let read = read_in_flight_reports::<Header, Jam>(jam, at, service_id);
	match tokio::time::timeout(MONITOR_BUDGET, read).await {
		Ok(Ok(reports)) => reports,
		Ok(Err(error)) => {
			tracing::warn!(
				target: LOG_TARGET,
				?at,
				error,
				"Unable to read JAM's in-flight reports; this tick sees nothing of the pipeline.",
			);
			Vec::new()
		},
		Err(_) => {
			tracing::warn!(
				target: LOG_TARGET,
				?at,
				budget_ms = MONITOR_BUDGET.as_millis(),
				"The in-flight monitor read did not finish within its budget; giving up on it \
				 for this tick rather than holding the tick up.",
			);
			Vec::new()
		},
	}
}

/// The work packages JAM has in flight for our service, from both places state keeps them.
///
/// The builder reads these for the log alone: with no links between packages, this is the only view
/// of the pipeline there is before accumulation, and it is what debugging and — later — recovery
/// consume. Nothing in a tick branches on the answer.
///
/// Filtering is by service id, a protocol-level field of a report's per-item digest.
async fn read_in_flight_reports<Header: HeaderT, Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	service_id: ServiceId,
) -> Result<Vec<InFlightReport<Header>>, String> {
	let started = Instant::now();
	let availability = jam
		.availability(anchor)
		.await
		.map_err(|error| format!("availability: {error}"))?;
	let availability_ms = started.elapsed().as_millis();
	let started = Instant::now();
	let ready = jam.ready_queue(anchor).await.map_err(|error| format!("readyQueue: {error}"))?;
	let ready_queue_ms = started.elapsed().as_millis();

	let mut reports = Vec::new();
	for assignment in availability.iter().flatten() {
		push_report(&mut reports, assignment.report(), ReportSource::Availability, service_id);
	}
	let from_availability = reports.len();
	for record in ready.iter().flatten() {
		push_report(&mut reports, &record.report, ReportSource::ReadyQueue, service_id);
	}

	// The level is the only thing that varies, and `tracing` needs it at the macro's call site.
	macro_rules! log_read {
		($message:literal) => {
			tracing::debug!(
				target: LOG_TARGET,
				method = "availability + readyQueue",
				at = ?anchor,
				service_id,
				cores = availability.len(),
				epoch_phases = ready.len(),
				from_availability,
				from_ready_queue = reports.len() - from_availability,
				unreadable_digests = reports.iter().filter(|report| report.head.is_none()).count(),
				?reports,
				availability_ms,
				ready_queue_ms,
				$message,
			)
		};
	}
	if reports.is_empty() {
		log_read!(
			"JAM read: nothing of ours is in flight — no work package for our service is \
		           in availability or in the ready queue at this anchor."
		);
	} else {
		log_read!("JAM read: the work packages in flight for our service.");
	}
	Ok(reports)
}

/// Keep a report if it refines something for our service and is not already listed.
///
/// The same package can sit in both state entries across a tick's two reads, and JAM keys them by
/// package hash, so the hash is what deduplicates. A report whose digest cannot be read is still
/// kept: the package is in flight either way, and that is the fact the monitor is there to show.
pub(crate) fn push_report<Header: HeaderT>(
	reports: &mut Vec<InFlightReport<Header>>,
	report: &WorkReport,
	source: ReportSource,
	service_id: ServiceId,
) {
	let Some(digest) = report.results.iter().find(|digest| digest.service == service_id) else {
		return;
	};
	let wp_hash = report.package_spec.hash;
	if reports.iter().any(|seen| seen.wp_hash == wp_hash) {
		return;
	}
	let head = digest
		.result
		.as_ref()
		.ok()
		.and_then(|output| decode_digest_head::<Header::Hash>(&output.0));
	reports.push(InFlightReport { wp_hash, source, head });
}
