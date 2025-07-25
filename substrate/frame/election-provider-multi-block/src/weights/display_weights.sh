
function display {
  echo "displaying $1"
  subweight compare files \
    --method asymptotic \
    --new $1 \
    --old $1 \
    --unit proof \
    --verbose \
    --threshold 0

  subweight compare files \
    --method asymptotic \
    --new $1 \
    --old $1 \
    --unit time \
    --verbose \
    --threshold 0
}

## Polkadot

display "pallet_election_provider_multi_block_dot_size.rs"
display "pallet_election_provider_multi_block_signed_dot_size.rs"
display "pallet_election_provider_multi_block_unsigned_dot_size.rs"
display "pallet_election_provider_multi_block_verifier_dot_size.rs"

## Kusama
display "pallet_election_provider_multi_block_ksm_size.rs"
display "pallet_election_provider_multi_block_signed_ksm_size.rs"
display "pallet_election_provider_multi_block_unsigned_ksm_size.rs"
display "pallet_election_provider_multi_block_verifier_ksm_size.rs"
