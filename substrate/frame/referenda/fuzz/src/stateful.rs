use crate::mock::*;
use frame_support::{dispatch::RawOrigin, traits::Get};
use pallet_conviction_voting::{AccountVote, Conviction, Vote};
use pallet_referenda::{
	Config, DecidingCount, ReferendumCount, ReferendumInfo, ReferendumInfoFor, TrackQueue,
	TracksInfo,
};
use rand::{rngs::StdRng, Rng};

pub struct FuzzState {
	pub block: u64,
	pub ref_count: u32,
	pub track0_deciding: u32,
	pub track1_deciding: u32,
	pub track0_queue_len: u32,
	pub track1_queue_len: u32,
	pub track0_max_deciding: u32,
	pub track1_max_deciding: u32,
	pub max_queued: u32,
}

pub fn snapshot_state() -> FuzzState {
	let tracks: Vec<_> = <Test as Config<()>>::Tracks::tracks().collect();
	let t0 = &tracks[0];
	let t1 = &tracks[1];
	FuzzState {
		block: frame_system::Pallet::<Test>::block_number(),
		ref_count: ReferendumCount::<Test>::get(),
		track0_deciding: DecidingCount::<Test>::get(t0.id),
		track1_deciding: DecidingCount::<Test>::get(t1.id),
		track0_queue_len: TrackQueue::<Test>::get(t0.id).len() as u32,
		track1_queue_len: TrackQueue::<Test>::get(t1.id).len() as u32,
		track0_max_deciding: t0.info.max_deciding,
		track1_max_deciding: t1.info.max_deciding,
		max_queued: <Test as Config>::MaxQueued::get(),
	}
}

fn is_ongoing(idx: u32) -> bool {
	matches!(ReferendumInfoFor::<Test>::get(idx), Some(ReferendumInfo::Ongoing(_)))
}

fn random_conviction(rng: &mut StdRng) -> Conviction {
	match rng.gen_range(0u8..7) {
		0 => Conviction::None,
		1 => Conviction::Locked1x,
		2 => Conviction::Locked2x,
		3 => Conviction::Locked3x,
		4 => Conviction::Locked4x,
		5 => Conviction::Locked5x,
		_ => Conviction::Locked6x,
	}
}

#[derive(Debug)]
pub enum Command {
	Submit { who: u64 },
	PlaceDeposit { who: u64, index: u32 },
	Vote { who: u64, index: u32, aye: bool, conviction: Conviction, balance: u64 },
	RemoveVote { who: u64, index: u32 },
	Delegate { who: u64, to: u64, track: u8, conviction: Conviction, balance: u64 },
	Undelegate { who: u64, track: u8 },
	Cancel { index: u32 },
	Kill { index: u32 },
	RefundDecisionDeposit { index: u32 },
	RefundSubmissionDeposit { index: u32 },
	AdvanceBlocks { n: u64 },
}

pub fn command_label(cmd: &Command) -> &'static str {
	match cmd {
		Command::Submit { .. } => "submit",
		Command::PlaceDeposit { .. } => "place_deposit",
		Command::Vote { .. } => "vote",
		Command::RemoveVote { .. } => "remove_vote",
		Command::Delegate { .. } => "delegate",
		Command::Undelegate { .. } => "undelegate",
		Command::Cancel { .. } => "cancel",
		Command::Kill { .. } => "kill",
		Command::RefundDecisionDeposit { .. } => "refund_decision_dep",
		Command::RefundSubmissionDeposit { .. } => "refund_submission_dep",
		Command::AdvanceBlocks { .. } => "advance_blocks",
	}
}

pub fn format_command(cmd: &Command) -> String {
	match cmd {
		Command::Submit { who } => format!("Submit(acct={})", who),
		Command::PlaceDeposit { who, index } => format!("PlaceDeposit(acct={}, ref={})", who, index),
		Command::Vote { who, index, aye, conviction, balance } =>
			format!("Vote(acct={}, ref={}, aye={}, {:?}, {})", who, index, aye, conviction, balance),
		Command::RemoveVote { who, index } => format!("RemoveVote(acct={}, ref={})", who, index),
		Command::Delegate { who, to, track, .. } => format!("Delegate(acct={}, to={}, t={})", who, to, track),
		Command::Undelegate { who, track } => format!("Undelegate(acct={}, t={})", who, track),
		Command::Cancel { index } => format!("Cancel(ref={})", index),
		Command::Kill { index } => format!("Kill(ref={})", index),
		Command::RefundDecisionDeposit { index } => format!("RefundDecDep(ref={})", index),
		Command::RefundSubmissionDeposit { index } => format!("RefundSubDep(ref={})", index),
		Command::AdvanceBlocks { n } => format!("AdvanceBlocks({})", n),
	}
}

