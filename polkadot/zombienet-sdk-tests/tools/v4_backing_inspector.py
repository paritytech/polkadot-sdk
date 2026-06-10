#!/usr/bin/env python3
"""
v4_backing_inspector.py — visualize the V4 collator-protocol → backing pipeline
across a zombienet-sdk run.

For a given zombie-* directory, parses the collator's `collator-*.log` and every
validator's `validator-N.log` and emits, for each candidate hash the collator
generated:

    * the slot it was generated on, and which `core_index` it targeted;
    * per validator: whether it was advertised, fetched, seconded, locally
      backed by the backing subsystem;
    * any membership-rejection reason (`SchedulingParentNotInScope`,
      `RelayParentOutOfScope`, …) and which leaf rejected it.

Outputs:
    * a markdown table on stdout (`--format=md`, default),
    * or an HTML page (`--format=html`) — same data, colour-coded matrix that
      makes per-core and per-validator gaps visually obvious.

Usage:

    polkadot/zombienet-sdk-tests/tools/v4_backing_inspector.py /tmp/zombie-XXX
    polkadot/zombienet-sdk-tests/tools/v4_backing_inspector.py /tmp/zombie-XXX --format=html > report.html

This is a debugging aid; the parsing is forgiving — log-format drift can lead to
empty cells rather than tracebacks.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from glob import glob


HASH_RE = r"0x[0-9a-f]+"


@dataclass
class CandidateRecord:
	hash: str
	core_index: int
	relay_parent: str
	# Monotonic sequence number assigned at insertion (the order the collator's
	# `Candidate generated` log produced this candidate). Used to display things
	# in chronological order within the same parablock.
	index: int = 0
	scheduling_parent: str | None = None
	# Parablock the candidate was built from — correlated from the producer-side
	# `Sending {WithBundle,ResubmitOnly} segment. … segment=[(N, 0xH), …]` log.
	parablock_number: int | None = None
	parablock_hash: str | None = None
	# Aura slot the parablock was authored in (`Claiming slot. … slot=Slot(S)` log
	# immediately preceding the `Pre-sealed` event).
	parablock_slot: int | None = None
	# True when this candidate came from a historical segment entry (a
	# resubmission of a previously-built parablock), False when it's the
	# fresh `WithBundle` bundle. None when not classified.
	is_resubmission: bool | None = None
	# Relay-block number for the candidate's `scheduling_parent`, when we can
	# resolve the hash via the collator log's relay-chain Pre-sealed events.
	scheduling_parent_number: int | None = None
	# validator_name -> bool
	advertised: dict[str, bool] = field(default_factory=dict)
	fetched: dict[str, bool] = field(default_factory=dict)
	seconded: dict[str, bool] = field(default_factory=dict)
	backed: dict[str, bool] = field(default_factory=dict)
	# validator_name -> list of rejection reasons (string + leaf)
	rejections: dict[str, list[str]] = field(default_factory=lambda: defaultdict(list))


def parse_collator_log(
	path: str,
) -> tuple[dict[str, CandidateRecord], dict[str, int]]:
	"""Correlate each `Candidate generated` event with the exact parablock the
	candidate carries, using the producer-side `Sending {WithBundle,ResubmitOnly}
	segment. core_index=X segment_len=K segment=[(N, 0xH), …]` log lines.

	Resets the per-core pending queue at each new send so stale entries (segment
	entries that failed to hydrate, etc.) don't shift subsequent mappings.

	Also returns a relay_block hash → block_number map gleaned from
	`[Relaychain] babe: 🔖 Pre-sealed block for proposal at N. Hash now 0xH`
	events the collator's relay-chain client observes, so we can resolve
	`scheduling_parent` numbers later.
	"""
	records: dict[str, CandidateRecord] = {}
	relay_block_map: dict[str, int] = {}
	if not os.path.exists(path):
		return records, relay_block_map

	# Parachain-side preseal (parablock hash → number) vs relay-chain-side preseal
	# (relay-block hash → number). We use the [Parachain] / [Relaychain] tags to
	# distinguish.
	presealed_pat = re.compile(
		rf"Pre-sealed block for proposal at (\d+)\. Hash now ({HASH_RE}), previously "
		rf"{HASH_RE}"
	)
	# `Claiming slot. … slot=Slot(S)` precedes each parablock build. We track the
	# most recent parachain slot so we can attach it to the next `Pre-sealed` event.
	claiming_slot_pat = re.compile(r"Claiming slot\..*\bslot=Slot\((\d+)\)")
	send_pat = re.compile(
		r"Sending (?:WithBundle|ResubmitOnly) segment\. core_index=CoreIndex\((\d+)\) "
		r"segment_len=\d+ segment=\[(.*?)\]"
	)
	segment_entry_pat = re.compile(rf"\((\d+), ({HASH_RE})\)")
	# `Adding entry to segment for core. core_index=X relay_parent=… block_numbers=[N]`
	# fires once per hydrated SegmentCollation (both historical entries and the
	# bundle), immediately before the corresponding `Candidate generated` event.
	# It's the most reliable 1:1 link between a parablock and a candidate hash.
	adding_pat = re.compile(
		r"Adding entry to segment for core\. core_index=CoreIndex\((\d+)\) "
		r"relay_parent=" + HASH_RE + r" block_numbers=\[([0-9, ]+)\]"
	)
	candidate_pat = re.compile(
		rf"Candidate generated candidate_hash=({HASH_RE}) "
		rf"pov_hash={HASH_RE} relay_parent=({HASH_RE}) "
		rf"para_id=\d+ core_index=CoreIndex\((\d+)\)"
	)

	last_para_presealed: tuple[int, str] | None = None
	last_para_slot: int | None = None
	# parablock_number -> Aura slot, populated as each Pre-sealed event fires.
	parablock_slot_map: dict[int, int] = {}
	# Per-core: latest "Sending segment" parablock_number -> parablock_hash map.
	# Used to look up the hash for an `Adding entry` block number.
	segment_hash_by_core: dict[int, dict[int, str]] = defaultdict(dict)
	# Per-core FIFO of pending entries — one per `Adding entry to segment for core`
	# event, consumed in order by the subsequent `Candidate generated` events for
	# that core. Multiple `Adding entry` lines accumulate before the burst of
	# `Candidate generated` lines, so a single-slot map would lose all but the last.
	# Each queued entry carries (parablock_number, parablock_hash, is_resub, emit_slot)
	# — `emit_slot` is `last_para_slot` at the moment the `Adding entry` event fired, so
	# resubmissions are reported under the paraslot in which they were re-advertised,
	# not the original build slot of the parablock.
	next_for_core: dict[int, list[tuple[int, str | None, bool, int | None]]] = defaultdict(
		list
	)

	with open(path) as fh:
		for line in fh:
			m = claiming_slot_pat.search(line)
			if m and "[Parachain]" in line:
				last_para_slot = int(m.group(1))
				continue

			m = presealed_pat.search(line)
			if m:
				num = int(m.group(1))
				h = m.group(2)
				if "[Parachain]" in line:
					last_para_presealed = (num, h)
					if last_para_slot is not None:
						parablock_slot_map[num] = last_para_slot
				elif "[Relaychain]" in line:
					relay_block_map[h] = num
				continue

			m = send_pat.search(line)
			if m:
				core = int(m.group(1))
				segment_hash_by_core[core] = {
					int(num): h for num, h in segment_entry_pat.findall(m.group(2))
				}
				continue

			m = adding_pat.search(line)
			if m:
				core = int(m.group(1))
				# Bundle entries may be multi-block (`block_numbers=[N1,N2,…]`); use the
				# highest number as the bundle's parablock (the tip of the bundle).
				numbers = [int(n.strip()) for n in m.group(2).split(",") if n.strip()]
				if not numbers:
					continue
				tip = max(numbers)
				# Hash lookup: prefer the segment map (historical entry); else fall back
				# to the most-recent parachain pre-sealed event (the freshly built bundle).
				h = segment_hash_by_core[core].get(tip)
				is_resub = h is not None
				if h is None and last_para_presealed is not None and last_para_presealed[0] == tip:
					h = last_para_presealed[1]
					is_resub = False
				next_for_core[core].append((tip, h, is_resub, last_para_slot))
				continue

			m = candidate_pat.search(line)
			if m:
				ch = m.group(1)
				core = int(m.group(3))
				rec = CandidateRecord(
					hash=ch,
					core_index=core,
					relay_parent=m.group(2),
					index=len(records),
				)
				if next_for_core[core]:
					n, ph, is_resub, emit_slot = next_for_core[core].pop(0)
					rec.parablock_number = n
					rec.parablock_hash = ph
					rec.is_resubmission = is_resub
					# Sort key: the paraslot in which this candidate was actually
					# generated. For initial bundles that's the build slot (also what
					# `parablock_slot_map` records); for resubmissions it's the later
					# slot at which the historical entry was re-advertised.
					rec.parablock_slot = emit_slot if emit_slot is not None else parablock_slot_map.get(n)
				records[ch] = rec
	return records, relay_block_map


def parse_validator_log(
	path: str,
	name: str,
	records: dict[str, CandidateRecord],
	relay_block_map: dict[str, int],
) -> None:
	if not os.path.exists(path):
		return
	with open(path) as fh:
		log = fh.read()

	# Each validator authors a subset of relay-chain blocks under the BABE target.
	# Combining the maps from all 6 validators gives a near-complete relay
	# hash → number lookup we can use to resolve candidate `scheduling_parent`s.
	for m in re.finditer(
		rf"babe: 🔖 Pre-sealed block for proposal at (\d+)\. Hash now ({HASH_RE}),",
		log,
	):
		relay_block_map.setdefault(m.group(2), int(m.group(1)))

	# Advertisement accepted lines carry `candidate_hash: 0x...`.
	for m in re.finditer(rf"Advertisement accepted .*candidate_hash: ({HASH_RE})", log):
		h = m.group(1)
		if h in records:
			records[h].advertised[name] = True

	# Collation fetch attempt succeeded — same Advertisement struct.
	for m in re.finditer(rf"Collation fetch attempt succeeded .*candidate_hash: ({HASH_RE})", log):
		h = m.group(1)
		if h in records:
			records[h].fetched[name] = True

	# Started seconding.
	for m in re.finditer(rf"Started seconding .*candidate_hash: ({HASH_RE})", log):
		h = m.group(1)
		if h in records:
			records[h].seconded[name] = True

	# Backing subsystem locally backed.
	for m in re.finditer(rf"Candidate backed candidate_hash=({HASH_RE})", log):
		h = m.group(1)
		if h in records:
			records[h].backed[name] = True

	# `Validate and second candidate candidate_hash=0xX candidate_receipt=...` carries the
	# full descriptor, including the candidate's `scheduling_parent`. We harvest the
	# (candidate_hash, scheduling_parent) pair so the report can show it.
	for m in re.finditer(
		rf"Validate and second candidate candidate_hash=({HASH_RE}).*?"
		rf"scheduling_parent: ({HASH_RE})",
		log,
	):
		h = m.group(1)
		sp = m.group(2)
		if h in records and records[h].scheduling_parent is None:
			records[h].scheduling_parent = sp

	# Prospective-parachains rejections.
	#   Per-leaf form: "Candidate is not a hypothetical member on: <err> ... candidate=0x..."
	#   Summary form:  "Candidate is not a hypothetical member on any of the active leaves ... candidate=0x..."
	for m in re.finditer(
		r"Candidate is not a hypothetical member on(?:: ([^p]+?))?.*candidate=("
		+ HASH_RE
		+ r")",
		log,
	):
		err = (m.group(1) or "any-of-leaves").strip()
		h = m.group(2)
		if h in records:
			records[h].rejections[name].append(err)


def render_markdown(records: dict[str, CandidateRecord], validators: list[str]) -> str:
	"""Emit a per-core summary and a per-candidate table."""
	out: list[str] = []

	# Per-core summary.
	out.append("## Per-core summary\n")
	per_core: dict[int, list[CandidateRecord]] = defaultdict(list)
	for r in records.values():
		per_core[r.core_index].append(r)

	headers = ["core", "candidates", "advertised (any)", "seconded (any)", "backed (any)"]
	out.append("| " + " | ".join(headers) + " |")
	out.append("| " + " | ".join(["---"] * len(headers)) + " |")
	for core in sorted(per_core):
		rs = per_core[core]
		adv = sum(1 for r in rs if r.advertised)
		sec = sum(1 for r in rs if r.seconded)
		bck = sum(1 for r in rs if r.backed)
		out.append(f"| {core} | {len(rs)} | {adv} | {sec} | {bck} |")

	# Per-validator-per-core summary.
	out.append("\n## Per validator × core counts\n")
	headers = ["validator", "core", "advertised", "fetched", "seconded", "backed", "rejected"]
	out.append("| " + " | ".join(headers) + " |")
	out.append("| " + " | ".join(["---"] * len(headers)) + " |")
	for core in sorted(per_core):
		rs = per_core[core]
		for v in validators:
			adv = sum(1 for r in rs if r.advertised.get(v))
			fet = sum(1 for r in rs if r.fetched.get(v))
			sec = sum(1 for r in rs if r.seconded.get(v))
			bck = sum(1 for r in rs if r.backed.get(v))
			rej = sum(1 for r in rs if v in r.rejections)
			out.append(f"| {v} | {core} | {adv} | {fet} | {sec} | {bck} | {rej} |")
		out.append("|" + " |" * len(headers))

	# Per-candidate detail — show ALL candidates, sorted by parablock then chronological,
	# with `backed` and `resub` columns side-by-side. Mark a parablock as "lost" only when
	# none of its candidates were backed by any validator.
	backed_parablocks: set[int] = {
		r.parablock_number
		for r in records.values()
		if r.backed and r.parablock_number is not None
	}
	all_records = sorted(
		records.values(),
		key=lambda r: (r.parablock_slot or 0, r.parablock_number or 0, r.index),
	)
	lost_parablocks = {
		r.parablock_number
		for r in records.values()
		if r.parablock_number is not None and r.parablock_number not in backed_parablocks
	}
	out.append(
		f"\n## All candidates ({len(records)})  —  "
		f"lost parablocks: {len(lost_parablocks)}\n"
	)
	out.append(
		"`resub` = built from a historical segment entry (resubmission) vs fresh bundle. "
		"`backed_by` = validators whose backing subsystem reported "
		"`Candidate backed` for this candidate hash. `parablock_lost` = `*` when no "
		"candidate hash for this parablock was backed by anyone (the parablock is "
		"truly lost; otherwise some other candidate for the same parablock won). "
		"`sched_parent` = the candidate's V3 `scheduling_parent` (with relay-block "
		"number when resolvable). `rejected_on` = validators that emitted a "
		"`Candidate is not a hypothetical member` log for this candidate.\n"
	)
	headers = [
		"paraslot",
		"parablock",
		"core",
		"resub",
		"backed_by",
		"parablock_lost",
		"candidate",
		"sched_parent",
		"advertised",
		"seconded",
		"rejected_on",
	]
	out.append("| " + " | ".join(headers) + " |")
	out.append("| " + " | ".join(["---"] * len(headers)) + " |")
	for r in all_records:
		adv = ",".join(sorted(r.advertised.keys())) or "-"
		sec = ",".join(sorted(r.seconded.keys())) or "-"
		backed = ",".join(sorted(r.backed.keys())) or "-"
		reasons: list[str] = []
		for v, rs in r.rejections.items():
			tag = sorted(set(x.split()[0] for x in rs))
			reasons.append(f"{v}:{','.join(tag)}")
		rej = "; ".join(reasons) or "-"
		para = (
			f"#{r.parablock_number} `{r.parablock_hash[:14]}…`"
			if r.parablock_hash and r.parablock_number is not None
			else "?"
		)
		sp = (
			f"#{r.scheduling_parent_number} `{r.scheduling_parent[:14]}…`"
			if r.scheduling_parent and r.scheduling_parent_number is not None
			else (f"`{r.scheduling_parent[:14]}…`" if r.scheduling_parent else "?")
		)
		resub = (
			"yes" if r.is_resubmission else ("no" if r.is_resubmission is False else "?")
		)
		para_lost = (
			"*"
			if r.parablock_number is not None
			and r.parablock_number in lost_parablocks
			else ""
		)
		slot = str(r.parablock_slot) if r.parablock_slot is not None else "?"
		out.append(
			f"| {slot} | {para} | {r.core_index} | {resub} | {backed} | {para_lost} | "
			f"`{r.hash[:14]}…` | {sp} | {adv} | {sec} | {rej} |"
		)

	return "\n".join(out) + "\n"


def render_html(records: dict[str, CandidateRecord], validators: list[str]) -> str:
	"""Compact HTML grid: rows = candidates, columns = validators × {adv, sec, bck}."""

	def cell(ok: bool, rej: bool) -> str:
		if ok:
			return '<td class="ok">●</td>'
		if rej:
			return '<td class="rej">×</td>'
		return '<td class="gap">·</td>'

	rows: list[str] = []
	for r in sorted(
		records.values(),
		key=lambda r: (r.parablock_slot or 0, r.parablock_number or 0, r.index),
	):
		cells: list[str] = []
		for v in validators:
			cells.append(cell(r.advertised.get(v, False), False))
			cells.append(cell(r.seconded.get(v, False), False))
			cells.append(cell(r.backed.get(v, False), v in r.rejections))
		any_backed = bool(r.backed)
		row_class = "row-backed" if any_backed else "row-lost"
		para_block = (
			f"#{r.parablock_number}"
			if r.parablock_number is not None
			else "?"
		)
		para_hash = (
			f'<span class="hash">{r.parablock_hash[:14]}…</span>'
			if r.parablock_hash
			else ""
		)
		resub_cell = (
			"<td>R</td>"
			if r.is_resubmission
			else ("<td>F</td>" if r.is_resubmission is False else "<td>?</td>")
		)
		if r.scheduling_parent:
			sp_text = (
				f"#{r.scheduling_parent_number} {r.scheduling_parent[:10]}…"
				if r.scheduling_parent_number is not None
				else f"{r.scheduling_parent[:10]}…"
			)
		else:
			sp_text = "?"
		slot_cell = f"<td>{r.parablock_slot}</td>" if r.parablock_slot is not None else "<td>?</td>"
		rows.append(
			f'<tr class="{row_class}">'
			+ slot_cell
			+ f"<td>{r.core_index}</td>"
			+ resub_cell
			+ f"<td>{para_block}</td>"
			+ f"<td>{para_hash}</td>"
			+ f'<td class="hash">{sp_text}</td>'
			+ f'<td class="hash">{r.hash[:14]}…</td>'
			+ "".join(cells)
			+ f"<td>{len(r.rejections)}</td>"
			+ "</tr>"
		)

	header_cells = [
		f'<th colspan="3">{v.replace("validator-", "v")}</th>' for v in validators
	]
	sub_header = "".join("<th>A</th><th>S</th><th>B</th>" for _ in validators)

	return f"""<!doctype html>
