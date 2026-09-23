# Peer steering

Issue #11935 / §3.3.3. A node continuously scores how useful each connected peer
is and steers its peer set toward the best ones: periodically disconnect the
worst performers and connect to new candidates, converging on a good DHT
neighbourhood and good explicit-affinity providers.

Signals to score on:
- peers likely to share our DHT affinity (fast convergence),
- peers that deliver statements matching our explicit affinity,
- lower-latency peers (same statements arrive sooner),
- peers that keep us well-connected (fewer hops for any statement).

Drives the connect/disconnect decisions executed by the [[orchestrator]], within
the connected-set cap ([[peer-set-eviction]]). Steering only churns the
**non-reserved** connection slots; the DHT-closest reserved peers are kept
regardless ([[peer-connection-model]]).

## Open questions (white-space)
The scoring algorithm and metric weights are intentionally left "flexible and
configurable" — no concrete formula, decay, or disconnect cadence is specified.
