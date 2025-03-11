# Scheduler Pallet

Based on the scheduler pallet's usage in the Polkadot/Kusama runtime, it primarily contains two types of tasks:
1. Tasks from passed referendums
2. Service tasks from the referendum pallet, specifically `nudge_referendum` and `refund_submission_deposit`

We plan to map all calls that are used in the Governance by inspecting the production snapshots.

During the migration process, we will disable the processing of scheduled tasks on both the Relay Chain and Asset Hub. This is achieved by setting the `MaximumWeight` parameter to zero for the scheduler using the `rc_pallet_migrator::types::ZeroWeightOr` helper type. Once the migration is complete, any tasks that are due for execution on Asset Hub will be processed, even if they are delayed. This behavior is appropriate for both types of tasks we handle.
