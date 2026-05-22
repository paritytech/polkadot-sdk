(function() {
    var type_impls = Object.fromEntries([["pallet_babe",[]],["sc_consensus_babe",[]],["yet_another_parachain_runtime",[]]]);
    if (window.register_type_impls) {
        window.register_type_impls(type_impls);
    } else {
        window.pending_type_impls = type_impls;
    }
})()
//{"start":55,"fragment_lengths":[18,25,37]}