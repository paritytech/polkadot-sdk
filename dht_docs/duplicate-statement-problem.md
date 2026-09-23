# Duplicate statement problem

§3.9.1. Explicit-affinity and light nodes connect to multiple peers for
reliability and censorship resistance. With overlapping topic affinity, a single
statement may arrive once from *each* such peer — up to P times for P peers —
wasting bandwidth proportional to the redundancy.

An inherent trade-off: more connections improve delivery and resilience but
increase duplicate traffic. [[set-reconciliation]] is the proposed mitigation.
