

This is the documentation for the repo `paritytech/cumulus`.

It contains 65 labels:

### A Labels: Action labels, used with GHA and trigger a certain process
- `A0-please_review`: *Pull request needs code review.*
- `A1-needs_burnin`: *Pull request needs to be tested on a live validator node before merge. DevOps is notified via matrix*
- `A2-insubstantial`: *Pull request requires no code review (e.g., a sub-repository hash update).*
- `A3-in_progress`: *Pull request is in progress. No review needed at this stage.*
- `A4-companion`: *A PR that should be considered alongside another (usually more comprehensive and detailed) PR.*
- `A5-stale`: *Pull request did not receive any updates in a long time. No review needed at this stage. Close it.*
- `A6-backport`: *Pull request is already reviewed well in another branch.*

### B labels: Release note labels, to be used in combination with a T* label
- `B0-silent`: *Changes should not be mentioned in any release notes*
- `B1-note_worthy`: *Changes should be noted in the release notes*

### C labels: Criticality - how critical is this change? Which impact does it have on the builders? To be used in combination with a T* label
- `C1-low`: *PR touches the given topic and has a low impact on builders.*
- `C3-medium`: *PR touches the given topic and has a medium impact on builders.*
- `C5-high`: *PR touches the given topic and has a high impact on builders.*
- `C7-critical`: *PR touches the given topic and has a critical impact on builders.*

### D labels: Auditing labels, optional for cumulus
- `D1-audited 👍`: *PR contains changes to fund-managing logic that has been properly reviewed and externally audited.*
- `D2-notlive 💤`: *PR contains changes in a runtime directory that is not deployed to a chain that requires an audit.*
- `D3-trivial 🧸`: *PR contains trivial changes in a runtime directory that do not require an audit*
- `D5-nicetohaveaudit ⚠️`: *PR contains trivial changes to logic that should be properly reviewed.*
- `D9-needsaudit 👮`: *PR contains changes to fund-managing logic that should be properly reviewed and externally audited*

### E labels: Upgrade dependencies
- `E0-runtime_migration`: *PR introduces code that might require downstream chains to run a runtime upgrade.*
- `E1-database_migration`: *PR introduces code that does a one-way migration of the database.*
- `E2-dependencies`: *Pull requests that update a dependency file.*
- `E3-host_functions`: *PR adds new host functions which requires a node release before a runtime upgrade.*
- `E4-node_first_update`: *This is a runtime change that will require all nodes to be update BEFORE the runtime upgrade.*

### F labels: Fail - change breaks some part of the code
- `F0-breaks_everything`: *This change breaks the underlying networking, sync or related and thus will cause a fork.*
- `F1-breaks_authoring`: *This change breaks authorities or authoring code.*
- `F2-breaks_consensus`: *This change breaks consensus or consensus code.*
- `F3-breaks_API`: *This PR changes public API; next release should be major.*

### I labels: Issue related labels
- `I0-consensus`: *Issue can lead to a consensus failure.*
- `I1-panic`: *The node panics and exits without proper error handling.*
- `I2-security`: *The node fails to follow expected, security-sensitive, behaviour.*
- `I3-bug`: *The node fails to follow expected behavior.*
- `I4-annoyance`: *The node behaves within expectations, however this “expected behaviour” itself is at issue.*
- `I5-tests`: *Tests need fixing, improving or augmenting.*
- `I6-documentation`: *Documentation needs fixing, improving or augmenting.*
- `I7-refactor`: *Code needs refactoring.*
- `I8-footprint`: *An enhancement to provide a smaller (system load, memory, network or disk) footprint.*
- `I9-optimisation`: *An enhancement to provide better overall performance in terms of time-to-completion for a task.*

### J labels: Just a continuation of the issue related labels
- `J0-enhancement`: *An additional feature request.*
- `J1-meta`: *A specific issue for grouping tasks or bugs of a specific category.*
- `J2-unconfirmed`: *Issue might be valid, but it's not yet known.*
- `J3-intended`: *Issue describes a behavior which turns out to work as intended. Closer should explain why.*
- `J4-duplicate`: *Issue is a duplicate. Closer should comment with a link to the duplicate.*
- `J5-wont_fix`: *Issue is in principle valid, but this project will not address it. Closer should explain why.*
- `J6-invalid`: *Issue is invalid. Closer should comment why.*

### S labels: Status of an issue
- `S0-design`: *Issue is in the design stage.*
- `S1-implement`: *Issue is in the implementation stage.*
- `S2-test/monitor`: *Issue is in the testing stage.*
- `S3-deploy`: *Issue is in the deployment stage*
- `S4-blocked`: *Issue is blocked, see comments for further information.*

### T labels: Topics - to be used in combination with other labels
- `T0-node`: *This PR/Issue is related to the topic “node”.*
- `T1-runtime`: *This PR/Issue is related to the topic “runtime”.*
- `T10-release`: *This PR/Issue is related to topics touching the release notes.*
- `T2-API`: *This PR/Issue is related to APIs.*
- `T3-relay_chain`: *This PR/Issue is related to the relay chain.*
- `T4-smart_contracts`: *This PR/Issue is related to smart contracts.*
- `T5-parachains`: *This PR/Issue is related to Parachains.*
- `T6-XCM`: *This PR/Issue is related to XCM.*
- `T7-statemint`: *This PR/Issue is related to Statemint*
- `T7-substrate`: *This is an issue that needs to be implemented upstream in Substrate.*
- `T8-CGP`: *This PR/Issue is related to Common Good Parachains.*

### U labels: Urgency - in what time manner does this issue need to be resolved?
- `U0-drop_everything`: *Everyone should address the issue now.*
- `U1-asap`: *No need to stop dead in your tracks, however issue should be addressed as soon as possible.*
- `U2-some_time_soon`: *Issue is worth doing soon.*
- `U3-nice_to_have`: *Issue is worth doing eventually.*
- `U4-some_day_maybe`: *Issue might be worth doing eventually.*
