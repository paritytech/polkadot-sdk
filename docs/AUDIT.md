# Audit

Audits are conducted to ensure the absence of severe or exploitable bugs. Pull Requests are generally merged into the
`master` branch without audit. The `audited` tag is used to track the latest audited commit of the `master` branch. This
means that audits need to happen in order of being merged.  
This is an optimistic approach that lets us develop with greater speed, while requiring (possibly) large refactors in
the failure case.

Audits can be deferred if the logic is gated by an `experimental` feature or marked as "Not Production Ready" within the
first line of doc. Such changes should be queued manually before these warnings are removed.

## General Guidelines for what to Audit

There is no single one-fits-all rule. Generally we should audit important logic that could immediately be used on
production networks. If in doubt, ask in chat or in the Merge Request.

## Requesting an Audit

1. Add the PR to the project `Security Audit (PRs) - SRLabs`
2. Set status to Backlog
3. Assign priority, considering the universe of PRs currently in the backlog
4. Add the component
