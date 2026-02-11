#!/bin/bash
set -e
PASSED=0
FAILED=0
CHROMIUM_PATH="/nix/store/g245pzpbacazlrca1fb7crb9883rhhs3-chromium-144.0.7559.59/bin/chromium"
export CHROMIUM_PATH

for i in $(seq 1 10); do
  echo "=========================================="
  echo "RUN $i / 10 (passed=$PASSED, failed=$FAILED)"
  echo "=========================================="
  
  pkill -f "smoldot-proxy" 2>/dev/null || true
  pkill -f chromium 2>/dev/null || true
  sleep 2
  
  if npx tsx tests/full-browser-e2e.ts 2>&1 | tee /tmp/e2e-10x-run$i.log; then
    PASSED=$((PASSED + 1))
    echo "RUN $i: PASSED ($PASSED/$i)"
  else
    FAILED=$((FAILED + 1))
    echo "RUN $i: FAILED ($FAILED/$i)"
    echo "STOPPING: test failed"
    exit 1
  fi
done

echo "=========================================="
echo "ALL 10 RUNS PASSED!"
echo "=========================================="
