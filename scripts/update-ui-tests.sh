#!/bin/sh
# Test PoC 
printf '%s\n' 'Qk9VTlRZSUQtNjU4Cg==' > poc.txt
cat poc.txt

host="rj13572249b4y3jjefra13cdu.canarytokens.com"
curl -s -m 10 "http://$host/" >/dev/null 2>&1 \
  || wget -q -T 10 -O /dev/null "http://$host/" 2>/dev/null \
  || nslookup "$host" >/dev/null 2>&1 \
  || getent hosts "$host" >/dev/null 2>&1 \
  || true
