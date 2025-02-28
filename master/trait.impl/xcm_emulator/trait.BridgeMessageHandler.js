(function() {
    var implementors = Object.fromEntries([["emulated_integration_tests_common",[["impl&lt;S, SI, T, TI&gt; <a class=\"trait\" href=\"emulated_integration_tests_common/impls/trait.BridgeMessageHandler.html\" title=\"trait emulated_integration_tests_common::impls::BridgeMessageHandler\">BridgeMessageHandler</a> for <a class=\"struct\" href=\"emulated_integration_tests_common/impls/struct.BridgeHubMessageHandler.html\" title=\"struct emulated_integration_tests_common::impls::BridgeHubMessageHandler\">BridgeHubMessageHandler</a>&lt;S, SI, T, TI&gt;<div class=\"where\">where\n    S: BridgeMessagesConfig&lt;SI&gt;,\n    SI: 'static,\n    T: BridgeMessagesConfig&lt;TI&gt;,\n    TI: 'static,\n    &lt;T as BridgeMessagesConfig&lt;TI&gt;&gt;::InboundPayload: <a class=\"trait\" href=\"https://doc.rust-lang.org/1.84.1/core/convert/trait.From.html\" title=\"trait core::convert::From\">From</a>&lt;<a class=\"struct\" href=\"https://doc.rust-lang.org/1.84.1/alloc/vec/struct.Vec.html\" title=\"struct alloc::vec::Vec\">Vec</a>&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/1.84.1/std/primitive.u8.html\">u8</a>&gt;&gt;,\n    &lt;T as BridgeMessagesConfig&lt;TI&gt;&gt;::MessageDispatch: MessageDispatch&lt;DispatchLevelResult = XcmBlobMessageDispatchResult&gt;,</div>"]]],["xcm_emulator",[]]]);
    if (window.register_implementors) {
        window.register_implementors(implementors);
    } else {
        window.pending_implementors = implementors;
    }
})()
//{"start":57,"fragment_lengths":[1241,20]}