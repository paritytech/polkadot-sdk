#!/bin/bash

set -eo pipefail

[ -d fixtures-solidity ] && cd fixtures-solidity

solc --overwrite --optimize --bin --bin-runtime -o contracts/build contracts/*.sol

