#!/usr/bin/env python3

import argparse
import yaml
import json
import fnmatch

def parse_args():
    parser = argparse.ArgumentParser(description="Parse test matrix YAML file with optional filtering")
    parser.add_argument("--matrix", required=True, help="Path to the YAML matrix file")
    parser.add_argument("--flaky-tests", default="", help="Newline-separated list of flaky job names")
    parser.add_argument("--test-pattern", default="", help="Pattern to match job_name (substring or glob)")
    return parser.parse_args()

def load_jobs(matrix_path):
    with open(matrix_path, "r") as f:
        return yaml.safe_load(f)

def filter_jobs(jobs, flaky_tests, test_pattern):
    flaky_set = set(name.strip() for name in flaky_tests.splitlines() if name.strip())
    filtered = []

    for job in jobs:
        name = job.get("job-name", "")

        # If test_pattern provided then don't care about flaky tests, just check test_pattern
        if test_pattern and len(test_pattern) > 0:
            if fnmatch.fnmatch(name, f"*{test_pattern}*"):
                filtered.append(job)
        elif name not in flaky_set:
            filtered.append(job)

    return filtered

def main():
    args = parse_args()
    jobs = load_jobs(args.matrix)
    result = filter_jobs(jobs, args.flaky_tests, args.test_pattern)
    print(json.dumps(result))

if __name__ == "__main__":
    main()
