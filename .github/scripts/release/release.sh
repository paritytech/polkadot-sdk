#!/usr/bin/env bash

# Set the new version by replacing the value of the constant given as patetrn
# in the file.
#
# input: pattern, version, file
#output: none
set_version() {
    pattern=$1
    version=$2
    file=$3

    sed -i "s/$pattern/\1\"${version}\"/g" $file
    return 0
}

# Commit changes to git with specific message.
# "|| true" does not let script to fail with exit code 1,
# in case there is nothing to commit.
#
# input: MESSAGE (any message which should be used for the commit)
# output: none
commit_with_message() {
    MESSAGE=$1
    git commit -a -m "$MESSAGE" || true
}
