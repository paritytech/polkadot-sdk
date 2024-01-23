#!/bin/env python3

from pprint import pprint
from math import (ceil, log2)

# Implements the signature sampling described in https://hackmd.io/9OedC7icR5m-in_moUZ_WQ

# The complete samples method.
def samples(ratio_per_validator, validators_length, slash_rate, randao_commit_expiry, signature_use_count):
    randao_biasability = 172.8 * (74+1+randao_commit_expiry) # Based on markov chain analysis

    result = ceil(log2(
        ratio_per_validator * (1/slash_rate) * randao_biasability
    ))

    result += ceil(log2(validators_length));

    result += 1 + (2 * ceil(log2(signature_use_count)) if signature_use_count > 1 else 0)

    return result

# The samples method that we use to get the minimum signatures. Run off-chain and set at beefy client initialization.
def samples_static(ratio_per_validator, slash_rate, randao_commit_expiry):
    randao_biasability = 172.8 * (74+1+randao_commit_expiry) # Based on markov chain analysis

    result = ceil(log2(
        ratio_per_validator * (1/slash_rate) * randao_biasability
    ))

    return result

# The samples method that we used to get the dynamic signatures. Run on-chain.
def samples_dynamic(validators_length, signature_use_count):
    result = ceil(log2(validators_length)) + 1 + (2 * ceil(log2(signature_use_count)) if signature_use_count > 1 else 0)

    return result

r=2.5
vlen=300
sr=0.25
re=3

pprint([(i, samples(r, vlen, sr, re, i), samples_static(r, sr, re), samples_dynamic(vlen, i)) for i in range(0,2**16)])

