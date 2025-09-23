#!/usr/bin/env bash

# This file is part of Substrate.
# Copyright (C) Parity Technologies (UK) Ltd.
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -e

cargo build --release -p cumulus-test-service --bin test-parachain -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot -p polkadot-parachain-bin --bin polkadot-parachain

RELEASE_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release

export PATH=$RELEASE_DIR:$PATH
ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci -- --ignored sparsely_connected_network_block_propagation_time
