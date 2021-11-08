#!/usr/bin/env bash

REF1=$1

JSON=$(git log $REF1..HEAD \
    --pretty=format:'{ "commit": "%H", "short_sha": "%h", "author": "%an", "date": "%ad", "message": "%s"},' \
    $@ | \
    perl -pe 'BEGIN{print "{ \"since\": \"'${REF1}'\",  \"commits\": ["}; END{print "]}"}' | \
    perl -pe 's/},]/}]/')

echo $JSON | tera --template templates/changelog.md --stdin | tee
