(function() {
    var type_impls = Object.fromEntries([["pallet_bridge_beefy",[]],["pallet_bridge_grandpa",[]]]);
    if (window.register_type_impls) {
        window.register_type_impls(type_impls);
    } else {
        window.pending_type_impls = type_impls;
    }
})()
//{"start":55,"fragment_lengths":[26,29]}