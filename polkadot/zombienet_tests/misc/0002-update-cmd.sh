#!/bin/bash

set -euxo pipefail

if [[ $(grep "insecure-validator-i-know-what-i-do" /cfg/zombie.cmd) ]]; then
  echo "insecure flag is already part of the cmd";
else
  echo -n " --insecure-validator-i-know-what-i-do" >> /cfg/zombie.cmd;
fi;

echo "update-cmd" > /tmp/zombiepipe;