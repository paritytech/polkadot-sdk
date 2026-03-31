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
- [Dashboard 2: Substrate Statement Network](#dashboard-2-substrate-statement-network)
  - [Network Overview](#network-overview)
  - [Network Throughput & Bandwidth](#network-throughput--bandwidth)
  - [Operation Latency (Network)](#operation-latency-network)
  - [Network Health Indicators](#network-health-indicators)
  - [Initial Sync](#initial-sync)
---

## Dashboard 1: Substrate Statement Store

**UID:** `substrate-statement-store`

This dashboard covers the storage engine: capacity, throughput, errors, memory estimation,
latency, and lifecycle of statements in the database.

### Storage Overview

#### Total Statements
- **Metric:** `substrate_sub_statement_store_statements_total`
- **Type:** Gauge
- **What it measures:** Count of statements persisted in the ParityDB store.
- **Why it matters:** Primary capacity indicator.
- **How to read:** Single stat panel. Green = healthy, yellow = 70%+ of capacity, red = 90%+.

#### Storage Used (Bytes)
- **Metric:** `substrate_sub_statement_store_bytes_total`
- **Type:** Gauge
- **What it measures:** Total bytes consumed by statement data in the store.
- **Why it matters:** Metric detects byte-level saturation independently of count.
- **How to read:** Color thresholds match statement count: green/yellow/red at 70%/90% of capacity.

#### Unique Accounts
- **Metric:** `substrate_sub_statement_store_accounts_total`
- **Type:** Gauge
- **What it measures:** Number of distinct accounts (public keys) that have at least one statement
  in the store.
- **Why it matters:** Indicates the diversity of statement authors. A single account flooding
  the store is a sign of abuse or misconfiguration. A healthy network shows many accounts.
- **How to read:** Single stat panel. No threshold coloring (informational).
- **Problems it solves:**
  - Detect single-account flooding (one account with thousands of statements).
  - Monitor adoption: how many unique participants are using the statement store.

#### Expired Statements
- **Metric:** `substrate_sub_statement_store_expired_total`
- **Type:** Gauge
- **What it measures:** Statements that have been marked as expired but not yet purged
  (purge happens after 8h by default).
- **Why it matters:** A growing backlog of expired statements means the cleanup process
  is falling behind, or the expiration rate is very high.
- **How to read:** Single stat. Green = healthy, yellow = 1000+, red = 100000+.
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
  is processing valid work. Drops indicate upstream issues (fewer clients, network problems);
  spikes indicate bursts of activity.
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
- **What it measures:** Rate of statements pruned during block processing. Each block import
  triggers pruning of statements that have been included on-chain.
- **Why it matters:** Confirms the block import pipeline is correctly removing on-chain
  statements from the store. A flat line when blocks are being produced indicates a
  pruning failure.
- **How to read:** Line chart showing pruned/sec. Legend shows mean/max.
- **Problems it solves:**
  - Verify that block-triggered pruning is working.
  - Correlate pruning rate with block production rate.

---

### Errors & Rejections

#### Invalid Validations Rate
- **Metric:** `rate(substrate_sub_statement_store_validations_invalid[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Statements that failed proof verification (BadProof, NoProof)
  or exceeded encoding size limits during submission.
- **Why it matters:** Invalid statements consume validation resources
  (CPU for signature verification) without producing useful work. High rates indicate
  malicious actors submitting garbage, network corruption, or client bugs.
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
  - **`store_full`** (red): The global store is at capacity. All new submissions are rejected.
    Action: increase capacity or reduce statement lifetimes.
  - **`account_full`** (orange): A specific account has hit its per-account quota. The account
    owner needs to remove old statements before submitting new ones.
  - **`channel_priority_too_low`** (yellow): A statement tried to replace a higher-priority
    statement in the same channel. The submitter should increase priority.
  - **`data_too_large`** (purple): Statement data exceeds `max_size`. The client is submitting
    oversized data.
  - **`no_allowance`** (grey): The account has no statement allowance set by the runtime.
    The runtime must grant an allowance before the account can submit statements.
- **How to read:** Bar chart; color of the dominant bar tells you the primary
  rejection cause.

---

### Capacity & Limits

#### Statement Utilization
- **Metric:** `substrate_sub_statement_store_statements_total / substrate_sub_statement_store_capacity_statements * 100`
- **Type:** Gauge (percentage)
- **What it measures:** Percentage of statement count capacity currently in use.
- **Why it matters:** Single-glance capacity indicator. Shows how close the store is to full.
- **How to read:** Gauge panel. Green < 70%, yellow 70-90%, red > 90%.
- **Problems it solves:**
  - Detect when the store is approaching capacity limits.
  - Verify that capacity configuration changes took effect after a deployment.

#### Statements Stored
- **Metric:** `substrate_sub_statement_store_statements_total`
- **Type:** Gauge (time series)
- **What it measures:** Number of statements currently stored over time.
- **Why it matters:** Shows the growth trend of statement count. Auto-scaled Y-axis for readability.
- **How to read:** Line chart. Look for step changes and trends.

#### Byte Utilization
- **Metric:** `substrate_sub_statement_store_bytes_total / substrate_sub_statement_store_capacity_bytes * 100`
- **Type:** Gauge (percentage)
- **What it measures:** Percentage of byte capacity currently in use.
- **Why it matters:** Same as Statement Utilization, but for byte-level capacity.
- **How to read:** Gauge panel. Green < 70%, yellow 70-90%, red > 90%.

#### Storage Used (Time Series)
- **Metric:** `substrate_sub_statement_store_bytes_total`
- **Type:** Gauge (time series)
- **What it measures:** Bytes used by statement data over time.
- **Why it matters:** Shows the growth trend of storage consumption. Auto-scaled Y-axis.
- **How to read:** Line chart. Look for step changes and trends.

#### Expired Statements Over Time
- **Metric:** `substrate_sub_statement_store_expired_total`
- **Type:** Gauge (time series)
- **What it measures:** Number of statements in the expired HashMap over time. This map
  grows unboundedly and is a key memory pressure indicator.
- **Why it matters:** High values (100K+) signal significant memory usage (~100 bytes per entry).
- **How to read:** Line chart. Thresholds: yellow = 1000, red = 100000.
- **Problems it solves:**
  - Detect unbounded growth of the expired map.
  - Correlate expired count with memory usage.

#### Expired Map Memory Usage (Estimated)
- **Metric:** `substrate_sub_statement_store_expired_total * 104`
- **Type:** Derived (calculated)
- **What it measures:** Estimated memory usage of the expired statement tracking HashMap,
  calculated as expired_total x 104 bytes per entry (32 hash + 8 timestamp + 64 overhead).
- **Why it matters:** High values indicate significant memory pressure from expired statement
  tracking. Directly translates expired count into memory cost.
- **How to read:** Line chart in bytes. Thresholds: yellow = 10 MB, red = 100 MB.
- **Problems it solves:**
  - Quantify memory impact of expired statement accumulation.
  - Trigger capacity planning when memory usage exceeds thresholds.

#### Estimated Index Memory Usage
- **Metric:** `substrate_sub_statement_store_statements_total * 530 + substrate_sub_statement_store_accounts_total * 128`
- **Type:** Derived (calculated)
- **What it measures:** Estimated RAM used by the in-memory statement index. Calculated as
  ~530 bytes per statement (entries, topics_and_keys, recent, by_topic, by_dec_key, by_priority)
  + ~128 bytes per unique account (account HashMap + container bases). Does not include expired
  map (shown separately) or ParityDB disk cache.
- **Why it matters:** Provides a memory budget estimate without requiring heap profiling.
- **How to read:** Line chart in bytes. Thresholds: yellow = 100 MB, red = 1 GB.
- **Problems it solves:**
  - Capacity planning: predict memory requirements as the store grows.
  - Detect unexpected memory growth from statement or account inflation.

---

### Operation Latency

Good for performance comparison across releases. Shows p50/p90/p99
percentiles, making it easy to spot tail latency regressions.

#### Submit Duration (Percentiles)
- **Metric:** `substrate_sub_statement_store_submit_duration_seconds` (histogram)
- **Buckets:** 1us, 10us, 100us, 1ms, 10ms, 100ms, 1s
- **What it measures:** Time to submit a statement, including signature verification,
  runtime validation, and database write.
- **Why it matters:** This is the single most important latency metric; it affects
  how fast the node can process incoming statements. If p99 > SLO the node will build
  up backpressure under load.
- **How to read:** Three lines (green=p50, yellow=p90, red=p99). Gap between p50 and p99 shows tail latency.
- **Problems it solves:**
  - Detect latency regressions after deployments (compare p50/p90/p99 before and after).
  - Identify which sub-operation is slow by comparing with verify/DB write panels.
  - Set SLOs: "p99 submit latency < (N)ms".

---

### Expiration & Cleanup

#### Expiration Check Duration (Percentiles)
- **Metric:** `substrate_sub_statement_store_check_expiration_duration_seconds` (histogram)
- **Buckets:** 1us, 10us, 100us, 1ms, 10ms, 100ms, 1s
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

## Dashboard 2: Substrate Statement Network

**UID:** `substrate-statement-network`

Dashboard covers the gossip/networking layer: peer connectivity, statement propagation,
bandwidth, latency, and network health indicators.

### Network Overview

#### Peers Connected (Stat)
- **Metric:** `substrate_sync_statement_peers_connected`
- **Type:** Gauge
- **What it measures:** Current number of peers connected via the statement gossip protocol.
- **Why it matters:** Zero peers means complete isolation: the node cannot send or receive
  statements. Low peer counts (<3) mean slow propagation and possible partitioning.
- **How to read:** Single stat. Red = 0, yellow = 1-2, green = 3+.
- **Problems it solves:**
  - Detect degraded connectivity: few peers mean slow statement propagation.

#### Statements Received/s (Stat)
- **Metric:** `rate(substrate_sync_statements_received[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of statements received from peers per second.
- **Why it matters:** Inbound throughput indicator. A drop to zero when peers are connected
  suggests the gossip protocol is stalled.
- **How to read:** Single stat with sparkline. Blue color, ops/sec unit.

#### Statements Propagated/s (Stat)
- **Metric:** `rate(substrate_sync_propagated_statements[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of statements propagated to peers per second.
- **Why it matters:** Outbound throughput indicator. Confirms the node is forwarding
  statements to its peers.
- **How to read:** Single stat with sparkline. Green color, ops/sec unit.

#### Pending Validations (Stat)
- **Metric:** `substrate_sync_pending_statement_validations`
- **Type:** Gauge
- **What it measures:** Number of statements waiting in the validation queue.
- **Why it matters:** Backlog metric for the validation pipeline.
- **How to read:** Single stat. Green = 0-49, yellow = 50-99, red = 100+.
- **Problems it solves:**
  - Detect validation bottleneck: if consistently high, validation cannot keep up with inbound rate.

#### Statements Received vs Propagated (Time Series)
- **Metrics:**
  - `rate(statements_received)` (blue = received from peers)
  - `rate(propagated_statements)` (green = sent to peers)
- **What it measures:** Inbound vs outbound statement rates over time.
- **Why it matters:** Primary indicator of network participation balance:
  - **Received ~ Propagated**: Balanced node. Receiving and forwarding roughly equally.
  - **Received >> Propagated**: Mostly consuming. The node may be new or catching up.
  - **Propagated >> Received**: Mostly producing. The node is generating more than it receives.
- **How to read:** Two-line chart. Mean and max values in the legend table.
- **Problems it solves:**
  - Compare network activity across releases to detect propagation regressions.
  - Identify nodes that are not propagating (broken outbound path).

#### Peers Connected Over Time
- **Metric:** `substrate_sync_statement_peers_connected` (time series)
- **What it measures:** Peer count connected over time.
- **Why it matters:** Drops indicate network instability, for example node restarts.
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

### Operation Latency (Network)

#### Statement Send Latency (Percentiles)
- **Metric:** `substrate_sync_statement_sent_latency_seconds` (histogram)
- **Buckets:** 1us, 10us, 100us, 1ms, 10ms, 100ms, 1s
- **What it measures:** Time to send a statement notification to a peer via the network layer.
- **Why it matters:** Measures network-level latency; high values indicate network
  congestion or slow peers.
- **How to read:** Three lines (p50/p90/p99).
- **Problems it solves:**
  - Detect network congestion between specific peers.
  - Identify slow peers.
  - Compare send latency across releases to measure protocol improvements.
  - Set SLOs: "p99 send latency < 2s".

---

### Network Health Indicators

#### Gossip Redundancy
- **Metrics:**
  - `rate(substrate_sync_statements_received)` (blue = total received)
  - `rate(substrate_sync_known_statement_received)` (yellow = already known / redundant)
- **Type:** Counter (displayed as rate)
- **What it measures:** How much received gossip is redundant (already known). Compares
  total received statements with those that were already in the local store.
- **Why it matters:** High redundancy ratio indicates gossip inefficiency. The network is
  spending bandwidth re-sending statements the node already has.
- **How to read:** Two-line chart. The gap between blue and yellow is the useful (new) traffic.
  If yellow approaches blue, most gossip is wasted.
- **Problems it solves:**
  - Detect gossip inefficiency and wasted bandwidth.
  - Tune gossip protocol parameters to reduce redundant traffic.

#### Dropped Statements
- **Metrics:**
  - `rate(substrate_sync_ignored_statements)` (red = ignored due to backpressure)
  - `rate(substrate_sync_skipped_oversized_statements)` (orange = skipped due to size limits)
- **Type:** Counter (displayed as rate)
- **What it measures:** Statements lost to backpressure or size limits. Any drops indicate
  data loss in the gossip layer.
- **Why it matters:** Dropped statements mean the node is missing data. Backpressure drops
  indicate the validation pipeline is overwhelmed; oversized drops indicate peers sending
  statements that exceed protocol limits.
- **How to read:** Two-line chart. Any rate above 0 is yellow, above 1/sec is red.
- **Problems it solves:**
  - Detect data loss in the gossip layer.
  - Distinguish between backpressure (system overload) and oversized (peer misconfiguration).
  - Trigger investigation into validation pipeline capacity.

#### Flooding Detection
- **Metric:** `rate(substrate_sync_statement_flooding_detected[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of flooding detection events. Fires when peers are rate-limited
  for sending statements too rapidly.
- **Why it matters:** Security-relevant: sustained flooding may indicate a misbehaving or
  malicious peer. Any non-zero value warrants investigation.
- **How to read:** Line chart. Threshold: any rate > 0.001 is red.
- **Problems it solves:**
  - Detect peers that are flooding the gossip protocol.
  - Identify potential DoS attempts or misbehaving nodes.
  - Verify that rate-limiting is engaging when expected.

#### Propagation Chunk Size Distribution
- **Metric:** `substrate_sync_propagated_statements_chunks` (histogram)
- **Type:** Histogram (displayed as p50/p90/p99)
- **What it measures:** Distribution of batch sizes when propagating statements to peers.
  Shows how many statements are batched per propagation cycle.
- **Why it matters:** Large chunks indicate efficient batching; very small chunks (1 statement)
  indicate high per-message overhead. Helps tune propagation batch parameters.
- **How to read:** Three lines (green=p50, yellow=p90, red=p99). Unit: statements per chunk.
- **Problems it solves:**
  - Evaluate propagation batching efficiency.
  - Detect changes in batching behavior across releases.

---

### Initial Sync

#### Initial Sync Active Peers
- **Metric:** `substrate_sync_initial_sync_peers_active`
- **Type:** Gauge
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
  - Help to understand initial sync's share of network traffic.

#### Initial Sync Burst Rate
- **Metric:** `rate(substrate_sync_initial_sync_bursts_total[$__rate_interval])`
- **Type:** Counter (displayed as rate)
- **What it measures:** Rate of initial sync burst rounds being processed.
- **Why it matters:** Each burst sends one batch of statements to one peer (round-robin);
  burst rate combined with statements-per-burst gives the effective sync throughput.
- **How to read:** Line chart showing bursts/sec. Legend shows mean/max.
- **Problems it solves:**
  - Verify that the round-robin is distributing work across peers.
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
