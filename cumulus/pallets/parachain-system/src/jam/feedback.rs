// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Reading the parachain's own feedback out of the Parachain Service (spec §5.1).
//!
//! The service records, per parachain, why its work packages were rejected and what its upward
//! messages did: `parachain_log[para_id]`, a bounded list of `(timeslot, LogEntry)`. On JAM it
//! is *the only* channel through which a parachain learns any of that — the relay path's events
//! do not exist — so a runtime that emits upward messages has to read it back to find out
//! whether they were applied.
//!
//! Reading it is a JAM state read like any other: it goes through the proof carried in the
//! block's additional data, verified against the anchor state root. The read is therefore only
//! possible during block execution (`set_validation_data`), which is where the upgrade path
//! already reads `ParaInfo` from.
//!
//! The log is **lossy by design** (§5.1): entries below a candidate's lookup-anchor are pruned
//! and the list is evicted under capacity pressure, rank-0 (refine) entries first. A runtime
//! must read it promptly and must not treat it as a durable record.

use alloc::vec::Vec;
use codec::Decode;
use parachain_service_core::{log::AccumulateLog, LogEntry, ParachainLog};

/// Decode a `parachain_log` value, requiring the whole slice to be consumed.
///
/// Split out from the state read so the ABI decode is testable without a JAM host: the bytes the
/// service stores under `para_log_key` are exactly what this takes.
pub fn decode_parachain_log(raw: &[u8]) -> Option<ParachainLog> {
	let mut slice = raw;
	let decoded = <ParachainLog as Decode>::decode(&mut slice).ok()?;
	// A trailing byte means the value is not what we think it is; the service writes only this
	// type under this key, so refusing is the honest answer.
	slice.is_empty().then_some(decoded)
}

/// The `AccumulateLog` events of the newest accumulate batch in `log`.
pub fn latest_accumulate_events(log: &ParachainLog) -> Option<Vec<AccumulateLog>> {
	parachain_service_core::latest_accumulate(log).map(<[AccumulateLog]>::to_vec)
}

/// Whether `log` carries `event` in any accumulate batch.
pub fn contains_accumulate_event(log: &ParachainLog, event: &AccumulateLog) -> bool {
	parachain_service_core::contains_accumulate(log, event)
}

/// Every core the service reported as `CoreNotAssignable`, in the order first seen.
///
/// This is the rejection a coretime chain retries: the service could not apply an assignment
/// because it is no longer the core's assigner. Nothing in JAM state names the message that was
/// rejected, so the retry is rebuilt from the chain's own record of what it last assigned.
pub fn rejected_cores(log: &ParachainLog) -> Vec<u16> {
	let mut cores = Vec::new();
	for (_, entry) in log.iter() {
		let LogEntry::Accumulate { entries } = entry else { continue };
		for event in entries {
			if let AccumulateLog::CoreNotAssignable { core } = event {
				if !cores.contains(core) {
					cores.push(*core);
				}
			}
		}
	}
	cores
}

/// Read and decode this parachain's `parachain_log` from the carried JAM state proof.
///
/// `None` when the service holds no log for the para — which is also what a `ParaInfo` that the
/// service has never written looks like, so a caller must not read it as "no failures".
#[cfg(jam)]
pub fn read_parachain_log<T: crate::Config>() -> Option<ParachainLog> {
	use frame_support::traits::Get;

	let para_id = parachain_service_core::types::ParaId::from(u32::from(T::SelfParaId::get()));
	let key = parachain_service_core::service_value_state_key(
		parachain_service_core::PARACHAIN_SERVICE_ID,
		&parachain_service_core::para_log_key(para_id),
	);
	cumulus_jam_state_reader::jam_state::jam_state_read(key)
		.and_then(|raw| decode_parachain_log(&raw))
}

/// The `AccumulateLog` events of the newest accumulate batch in this parachain's log.
///
/// This is what a runtime reacts to: the newest batch is the service's answer about the newest
/// work package. `None` when the log holds no accumulate entry at all.
#[cfg(jam)]
pub fn latest_feedback<T: crate::Config>() -> Option<Vec<AccumulateLog>> {
	read_parachain_log::<T>().and_then(|log| latest_accumulate_events(&log))
}

/// The cores the service reported as not assignable, read from this parachain's log.
#[cfg(jam)]
pub fn rejected_cores_now<T: crate::Config>() -> Vec<u16> {
	read_parachain_log::<T>().map(|log| rejected_cores(&log)).unwrap_or_default()
}

