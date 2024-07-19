import subprocess, sys

# Get all crates
output = subprocess.check_output(["cargo", "tree", "--locked", "--workspace", "--depth", "0", "--prefix", "none"])

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

part = crates[:len(crates)//6]

for pkg in part:
    print ('+package('+pkg+') ')