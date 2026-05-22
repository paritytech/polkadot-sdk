(function() {
    var type_impls = Object.fromEntries([["pallet_grandpa",[]],["polkadot_sdk_frame",[]],["sc_consensus_grandpa",[]],["yet_another_parachain_runtime",[]]]);
    if (window.register_type_impls) {
        window.register_type_impls(type_impls);
    } else {
        window.pending_type_impls = type_impls;
    }
})()
//{"start":55,"fragment_lengths":[21,26,28,37]}