# Approval voting parallel

The approval-voting-parallel subsystem acts as an orchestrator for the tasks handled by the [Approval Voting](approval-voting.md)
and [Approval Distribution](approval-distribution.md) subsystems. Initially, these two systems operated separately and interacted
with each other and other subsystems through orchestra.

With approval-voting-parallel, we have a single subsystem that creates two types of workers:
- Four approval-distribution workers that operate in parallel, each handling tasks based on the validator_index of the message
  originator.
- One approval-voting worker that performs the tasks previously managed by the standalone approval-voting subsystem.

This subsystem does not maintain any state. Instead, it functions as an orchestrator that:
- Spawns and initializes each workers.
- Forwards each message and signal to the appropriate worker.
- Aggregates results for messages that require input from more than one worker, such as GetApprovalSignatures.

## Forwarding logic

The messages received and forwarded by approval-voting-parallel split in three categories:
- Signals which need to be forwarded to all workers.
- Messages that only the `approval-voting` worker needs to handle, `ApprovalVotingParallelMessage::ApprovedAncestor`
  and   `ApprovalVotingParallelMessage::GetApprovalSignaturesForCandidate`
- Control messages  that all `approval-distribution` workers need to receive `ApprovalVotingParallelMessage::NewBlocks`,
  `ApprovalVotingParallelMessage::ApprovalCheckingLagUpdate`  and all network bridge variants `ApprovalVotingParallelMessage::NetworkBridgeUpdate`
  except `ApprovalVotingParallelMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage)`
- Data messages `ApprovalVotingParallelMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage)`  which need to be sent
  just to a single `approval-distribution`  worker based on the ValidatorIndex. The logic for assigning the work is:
  ```
  assigned_worker_index = validator_index % number_of_workers;
  ```
