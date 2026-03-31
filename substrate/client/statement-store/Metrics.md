# Statement Store & Network Dashboards

Document provides a reference for every metric displayed in the two
Statement Store Grafana dashboards. For each metric, you will find: what it measures,
why it matters, how to read it, and what problems it helps diagnose.

## Table of Contents

- [Dashboard 1: Substrate Statement Store](#dashboard-1-substrate-statement-store)
  - [Storage Overview](#storage-overview)
  - [Throughput & Operations](#throughput--operations)
  - [Errors & Rejections](#errors--rejections)
  - [Capacity & Limits](#capacity--limits)
  - [Operation Latency](#operation-latency)
  - [Expiration & Cleanup](#expiration--cleanup)
  - [RPC Operations](#rpc-operations)
- [Dashboard 2: Substrate Statement Network](#dashboard-2-substrate-statement-network)
  - [Network Overview](#network-overview)
  - [Network Throughput & Bandwidth](#network-throughput--bandwidth)
  - [Network Operation Latency](#network-operation-latency)
  - [Network Health Indicators](#network-health-indicators)
  - [Initial Sync](#initial-sync)
---

## Dashboard 1: Substrate Statement Store

**UID:** `substrate-statement-store-last`

This dashboard covers the storage engine: capacity, throughput, errors, and lifecycle
of statements in the database.

### Storage Overview

#### Total Statements
- **Metric:** `substrate_sub_statement_store_statements_total`
- **Type:** Gauge (stat panel)
- **What it measures:** Count of statements persisted in the ParityDB store.
- **Why it matters:** Primary capacity indicator.
- **How to read:** Single stat panel. Green = healthy, yellow = 70%+ of capacity, red = 90%+.

#### Storage Used
- **Metric:** `substrate_sub_statement_store_bytes_total`
- **Type:** Gauge (stat panel)
- **What it measures:** Total bytes consumed by statement data in the store.
- **Why it matters:** Detects byte-level saturation independently of count.
- **How to read:** Color thresholds match statement count: green/yellow/red at 70%/90% of capacity.

#### Unique Accounts
- **Metric:** `substrate_sub_statement_store_accounts_total`
- **Type:** Gauge (stat panel)
- **What it measures:** Number of distinct accounts (public keys) that have at least one statement
  in the store.
- **Why it matters:** Indicates the diversity of statement authors. A single account flooding
  the store is a sign of abuse or misconfiguration.
- **How to read:** Single stat panel. No threshold coloring (informational).
- **Problems it solves:**
  - Detect single-account flooding (one account with thousands of statements).
  - Monitor adoption: how many unique participants are using the statement store.

#### Expired Statements
- **Metric:** `substrate_sub_statement_store_expired_total`
- **Type:** Gauge (stat panel)
- **What it measures:** Statements that have been marked as expired but not yet purged.
- **Why it matters:** A growing backlog of expired statements means the cleanup process
  is falling behind, or the expiration rate is very high.
- **How to read:** Single stat. Green = normal, yellow at 1000, red at 100000.
- **Problems it solves:**
  - Detect stalled or slow cleanup processes per node.
  - Identify nodes where expired statements accumulate (possible disk I/O issue).

---

### Throughput & Operations

#### Submission Rate (Successful)
- **Metric:** `rate(substrate_sub_statement_store_submitted_statements[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Successful statement submissions per second.
- **Why it matters:** Throughput metric that tells you how fast the system
  is processing valid work. Drops indicate upstream issues; spikes indicate bursts of activity.
- **How to read:** Line chart showing submissions/sec. Legend shows mean/max/sum.
- **Problems it solves:**
  - Detect throughput degradation after a release (compare before/after deployment).

#### Throughput vs Errors
- **Metrics:**
  - `rate(submitted_statements)` (green = successful)
  - `rate(validations_invalid)` (red = invalid)
  - `rate(rejections_total)` (orange = rejected)
- **What it measures:** Comparison of successful vs failed operations.
- **Why it matters:** A healthy system shows green >> red + orange.
- **How to read:** Overlapping lines; ratio of green to red/orange is the key signal.
- **Problems it solves:**
  - Detect capacity saturation.
  - Compare error ratios across releases to ensure quality.

#### Block Pruning Rate
- **Metric:** `rate(substrate_sub_statement_store_block_statements[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of statements pruned during block processing.
- **Why it matters:** Shows how frequently statements are cleaned up as part of block
  finalization. Spikes correlate with blocks containing many statement-related extrinsics.
- **How to read:** Line chart showing pruning events/sec per node.

---

### Errors & Rejections

#### Invalid Validations Rate
- **Metric:** `rate(substrate_sub_statement_store_validations_invalid[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Statements that failed proof verification (BadProof, NoProof)
  or exceeded encoding size limits during submission.
- **Why it matters:** Invalid statements consume validation resources
  without producing useful work. High rates indicate malicious actors, network corruption,
  or client bugs.
- **How to read:** Line chart. Any sustained rate above 0 warrants investigation.
- **Problems it solves:**
  - Detect ongoing attacks
  - Detect client bugs
  - Detect network corruption

#### Rejection Reasons Breakdown
- **Metric:** `rate(substrate_sub_statement_store_rejections_total[$__rate_interval])` with label `reason`
- **Type:** CounterVec (displayed as stacked rate)
- **Labels:** `reason` = `data_too_large` | `channel_priority_too_low` | `account_full` | `store_full` | `no_allowance`
- **What it measures:** Statements that passed validation but were rejected by the store.
- **Why it matters:** Each rejection reason points to a different problem:
  - **`store_full`** (red): The global store is at capacity.
  - **`account_full`** (orange): A specific account has hit its per-account quota.
  - **`channel_priority_too_low`** (yellow): A statement tried to replace a higher-priority
    statement in the same channel.
  - **`data_too_large`** (purple): Statement data exceeds `max_size`.
  - **`no_allowance`** (grey): The account has no statement allowance set by the runtime.
- **How to read:** Bar chart; color of the dominant bar tells you the primary rejection cause.

---

### Capacity & Limits

#### Statement Utilization
- **Metric:** `statements_total / capacity_statements * 100`
- **Type:** Gauge panel (percentage)
- **What it measures:** Statement count as a percentage of configured maximum capacity.
- **Why it matters:** Shows how close the store is to its statement limit.
- **How to read:** Gauge per node. Green = healthy headroom, red = approaching capacity.

#### Statements Stored
- **Metric:** `substrate_sub_statement_store_statements_total`
- **Type:** Gauge (timeseries)
- **What it measures:** Absolute statement count over time.
- **How to read:** Line chart per node. Look for trends (steady growth, sudden drops from pruning).

#### Byte Utilization
- **Metric:** `bytes_total / capacity_bytes * 100`
- **Type:** Gauge panel (percentage)
- **What it measures:** Byte usage as a percentage of configured maximum capacity.
- **Why it matters:** Byte-level saturation can occur even when statement count is within limits
  (large statements).
- **How to read:** Gauge per node. Same thresholds as statement utilization.

#### Storage Used (Over Time)
- **Metric:** `substrate_sub_statement_store_bytes_total`
- **Type:** Gauge (timeseries)
- **What it measures:** Total bytes stored over time.
- **How to read:** Line chart per node. Compare with byte capacity for headroom.

#### Expired Statements Over Time
- **Metric:** `substrate_sub_statement_store_expired_total`
- **Type:** Gauge (timeseries)
- **What it measures:** Number of expired statements over time.
- **Why it matters:** Helps identify accumulation patterns and verify cleanup is working.

#### Expired Map Memory Usage (Estimated)
- **Metric:** `substrate_sub_statement_store_expired_total * 104`
- **Type:** Timeseries (derived)
- **What it measures:** Estimated memory consumed by the expired-statement tracking map.
  Each entry is ~104 bytes (hash + metadata).
- **Why it matters:** Large expired backlogs consume memory even before purge.

#### Estimated Index Memory Usage
- **Metric:** `statements_total * 530 + accounts_total * 128`
- **Type:** Timeseries (derived)
- **What it measures:** Estimated memory consumed by the in-memory index structures.
  Each statement index entry is ~530 bytes; each account entry is ~128 bytes.
- **Why it matters:** Helps predict memory requirements as the store grows.

---

### Operation Latency

#### Submit Duration (Percentiles)
- **Metric:** `substrate_sub_statement_store_submit_duration_seconds` (histogram)
- **What it measures:** Time to submit a statement, including signature verification,
  runtime validation, and database write.
- **Why it matters:** The single most important latency metric. If p99 exceeds SLO,
  the node will build up backpressure under load.
- **How to read:** Three lines (green=p50, yellow=p90, red=p99). Gap between p50 and p99
  shows tail latency.
- **Problems it solves:**
  - Detect latency regressions after deployments.
  - Identify which sub-operation is slow by comparing with verify/DB write panels.
  - Set SLOs: "p99 submit latency < (N)ms".

---

### Expiration & Cleanup

#### Expiration Check Duration (Percentiles)
- **Metric:** `substrate_sub_statement_store_check_expiration_duration_seconds` (histogram)
- **What it measures:** Time spent in each expiration check cycle. Expiration periodically
  scans accounts and marks statements as expired.
- **Why it matters:** Expiration checks run on the main store thread. If they take too long,
  they block statement submissions.
- **How to read:** Three lines (p50/p90/p99). Should typically be sub-millisecond.
- **Problems it solves:**
  - Detect expiration performance degradation as the store grows.
  - Identify if expiration is blocking the submission path.

#### Expiration Rate
- **Metric:** `rate(substrate_sub_statement_store_statements_expired_total[$__rate_interval])`
- **What it measures:** Rate at which statements are expired (marked for later purge).
- **Why it matters:** A sudden spike means many statements expired at once.
- **How to read:** Line chart showing expired/sec. Legend shows mean/max/sum.
- **Problems it solves:**
  - Verify that the expiration system is working.
  - Detect mass expiration events.

---

### RPC Operations

These metrics come from the built-in substrate RPC middleware (`sc-rpc-server`)
which automatically instruments all JSON-RPC methods. No custom metrics
registration is needed — the middleware tracks every call by method name.

The dashboard filters for the following methods:
- `statement_submit`
- `statement_subscribeStatement`
- `statement_unsubscribeStatement`

To view all available RPC metrics for a running node:
```
curl http://127.0.0.1:9615/metrics | grep rpc
```

#### RPC Call Rate by Method
- **Metric:** `rate(substrate_rpc_calls_started{method=~"statement_submit|statement_subscribeStatement|statement_unsubscribeStatement"}[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **Labels:** `protocol` (ws/http), `method`
- **What it measures:** Rate of incoming RPC calls for statement methods.
- **Why it matters:** Shows the actual request rate hitting the node's JSON-RPC endpoint.
- **How to read:** Line chart showing calls/sec per method.
- **Problems it solves:**
  - Measure actual RPC load for capacity planning.
  - Detect traffic spikes or drops correlated with infrastructure events.

#### RPC Completed Calls (Success vs Error)
- **Metric:** `rate(substrate_rpc_calls_finished{method=~"statement_submit|statement_subscribeStatement|statement_unsubscribeStatement"}[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **Labels:** `protocol`, `method`, `is_error` (true/false), `is_rate_limited` (true/false)
- **What it measures:** Completed RPC calls split by success/error status.
- **Why it matters:** A healthy system shows `is_error=false` dominating.
- **How to read:** Line chart with method and error status breakdown. Red lines
  (`is_error=true`) should be near zero.

#### RPC Call Latency (Percentiles)
- **Metric:** `substrate_rpc_calls_time{method=~"statement_submit|statement_subscribeStatement|statement_unsubscribeStatement"}` (histogram, in microseconds)
- **Type:** Histogram
- **Labels:** `protocol`, `method`, `is_rate_limited`
- **What it measures:** End-to-end latency of statement RPC calls as experienced by callers.
- **How to read:** Three lines (p50, p90, p99). Units are microseconds.
- **Problems it solves:**
  - Set SLOs on the public API surface.
  - Detect latency regressions after deployments.

#### RPC Error Rate
- **Metric:** `rate(substrate_rpc_calls_finished{method=~"statement_submit|statement_subscribeStatement|statement_unsubscribeStatement", is_error="true"}[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of statement RPC calls that completed with errors.
- **How to read:** Line chart per method. Any sustained rate above 0 warrants investigation.

---

## Dashboard 2: Substrate Statement Network

**UID:** `substrate-statement-network-last`

This dashboard covers the gossip/networking layer: peer connectivity, statement propagation,
bandwidth, latency, and network health indicators.

### Network Overview

#### Peers Connected (per node)
- **Metric:** `substrate_sync_statement_peers_connected`
- **Type:** Gauge (bar gauge panel)
- **What it measures:** Current number of peers connected via the statement gossip protocol.
- **Why it matters:** Zero peers means complete isolation. Low peer counts (<3) mean
  slow propagation and possible partitioning.
- **How to read:** Bar gauge per node.
- **Problems it solves:**
  - Detect degraded connectivity: few peers mean slow statement propagation.

#### Statements Received/s
- **Metric:** `rate(substrate_sync_statements_received[$__rate_interval])`
- **Type:** Counter (stat panel, displayed as rate)
- **What it measures:** Aggregate rate of statements received from peers across all nodes.
- **Why it matters:** Quick overview of inbound statement throughput.

#### Statements Propagated/s
- **Metric:** `rate(substrate_sync_propagated_statements[$__rate_interval])`
- **Type:** Counter (stat panel, displayed as rate)
- **What it measures:** Aggregate rate of statements propagated to peers across all nodes.
- **Why it matters:** Quick overview of outbound statement throughput.

#### Pending Validations
- **Metric:** `substrate_sync_pending_statement_validations`
- **Type:** Gauge (stat panel)
- **What it measures:** Number of statements waiting in the validation queue.
- **Why it matters:** Leading indicator for the validation pipeline.
- **How to read:** Single stat.
- **Problems it solves:**
  - Detect validation bottleneck: if consistently high, validation is a bottleneck.

#### Statements Received vs Propagated
- **Metrics:**
  - `rate(substrate_sync_statements_received)` (blue = received from peers)
  - `rate(substrate_sync_propagated_statements)` (green = sent to peers)
- **What it measures:** Inbound vs outbound statement rates over time.
- **Why it matters:** Primary indicator of network participation balance:
  - **Received ~ Propagated**: Balanced node.
  - **Received >> Propagated**: Mostly consuming (new or catching up).
  - **Propagated >> Received**: Mostly producing.
- **How to read:** Two-line chart. Mean and max values in the legend table.
- **Problems it solves:**
  - Detect propagation regressions across releases.
  - Identify nodes that are not propagating (broken outbound path).

#### Peers Connected Over Time
- **Metric:** `substrate_sync_statement_peers_connected` (timeseries)
- **What it measures:** Peer count over time.
- **Why it matters:** Drops indicate network instability or node restarts.
- **How to read:** Line chart with mean/max/last in legend. Look for step changes and trends.
- **Problems it solves:**
  - Detect network partitioning (drop to 0).
  - Detect gradual peer loss.

---

### Network Throughput & Bandwidth

#### Network Bandwidth (Statement Protocol)
- **Metrics:**
  - `rate(substrate_sync_statement_bytes_sent_total)` (orange = bytes sent/sec)
  - `rate(substrate_sync_statement_bytes_received_total)` (blue = bytes received/sec)
- **Type:** Counter (displayed as rate)
- **What it measures:** Actual network bandwidth consumed by the statement protocol.
- **Why it matters:** Metric for sizing network requirements.
- **How to read:** Two-line chart in bytes/sec. Compare sent vs received for balance.
- **Problems it solves:**
  - Detect bandwidth issues.
  - Compare bandwidth across releases to measure protocol efficiency improvements.

---

### Network Operation Latency

#### Statement Send Latency (Percentiles)
- **Metric:** `substrate_sync_statement_sent_latency_seconds` (histogram)
- **What it measures:** Time to send a statement notification to a peer via the network layer.
- **Why it matters:** Measures network-level latency. High values indicate network
  congestion or slow peers.
- **How to read:** Three lines (p50/p90/p99).
- **Problems it solves:**
  - Detect network congestion between specific peers.
  - Identify slow peers.
  - Set SLOs: "p99 send latency < 2s".

---

### Network Health Indicators

#### Gossip Redundancy
- **Metrics:**
  - `rate(substrate_sync_statements_received)` (total received)
  - `rate(substrate_sync_known_statement_received)` (known/redundant)
- **What it measures:** Compares total received statements against statements the node
  already knew about. The gap between the two lines is the rate of new, useful statements.
- **Why it matters:** High redundancy wastes bandwidth. If known/received ratio approaches 1.0,
  nearly all gossip is duplicate traffic.
- **How to read:** Two-line chart. A large gap between total and known means most statements
  are new (healthy). Lines converging means high redundancy.
- **Problems it solves:**
  - Detect inefficient gossip topology.
  - Identify nodes that receive mostly redundant data.

#### Dropped Statements
- **Metrics:**
  - `rate(substrate_sync_ignored_statements)` (backpressure drops)
  - `rate(substrate_sync_skipped_oversized_statements)` (oversized drops)
- **What it measures:** Statements that were dropped before processing. Ignored statements
  are dropped due to exceeding the pending validation limit (backpressure). Skipped statements
  exceed the maximum allowed size.
- **Why it matters:** Any sustained drop rate indicates the node is losing data.
- **How to read:** Two-line chart. Both lines should be near zero in normal operation.
- **Problems it solves:**
  - Detect backpressure (validation queue full).
  - Detect oversized statement attacks or client bugs.

#### Flooding Detection
- **Metric:** `rate(substrate_sync_statement_flooding_detected[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate at which the gossip protocol detects flooding from peers.
- **Why it matters:** Flooding indicates a peer sending statements faster than the rate limit
  allows. Could be a misbehaving or malicious peer.
- **How to read:** Line chart per node. Non-zero rates warrant investigation.
- **Problems it solves:**
  - Detect abusive peers.
  - Verify rate-limiting effectiveness.

#### Propagation Chunk Size Distribution
- **Metric:** `substrate_sync_propagated_statements_chunks` (histogram)
- **What it measures:** Distribution of how many statements are batched together in each
  propagation chunk sent to peers.
- **Why it matters:** Small chunks mean frequent but lightweight sends. Large chunks mean
  fewer but heavier sends. The distribution helps tune batch sizes.
- **How to read:** Three lines (p50/p90/p99) showing chunk sizes.
- **Problems it solves:**
  - Tune propagation batch sizes for optimal throughput.
  - Detect if batching is working as expected.

---

### Initial Sync

#### Initial Sync Active Peers
- **Metric:** `substrate_sync_initial_sync_peers_active`
- **Type:** Gauge (stat panel)
- **What it measures:** Number of peers currently being synced via the initial sync burst mechanism.
- **Why it matters:** Each active initial sync consumes bandwidth and CPU. Too many concurrent
  syncs can starve normal gossip propagation.
- **How to read:** Single stat. Green = 0-4, yellow = 5-19, red = 20+.
- **Problems it solves:**
  - Detect excessive initial syncs.
  - Capacity planning for nodes that frequently accept new peers.

#### Initial Sync Statements Sent
- **Metric:** `rate(substrate_sync_initial_sync_statements_sent[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of statements sent to peers during initial sync bursts.
- **Why it matters:** High rates indicate many new peers are connecting and receiving full
  statement sets. Impacts network bandwidth.
- **How to read:** Line chart showing statements/sec. Legend shows mean/max.
- **Problems it solves:**
  - Understand initial sync's share of network traffic.

#### Initial Sync Burst Rate
- **Metric:** `rate(substrate_sync_initial_sync_bursts_total[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of initial sync burst rounds being processed.
- **Why it matters:** Each burst sends one batch of statements to one peer (round-robin).
  Burst rate combined with statements-per-burst gives the effective sync throughput.
- **How to read:** Line chart showing bursts/sec. Legend shows mean/max.
- **Problems it solves:**
  - Verify that round-robin is distributing work across peers.
  - Detect stalled initial syncs (burst rate drops to 0 while active peers > 0).

#### Per-Peer Initial Sync Duration (Percentiles)
- **Metric:** `substrate_sync_initial_sync_duration_seconds` (histogram)
- **Buckets:** 10ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s, 30s, 60s
- **What it measures:** Total wall-clock time from the first burst to completion of initial
  sync for each peer.
- **Why it matters:** Long sync durations mean new peers wait a long time before having
  a complete view of the statement store. Threshold: 5s (yellow), 30s (red).
- **How to read:** Three lines (green=p50, yellow=p90, red=p99).
- **Problems it solves:**
  - Detect slow initial syncs that delay new peer participation.
  - Compare sync durations as statement store size grows.
  - Identify network-level bottlenecks.

---
