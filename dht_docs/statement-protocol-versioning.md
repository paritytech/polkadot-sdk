# Statement protocol versioning (v1/v2)

How a node speaks both statement protocols during the transition
([[rollout-plan]]). Implemented in smoldot (`lib/src/network/codec.rs:163`,
`lib/src/network/service.rs:2511`); the full node registers the same pair of
notification protocols.

- Two notification protocols exist side by side, named
  `/<genesis-hash>/statement/1` and `/<genesis-hash>/statement/2` (fork-id
  aware).
- A node prefers **v2**: it attempts the v2 substream first, and only when the
  remote answers `ProtocolNotAvailable` does it retry on **v1**. The negotiated
  version is tracked per peer; filter exchange
  ([[bloom-filter-affinity]]) happens only on v2 connections.
- This gives per-connection graceful degradation without any network-wide
  coordination: v2-capable pairs use affinity routing, mixed pairs fall back to
  v1 flood semantics.

Relation to [[rollout-plan]]'s open question: this fallback is exactly the
transitional mechanism whose *removal* is unspecified — per-connection fallback
works for the migration window, but the design intends a coordinated switch
with no long-term v1 interop.
