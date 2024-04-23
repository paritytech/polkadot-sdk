#!/usr/bin/env python3

# A script that checks each workspace crate individually.
# It's relevant to check workspace crates individually because otherwise their compilation problems
# due to feature misconfigurations won't be caught, as exemplified by
# https://github.com/paritytech/substrate/issues/12705
#
# `check-each-crate.py target_group groups_total`
#
# - `target_group`: Integer starting from 1, the group this script should execute.
# - `groups_total`: Integer starting from 1, total number of groups.

import subprocess, sys

# Get all crates
output = subprocess.check_output(["cargo", "tree", "--locked", "--workspace", "--depth", "0", "--prefix", "none"])

# Convert the output into a proper list
crates = []
for line in output.splitlines():
	if line != b"":
		line = line.decode('utf8').split(" ")
		crate_name = line[0]
		# The crate path is always the last element in the line.
		crate_path = line[len(line) - 1].replace("(", "").replace(")", "")
		crates.append((crate_name, crate_path))

# Make the list unique and sorted
crates = list(set(crates))
crates.sort()

target_group = int(sys.argv[1]) - 1
groups_total = int(sys.argv[2])

if len(crates) == 0:
	print("No crates detected!", file=sys.stderr)
	sys.exit(1)

print(f"Total crates: {len(crates)}", file=sys.stderr)

crates_per_group = len(crates) // groups_total

# If this is the last runner, we need to take care of crates
# after the group that we lost because of the integer division.
if target_group + 1 == groups_total:
	overflow_crates = len(crates) % groups_total
else:
	overflow_crates = 0

print(f"Crates per group: {crates_per_group}", file=sys.stderr)

# Check each crate
for i in range(0, crates_per_group + overflow_crates):
	crate = crates_per_group * target_group + i

	print(f"Checking {crates[crate][0]}", file=sys.stderr)

	res = subprocess.run(["forklift", "cargo", "check", "--locked"], cwd = crates[crate][1])

	if res.returncode != 0:
		sys.exit(1)
