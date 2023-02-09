Description: PoV recovery test
Network: ./0002-pov_recovery.toml
Creds: config


validator-0: is up
validator-1: is up
validator-2: is up
validator-3: is up
alice: is up within 60 seconds
bob: is up within 60 seconds
charlie: is up within 60 seconds
one: is up within 60 seconds
two: is up within 60 seconds
eve: is up within 60 seconds

# wait 20 blocks and register parachain
validator-3: reports block height is at least 20 within 250 seconds
validator-0: js-script ./register-para.js with "2000" within 240 seconds
validator-0: parachain 2000 is registered within 300 seconds

# check block production
bob: reports block height is at least 20 within 600 seconds
alice: reports block height is at least 20 within 600 seconds
charlie: reports block height is at least 20 within 600 seconds
one: reports block height is at least 20 within 800 seconds
two: reports block height is at least 20 within 800 seconds
eve: reports block height is at least 20 within 800 seconds
