use crate::ah::mock::*;

// Tests that are specific to Asset Hub.

#[test]
fn on_receive_session_report() {
    // todo(ank4n):
    // Tests
    // - receives and accumulates validator era points.
    // - initiate election prep at start session 5.
    // - send new validator set to rc.
    ExtBuilder::default().local_queue().build().execute_with(|| {})

}

#[test]
fn on_new_offence() {
    // todo(ank4n):
    // Tests processing of offence and slashing
    ExtBuilder::default().local_queue().build().execute_with(|| {});
}