pub fn gen_command(rng: &mut StdRng, s: &FuzzState) -> Command {
	type GenFn = fn(&mut StdRng, &FuzzState) -> Command;
	let mut candidates: Vec<(u32, GenFn)> = Vec::new();

	let any_track_full = s.track0_deciding >= s.track0_max_deciding
		|| s.track1_deciding >= s.track1_max_deciding;
	let any_queue_pressure = s.track0_queue_len + 2 >= s.max_queued
		|| s.track1_queue_len + 2 >= s.max_queued;

	let submit_w = match (any_track_full, any_queue_pressure) {
		(true, true) => 25,
		(true, false) => 15,
		_ => 10,
	};
	candidates.push((submit_w, |rng, _s| {
		Command::Submit { who: rng.gen_range(1..=N_ACCOUNTS) }
	}));

	if s.ref_count > 0 {
		let dep_w = if any_track_full { 30 } else { 10 };
		candidates.push((dep_w, |rng, s| {
			let lo = if s.ref_count > 5 { s.ref_count - s.ref_count / 5 } else { 0 };
			let index = rng.gen_range(lo..s.ref_count);
			Command::PlaceDeposit { who: rng.gen_range(1..=N_ACCOUNTS), index }
		}));
	}

	if s.ref_count > 0 {
		candidates.push((15, |rng, s| {
			let lo = if s.ref_count > 5 { s.ref_count - s.ref_count / 5 } else { 0 };
			let index = rng.gen_range(lo..s.ref_count);
			Command::Vote {
				who: rng.gen_range(1..=N_ACCOUNTS),
				index,
				aye: rng.gen_bool(0.6),
				conviction: random_conviction(rng),
				balance: rng.gen_range(1..=100),
			}
		}));
	}

	if s.ref_count > 0 {
		candidates.push((5, |rng, s| {
			Command::RemoveVote { who: rng.gen_range(1..=N_ACCOUNTS), index: rng.gen_range(0..s.ref_count) }
		}));
	}

	candidates.push((3, |rng, _s| {
		let who = rng.gen_range(1..=N_ACCOUNTS);
		let mut to = rng.gen_range(1..=N_ACCOUNTS);
		while to == who { to = rng.gen_range(1..=N_ACCOUNTS); }
		Command::Delegate {
			who, to,
			track: if rng.gen_bool(0.5) { 0 } else { 1 },
			conviction: random_conviction(rng),
			balance: rng.gen_range(1..=100),
		}
	}));

	candidates.push((2, |rng, _s| {
		Command::Undelegate { who: rng.gen_range(1..=N_ACCOUNTS), track: if rng.gen_bool(0.5) { 0 } else { 1 } }
	}));

	if s.ref_count > 0 {
		candidates.push((3, |rng, s| {
			Command::Cancel { index: rng.gen_range(0..s.ref_count) }
		}));
	}

	if s.ref_count > 0 {
		candidates.push((3, |rng, s| {
			Command::Kill { index: rng.gen_range(0..s.ref_count) }
		}));
	}

	if s.ref_count > 0 {
		candidates.push((3, |rng, s| {
			let index = rng.gen_range(0..s.ref_count);
			if rng.gen_bool(0.5) {
				Command::RefundDecisionDeposit { index }
			} else {
				Command::RefundSubmissionDeposit { index }
			}
		}));
	}

	let adv_w = if any_track_full { 3 } else { 8 };
	candidates.push((adv_w, |rng, _s| {
		Command::AdvanceBlocks { n: rng.gen_range(1..6) }
	}));

	let total: u32 = candidates.iter().map(|(w, _)| *w).sum();
	let mut pick = rng.gen_range(0..total);
	for (w, f) in &candidates {
		if pick < *w { return f(rng, s); }
		pick -= w;
	}
	(candidates[0].1)(rng, s)
}

pub fn execute_command(cmd: &Command) -> &'static str {
	match cmd {
		Command::Submit { who } => {
			match Referenda::submit(
				RuntimeOrigin::signed(*who),
				Box::new(RawOrigin::Root.into()),
				set_balance_proposal_bounded(1),
				frame_support::traits::schedule::DispatchTime::After(0),
			) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::PlaceDeposit { who, index } => {
			if !is_ongoing(*index) { return "SKIP"; }
			match Referenda::place_decision_deposit(RuntimeOrigin::signed(*who), *index) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::Vote { who, index, aye, conviction, balance } => {
			if !is_ongoing(*index) { return "SKIP"; }
			match ConvictionVoting::vote(
				RuntimeOrigin::signed(*who),
				*index,
				AccountVote::Standard {
					vote: Vote { aye: *aye, conviction: *conviction },
					balance: *balance,
				},
			) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::RemoveVote { who, index } => {
			if !is_ongoing(*index) { return "SKIP"; }
			match ConvictionVoting::remove_vote(RuntimeOrigin::signed(*who), Some(0), *index) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::Delegate { who, to, track, conviction, balance } => {
			match ConvictionVoting::delegate(
				RuntimeOrigin::signed(*who), *track, *to, *conviction, *balance,
			) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::Undelegate { who, track } => {
			match ConvictionVoting::undelegate(RuntimeOrigin::signed(*who), *track) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::Cancel { index } => {
			if !is_ongoing(*index) { return "SKIP"; }
			match Referenda::cancel(RuntimeOrigin::signed(4), *index) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::Kill { index } => {
			if !is_ongoing(*index) { return "SKIP"; }
			match Referenda::kill(RuntimeOrigin::root(), *index) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::RefundDecisionDeposit { index } => {
			match Referenda::refund_decision_deposit(RuntimeOrigin::signed(1), *index) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::RefundSubmissionDeposit { index } => {
			match Referenda::refund_submission_deposit(RuntimeOrigin::signed(1), *index) {
				Ok(_) => "OK",
				Err(_) => "ERR",
			}
		},
		Command::AdvanceBlocks { n } => {
			for _ in 0..*n { next_block(); }
			"OK"
		},
	}
}