impl<T: crate::Config> crate::Pallet<T> {
	/// The cores the service reported as `CoreNotAssignable` in this parachain's log.
	///
	/// The reaction a coretime chain hangs off this: the service could not apply one of its
	/// assignments, and the retry is rebuilt from the chain's own record of what it assigned.
	/// Empty when the log is absent, unreadable or clean — a caller must not read the empty case
	/// as proof of anything, only the named cores as proof of rejection.
	#[cfg(jam)]
	pub fn jam_rejected_cores() -> Vec<u16> {
		rejected_cores_now::<T>()
	}

	/// The newest accumulate batch of this parachain's log, as the service recorded it.
	#[cfg(jam)]
	pub fn jam_latest_feedback() -> Option<Vec<AccumulateLog>> {
		latest_feedback::<T>()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parachain_service_core::{log::InsufficientBalanceReason, LogEntry, RefineLog};

	fn encode(log: &ParachainLog) -> Vec<u8> {
		use codec::Encode;
		log.encode()
	}

	#[test]
	fn a_clean_log_round_trips_works() {
		let log: ParachainLog = vec![
			(
				7,
				LogEntry::Accumulate {
					entries: vec![AccumulateLog::InsufficientStateBalance {
						reason: InsufficientBalanceReason::SetKV { key_hash: [0x11; 32] },
					}],
				},
			),
			(
				8,
				LogEntry::Accumulate {
					entries: vec![AccumulateLog::CoreNotAssignable { core: 3 }],
				},
			),
		];
		let decoded = decode_parachain_log(&encode(&log)).expect("decodes");
		assert_eq!(decoded, log);
	}

	#[test]
	fn trailing_bytes_are_refused_errors() {
		let log: ParachainLog = vec![(1, LogEntry::Accumulate { entries: vec![] })];
		let mut bytes = encode(&log);
		bytes.push(0);
		assert_eq!(decode_parachain_log(&bytes), None);
	}

	#[test]
	fn garbage_is_refused_errors() {
		assert_eq!(decode_parachain_log(&[0xff; 8]), None);
	}

	#[test]
	fn the_newest_accumulate_batch_is_the_one_returned_works() {
		let log: ParachainLog = vec![
			(
				1,
				LogEntry::Refine {
					error: RefineLog::EmptyKVKeyOrValue,
					auth_trace: Default::default(),
				},
			),
			(2, LogEntry::Accumulate { entries: vec![AccumulateLog::TooMuchStateHeld] }),
			(
				3,
				LogEntry::Accumulate {
					entries: vec![AccumulateLog::CoreNotAssignable { core: 9 }],
				},
			),
		];
		assert_eq!(
			latest_accumulate_events(&log),
			Some(vec![AccumulateLog::CoreNotAssignable { core: 9 }]),
		);
	}

	#[test]
	fn no_accumulate_batch_yields_nothing_works() {
		let log: ParachainLog = vec![(
			1,
			LogEntry::Refine {
				error: RefineLog::EmptyKVKeyOrValue,
				auth_trace: Default::default(),
			},
		)];
		assert_eq!(latest_accumulate_events(&log), None);
	}

	#[test]
	fn rejected_cores_are_collected_once_in_order_works() {
		let log: ParachainLog = vec![
			(
				1,
				LogEntry::Accumulate {
					entries: vec![AccumulateLog::CoreNotAssignable { core: 2 }],
				},
			),
			(
				2,
				LogEntry::Accumulate {
					entries: vec![
						AccumulateLog::CoreNotAssignable { core: 5 },
						AccumulateLog::TooMuchStateHeld,
						AccumulateLog::CoreNotAssignable { core: 2 },
					],
				},
			),
		];
		assert_eq!(rejected_cores(&log), vec![2, 5]);
	}

	#[test]
	fn contains_finds_events_in_any_batch_works() {
		let log: ParachainLog = vec![
			(1, LogEntry::Accumulate { entries: vec![AccumulateLog::TooMuchStateHeld] }),
			(
				2,
				LogEntry::Accumulate {
					entries: vec![AccumulateLog::CoreNotAssignable { core: 1 }],
				},
			),
		];
		assert!(contains_accumulate_event(&log, &AccumulateLog::TooMuchStateHeld));
		assert!(contains_accumulate_event(&log, &AccumulateLog::CoreNotAssignable { core: 1 }));
		assert!(!contains_accumulate_event(&log, &AccumulateLog::CoreNotAssignable { core: 2 }));
	}

	#[test]
	fn a_refine_only_log_has_no_feedback_works() {
		let log: ParachainLog = vec![(
			4,
			LogEntry::Refine {
				error: RefineLog::ValidationCodeLookupFailed,
				auth_trace: Default::default(),
			},
		)];
		assert_eq!(latest_accumulate_events(&log), None);
		assert!(rejected_cores(&log).is_empty());
	}
}
