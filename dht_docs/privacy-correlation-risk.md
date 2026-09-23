# Privacy: correlation risk

§3.7.2. If a node connects to *all* k-closest DHT-affine peers for a topic and
uses a wide (low-false-positive) bloom filter ([[bloom-filter-affinity]]), those
peers can correlate the connection pattern with the filter and infer the node's
exact topic interest.

Mitigation, trading delivery reliability for privacy:
- connect to only a **subset** of the DHT-affine peers for a topic, and
- use a **narrower** filter (more false positives) toward peers in the same
  topic neighbourhood.
