TODOs:
- Fix the activation/deactivation of total issuance for teleported and reserve-transferred native 
assets to/from Asset Hub (https://github.com/paritytech/polkadot-sdk/issues/8055). After migration,
deactivate the correct issuance on Asset Hub.
- Consider a dedicated migration stage for updating the teleport/reserve location, adjusting total
issuance and checking account balances. This approach prevents XCM teleport locking during the 
entire migration and requires only a two-block lock for the switch.