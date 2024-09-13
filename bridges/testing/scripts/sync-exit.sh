#!/bin/bash
set -e

# every network adds a char to the file, let's remove ours
truncate -s -1 $TEST_FOLDER/exit-sync

# when all chars are removed, then our test is done
while true
do
    if [ `stat --printf="%s" $TEST_FOLDER/exit-sync` -eq 0 ]; then
        exit
    fi
    sleep 100
done
