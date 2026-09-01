// Quint `run` tests. Driven via `quint test --backend=typescript tests.qnt`.
//
// This module is a thin aggregator. Every test lives in a topic module under
// `tests/`, one per design-doc section, and importing them here runs the whole
// suite under a single `quint test tests.qnt` invocation. Shared deterministic
// helpers and fixtures live in `tests/support.qnt`.
module tests {
  import transfers_test.* from "./tests/transfers_test"
  import transfer_drain_test.* from "./tests/transfer_drain_test"
  import accumulate_test.* from "./tests/accumulate_test"
  import head_commitment_test.* from "./tests/head_commitment_test"
  import parachain_log_test.* from "./tests/parachain_log_test"
  import code_upgrades_test.* from "./tests/code_upgrades_test"
  import state_balance_test.* from "./tests/state_balance_test"
  import management_test.* from "./tests/management_test"
  import service_ops_test.* from "./tests/service_ops_test"
  import refine_test.* from "./tests/refine_test"
  import authorizer_queue_test.* from "./tests/authorizer_queue_test"
  import spec_msg_test.* from "./tests/spec_msg_test"
}