<html><head><meta charset="utf-8"><title>V4 backing inspector</title>
<style>
 body {{ font-family: -apple-system, system-ui, sans-serif; margin: 24px; }}
 table {{ border-collapse: collapse; font-size: 11px; }}
 th, td {{ border: 1px solid #ddd; padding: 2px 6px; text-align: center; }}
 .hash {{ font-family: ui-monospace, monospace; text-align: left; }}
 td.ok {{ background: #c6efce; color: #006100; font-weight: bold; }}
 td.rej {{ background: #ffc7ce; color: #9c0006; font-weight: bold; }}
 td.gap {{ background: #f7f7f7; color: #999; }}
 tr.row-lost {{ background: #fff4f4; }}
 tr.row-backed {{ background: #f4fff4; }}
 .legend {{ margin: 8px 0; font-size: 12px; color: #444; }}
</style></head><body>
<h2>V4 backing inspector — {len(records)} candidates × {len(validators)} validators</h2>
<div class="legend">A = advertised, S = seconded, B = backed (per validator). ● = yes,
× = explicit rejection reason logged on that validator, · = absent. Rows with no
green B column anywhere = lost. <b>resub</b>: R = built from a historical
segment entry (a resubmission), F = freshly built bundle, ? = not classified.
<b>sched_parent</b> = the candidate's V3 `scheduling_parent` (and its relay-block
number when resolvable from the collator's view). <b>rejected on</b> = count of
validators that emitted a `Candidate is not a hypothetical member` log for this
candidate.</div>
<table>
<thead>
<tr><th>slot</th><th>core</th><th>resub</th><th>para #</th><th>para hash</th><th>sched parent</th><th>candidate</th>{''.join(header_cells)}<th>rejected on</th></tr>
<tr><th></th><th></th><th></th><th></th><th></th><th></th><th></th>{sub_header}<th>#</th></tr>
</thead>
<tbody>
{"".join(rows)}
</tbody>
</table>
</body></html>
"""


def main() -> int:
	ap = argparse.ArgumentParser(description="V4 backing-pipeline inspector for zombienet-sdk runs.")
	ap.add_argument("run_dir", help="Path to a /tmp/zombie-* directory.")
	ap.add_argument(
		"--format",
		choices=("md", "html"),
		default="md",
		help="Output format (default: md).",
	)
	args = ap.parse_args()

	if not os.path.isdir(args.run_dir):
		print(f"Not a directory: {args.run_dir}", file=sys.stderr)
		return 2

	collator_logs = glob(os.path.join(args.run_dir, "collator-*/collator-*.log"))
	if not collator_logs:
		print(f"No collator log under {args.run_dir}/collator-*/", file=sys.stderr)
		return 2

	records: dict[str, CandidateRecord] = {}
	relay_block_map: dict[str, int] = {}
	for path in collator_logs:
		rec, m = parse_collator_log(path)
		records.update(rec)
		relay_block_map.update(m)

	validator_paths = sorted(glob(os.path.join(args.run_dir, "validator-*/validator-*.log")))
	validators = [os.path.splitext(os.path.basename(p))[0] for p in validator_paths]
	for path, name in zip(validator_paths, validators):
		parse_validator_log(path, name, records, relay_block_map)

	# Resolve scheduling_parent hashes to block numbers using the relay-chain
	# Pre-sealed events the collator's relay-chain client observed.
	for r in records.values():
		if r.scheduling_parent and r.scheduling_parent in relay_block_map:
			r.scheduling_parent_number = relay_block_map[r.scheduling_parent]

	if args.format == "html":
		print(render_html(records, validators))
	else:
		print(render_markdown(records, validators))

	return 0


if __name__ == "__main__":
	sys.exit(main())
