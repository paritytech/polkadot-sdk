# Duplicate tracking

§3.4.4 (same as v1). Each node keeps a per-peer LRU cache of statement hashes and
never sends a statement a peer has already seen. This is what terminates
propagation ([[propagation-termination]]).

If a peer sends us a duplicate, its reputation is reduced and it may be
disconnected or banned — a signal that also feeds [[peer-steering]].
