import subprocess, sys

# Get all crates
output = subprocess.check_output(["cargo", "tree", "--locked", "--workspace", "--depth", "0", "--prefix", "none", "--features", "try-runtime,ci-only-tests,experimental,riscv"])

# Convert the output into a proper list
crates = []
for line in output.splitlines():
	if line != b"":
		line = line.decode('utf8').split(" ")
		crate_name = line[0]
		# The crate path is always the last element in the line.
		crate_path = line[len(line) - 1].replace("(", "").replace(")", "")
		crates.append(crate_name)

# Make the list unique and sorted
crates = list(set(crates))
crates.sort()

#print(f'total crates: {len(crates)}')

#
current_group = int(sys.argv[1]) - 1
total_groups = int(sys.argv[2])

cratesPerGroup = len(crates) // total_groups

if current_group >= total_groups:
	print("`current group` is greater than `total groups`")
	sys.exit(1)

#print(f'group {current_group+1}/{total_groups}, {cratesPerGroup} crates per group')

#
start = cratesPerGroup * current_group
end = cratesPerGroup * (current_group + 1)

if current_group + 1 == total_groups:
	end = len(crates)

#
part = crates[start : end]

result = 'package('+part[0]+')'
for pkg in part[1:]:
    result += ' + package('+pkg+')'

print(result)