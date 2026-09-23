# Premium prioritisation

§3.3.4. A node operator can offer **paid retention**: a client pays the node to
retain and serve statements for chosen topics or accounts at higher priority
than unpaid traffic (payment mechanics are out of scope).

No protocol change is needed — the node simply adds the paid topics to its
[[explicit-affinity]] set, which the protocol treats like any other affinity. To
cover a whole account, it can advertise affinity for all of that account's
topics.
