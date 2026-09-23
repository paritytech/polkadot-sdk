# Periodic propagation

§3.4.5 (same as v1). On a regular interval the node collects recently added
statements and re-runs the [[forwarding-decision]] against each peer.

This covers peers that connected *after* a statement first arrived, so they
still receive statements they have affinity for.
