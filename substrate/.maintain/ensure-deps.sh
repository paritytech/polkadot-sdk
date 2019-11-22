#!/usr/bin/env bash

# The script is meant to check if the rules regarding packages
# dependencies are satisfied.
# The general format is:
# [top-lvl-dir]<[crate-name-prefix]

# For instance no crate within `./client` directory
# is allowed to import any crate with a directory path containing `frame`.
# Such rule is just: `client<frame`.

# The script should be run from the main repo directory!

set -u

PLEASE_DONT=(
	"client<frame"
	"client<node"
	"frame<node"
	"frame<client"
	"primitives<frame"
	"primitives<client"
)

VIOLATIONS=()
PACKAGES=()

for rule in "${PLEASE_DONT[@]}"
do
	from=$(echo $rule | cut -f1 -d\<)
	to=$(echo $rule | cut -f2 -d\<)

	cd $from
	echo "Checking rule $rule"
	packages=$(find -name Cargo.toml | xargs grep -wn "path.*$to")
	has_references=$(echo -n $packages | wc -c)
	if [ "$has_references" != "0" ]; then
		VIOLATIONS+=("$rule")
		# Find packages that violate:
		PACKAGES+=("$packages")
	fi
	cd - > /dev/null
done

# Display violations and fail
I=0
for v in "${VIOLATIONS[@]}"
do
	cat << EOF

===========================================
======= Violation of rule: $v
===========================================
${PACKAGES[$I]}


EOF
	I=$I+1
done

exit ${#VIOLATIONS[@]}
