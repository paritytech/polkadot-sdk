#!/usr/bin/env python3
"""Parse nextest JUnit XML files and produce a test summary with flaky/failed details.

Artifacts are expected under <dir>/<artifact-name>/junit.xml where the artifact
name encodes the job (e.g. junit-stable-0, junit-no-try-runtime-1, junit-benchmarks).
The report is grouped by job. Output goes to stdout.

An optional second argument points at a JSON file produced by
`gh api .../jobs --jq '[.jobs[] | {runner_name, name, html_url}]'`. When present,
each table row links the shard cell to the specific GitHub Actions job page, using
the `runner-name.txt` file stored next to `junit.xml` inside each artifact.
"""

import glob
import json
import os
import re
import sys
import xml.etree.ElementTree as ET

# Map artifact-name prefix to a human-readable job heading.
JOB_LABELS = {
    "junit-stable": "test-linux-stable",
    "junit-no-try-runtime": "test-linux-stable-no-try-runtime",
    "junit-benchmarks": "test-linux-stable-runtime-benchmarks",
}


def artifact_dir_and_name(path):
    """Return (artifact_dir, artifact_name) for a junit.xml path."""
    norm = path.replace("\\", "/")
    parts = norm.split("/")
    for i, part in enumerate(parts):
        for prefix in JOB_LABELS:
            if part == prefix or part.startswith(prefix + "-"):
                return "/".join(parts[: i + 1]), part
    # Fallback: parent directory
    parent = os.path.dirname(norm)
    return parent, os.path.basename(parent)


def job_label(artifact_name):
    for prefix, label in JOB_LABELS.items():
        if artifact_name == prefix or artifact_name.startswith(prefix + "-"):
            return label
    return artifact_name


def load_jobs(path):
    """Index job metadata by runner_name."""
    if not path or not os.path.exists(path):
        return {}
    try:
        with open(path) as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError):
        return {}
    return {j["runner_name"]: j for j in data if j.get("runner_name")}


def read_runner_name(dir_path):
    p = os.path.join(dir_path, "runner-name.txt")
    if not os.path.isfile(p):
        return None
    try:
        with open(p) as f:
            return f.read().strip() or None
    except OSError:
        return None


def shard_label_for(artifact_name, job_name):
    """Short label for a shard. Prefers matrix values from the API job name."""
    if job_name:
        m = re.search(r"\(([^)]*)\)\s*$", job_name)
        if m:
            return m.group(1)
    for prefix in JOB_LABELS:
        if artifact_name.startswith(prefix + "-"):
            suffix = artifact_name[len(prefix) + 1:]
            return suffix or "-"
    return "-"


def parse_junit(path):
    """Parse a JUnit XML and return (total, passed, failed_tests, flaky_tests).

    failed_tests: list of (name, message)
    flaky_tests:  list of (name, attempts, time)
    """
    tree = ET.parse(path)
    total = 0
    failed = []
    flaky = []

    for tc in tree.iter("testcase"):
        total += 1
        classname = tc.get("classname", "")
        name = tc.get("name", "")
        full = f"{classname}::{name}" if classname else name
        time = float(tc.get("time", 0))

        flaky_failures = list(tc.iter("flakyFailure"))
        hard_failure = tc.find("failure")

        if hard_failure is not None:
            msg = hard_failure.get("message", "")
            failed.append((full, msg))
        elif flaky_failures:
            flaky.append((full, len(flaky_failures), time))

    passed = total - len(failed) - len(flaky)
    return total, passed, failed, flaky


def render_table(rows, columns):
    """Render a markdown table. columns: list of (header, align), rows: list of tuples."""
    header = "| " + " | ".join(c[0] for c in columns) + " |"
    sep_map = {"left": "|---", "right": "|---:", "center": "|:---:"}
    separator = "".join(sep_map.get(c[1], "|---") for c in columns) + "|"
    lines = [header, separator]
    for row in rows:
        lines.append("| " + " | ".join(str(v) for v in row) + " |")
    return "\n".join(lines)


