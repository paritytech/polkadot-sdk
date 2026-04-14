#!/usr/bin/env python3
"""Parse nextest JUnit XML files and produce a test summary with flaky/failed details.

Artifacts are expected under <dir>/<artifact-name>/junit.xml where the artifact
name encodes the job (e.g. junit-stable-0, junit-no-try-runtime-1, junit-benchmarks).
The report is grouped by job. Output goes to stdout.
"""

import glob
import sys
import xml.etree.ElementTree as ET

# Map artifact-name prefix to a human-readable job heading.
JOB_LABELS = {
    "junit-stable": "test-linux-stable",
    "junit-no-try-runtime": "test-linux-stable-no-try-runtime",
    "junit-benchmarks": "test-linux-stable-runtime-benchmarks",
}


def job_label(path):
    """Derive a job label from the artifact directory name in the path."""
    parts = path.replace("\\", "/").split("/")
    for part in parts:
        for prefix, label in JOB_LABELS.items():
            if part == prefix or part.startswith(prefix + "-"):
                return label
    return path.replace("\\", "/").rsplit("/", 1)[0].rsplit("/", 1)[-1]


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


def main():
    if len(sys.argv) < 2:
        print("Usage: flaky-report.py <glob-pattern>", file=sys.stderr)
        sys.exit(1)

    files = sorted(glob.glob(sys.argv[1], recursive=True))
    if not files:
        print("### Test Report\n\nNo JUnit XML files found — test jobs may have been skipped.")
        return

    # Aggregate per job.
    # {label: {total, passed, failed: {name: msg}, flaky: {name: (attempts, time)}}}
    by_job = {}
    for path in files:
        label = job_label(path)
        job = by_job.setdefault(label, {"total": 0, "passed": 0, "failed": {}, "flaky": {}})
        total, passed, failed, flaky = parse_junit(path)
        job["total"] += total
        job["passed"] += passed
        for name, msg in failed:
            job["failed"][name] = msg
        for name, attempts, time in flaky:
            prev = job["flaky"].get(name)
            if prev is None or attempts > prev[0]:
                job["flaky"][name] = (attempts, time)

    lines = [
        "### Test Report",
    ]

    for label in sorted(by_job):
        job = by_job[label]
        lines.append("")
        lines.append(
            f"#### `{label}` — {job['total']} total,"
            f" {job['passed']} passed,"
            f" {len(job['failed'])} failed,"
            f" {len(job['flaky'])} flaky"
        )

        if job["failed"]:
            lines.append("")
            lines.append("**Failed tests:**")
            lines.append("")
            rows = []
            for name in sorted(job["failed"]):
                msg = job["failed"][name]
                # Truncate long messages for the table
                short = (msg[:80] + "...") if len(msg) > 80 else msg
                rows.append((f"`{name}`", short))
            lines.append(render_table(rows, [("Test", "left"), ("Message", "left")]))

        if job["flaky"]:
            lines.append("")
            lines.append("**Flaky tests:**")
            lines.append("")
            rows = []
            for name, (attempts, time) in sorted(
                job["flaky"].items(), key=lambda x: -x[1][0]
            ):
                rows.append((f"`{name}`", str(attempts), f"{time:.1f}"))
            lines.append(render_table(
                rows,
                [("Test", "left"), ("Failed attempts", "right"), ("Time (s)", "right")],
            ))

    lines.append("")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
