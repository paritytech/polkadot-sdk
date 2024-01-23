import math

MAX_VALIDATOR_COUNT = 200000
MIN_REQUIRED_SIGNATURES = 10

curve = [(x, math.ceil(math.log2(3 * x))) for x in range(1, MAX_VALIDATOR_COUNT + 1)]

output = []
cursor = curve[0][1]

for (x, y) in curve:
    if y < MIN_REQUIRED_SIGNATURES:
        continue
    if y > cursor:
        cursor = y
        output.append((x, y))

print("x\ty")
for (x, y) in output:
    print(f"{x}\t{y}")