def render_details(summary, body):
    """Wrap body in a collapsible <details> block."""
    return f"<details>\n<summary>{summary}</summary>\n\n{body}\n\n</details>"


def shard_cell(artifact_name, shard_info):
    info = shard_info.get(artifact_name) or {}
    label = info.get("label") or "-"
    url = info.get("url")
    return f"[{label}]({url})" if url else label


def main():
    if len(sys.argv) < 2:
        print("Usage: flaky-report.py <glob-pattern> [<jobs.json>]", file=sys.stderr)
        sys.exit(1)

    files = sorted(glob.glob(sys.argv[1], recursive=True))
    jobs_by_runner = load_jobs(sys.argv[2] if len(sys.argv) > 2 else None)

    if not files:
        print("### Test Report\n\nNo JUnit XML files found — test jobs may have been skipped.")
        return

    # Per-job-type aggregation, per-artifact linking.
    # by_job[label] = {"total", "passed",
    #                   "failed": {(name, artifact): msg},
    #                   "flaky":  {(name, artifact): (attempts, time)}}
    # shard_info[artifact_name] = {"label": str, "url": str|None}
    by_job = {}
    shard_info = {}

    for path in files:
        art_dir, artifact_name = artifact_dir_and_name(path)
        label = job_label(artifact_name)

        if artifact_name not in shard_info:
            runner_name = read_runner_name(art_dir)
            job_meta = jobs_by_runner.get(runner_name) if runner_name else None
            shard_info[artifact_name] = {
                "label": shard_label_for(artifact_name, job_meta["name"] if job_meta else None),
                "url": job_meta["html_url"] if job_meta else None,
            }

        job = by_job.setdefault(label, {"total": 0, "passed": 0, "failed": {}, "flaky": {}})
        total, passed, failed, flaky = parse_junit(path)
        job["total"] += total
        job["passed"] += passed
        for name, msg in failed:
            job["failed"][(name, artifact_name)] = msg
        for name, attempts, time in flaky:
            key = (name, artifact_name)
            prev = job["flaky"].get(key)
            if prev is None or attempts > prev[0]:
                job["flaky"][key] = (attempts, time)

    lines = ["### Test Report"]

    for label in sorted(by_job):
        job = by_job[label]
        n_failed = len({name for (name, _) in job["failed"]})
        n_flaky = len({name for (name, _) in job["flaky"]})

        lines.append("")
        lines.append(
            f"#### `{label}` — {job['total']} total,"
            f" {job['passed']} passed,"
            f" {n_failed} failed,"
            f" {n_flaky} flaky"
        )

        sections = []

        if job["failed"]:
            rows = []
            for (name, artifact) in sorted(job["failed"]):
                msg = job["failed"][(name, artifact)]
                short = (msg[:80] + "...") if len(msg) > 80 else msg
                rows.append((f"`{name}`", short, shard_cell(artifact, shard_info)))
            table = render_table(
                rows,
                [("Test", "left"), ("Message", "left"), ("Shard", "left")],
            )
            sections.append(f"**Failed tests:**\n\n{table}")

        if job["flaky"]:
            rows = []
            items = sorted(job["flaky"].items(), key=lambda x: -x[1][0])
            for (name, artifact), (attempts, time) in items:
                rows.append((
                    f"`{name}`",
                    str(attempts),
                    f"{time:.1f}",
                    shard_cell(artifact, shard_info),
                ))
            table = render_table(
                rows,
                [
                    ("Test", "left"),
                    ("Failed attempts", "right"),
                    ("Time (s)", "right"),
                    ("Shard", "left"),
                ],
            )
            sections.append(f"**Flaky tests:**\n\n{table}")

        if sections:
            summary = ", ".join(
                s for s in [
                    f"{n_failed} failed" if n_failed else "",
                    f"{n_flaky} flaky" if n_flaky else "",
                ] if s
            )
            lines.append("")
            lines.append(render_details(summary, "\n\n".join(sections)))

    lines.append("")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
