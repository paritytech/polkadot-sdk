#!/usr/bin/env python3

"""
Zombienet Test Matrix Parser

This script parses YAML test definition files and converts them to JSON format
for use as GitHub Actions matrix jobs. It provides filtering capabilities to:

1. Exclude flaky tests (unless a specific test pattern is provided)
2. Filter tests by name pattern for targeted execution
3. Convert YAML test definitions to JSON matrix format

The script is used by GitHub Actions workflows to dynamically generate
test matrices based on YAML configuration files, enabling flexible
test execution and maintenance.

Usage:
    python parse-zombienet-tests.py --matrix tests.yml [--flaky-tests flaky.txt] [--test-pattern pattern]

Output:
    JSON array of test job objects suitable for GitHub Actions matrix strategy
"""

import argparse
import yaml
import json
import re

def parse_args():
    parser = argparse.ArgumentParser(description="Parse test matrix YAML file with optional filtering")
    parser.add_argument("--matrix", required=True, help="Path to the YAML matrix file")
    parser.add_argument("--flaky-tests", default="", help="Newline-separated list of flaky job names")
    parser.add_argument("--test-pattern", default="", help="Regex pattern to match job_name")
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
            if re.search(test_pattern, name):
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
