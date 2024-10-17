#!/usr/bin/env bash

# Set the new version by replacing the value of the constant given as patetrn
# in the file.
#
# input: pattern, version, file
#output: none
set_version() {
    pattern=$1
    version=$2
    file=$3

    sed -i "s/$pattern/\1\"${version}\"/g" $file
    return 0
}

# Commit changes to git with specific message.
# "|| true" does not let script to fail with exit code 1,
# in case there is nothing to commit.
#
# input: MESSAGE (any message which should be used for the commit)
# output: none
commit_with_message() {
    MESSAGE=$1
    git commit -a -m "$MESSAGE" || true
}

# Retun list of the runtimes filterd
# input: none
# output: list of filtered runtimes
get_filtered_runtimes_list() {
    grep_filters=("runtime.*" "test|template|starters|substrate")

    git grep spec_version: | grep .rs: | grep -e "${grep_filters[0]}" | grep "lib.rs" | grep -vE "${grep_filters[1]}" | cut -d: -f1
}

# Sets provided spec version
# input: version
set_spec_versions() {
  NEW_VERSION=$1
  runtimes_list=(${@:2})

  printf "Setting spec_version to $NEW_VERSION\n"

  for f in ${runtimes_list[@]}; do
      printf "  processing $f"
      sed -ri "s/spec_version: [0-9]+_[0-9]+_[0-9]+,/spec_version: $NEW_VERSION,/" $f
  done

  commit_with_message "Bump spec_version to $NEW_VERSION"

  git_show_log 'spec_version'
}

# Displays formated results of the git log command
# for the given pattern which needs to be found in logs
# input: pattern, count (optional, default is 10)
git_show_log() {
  PATTERN="$1"
  COUNT=${2:-10}
  git log --pretty=format:"%h %ad | %s%d [%an]" --graph --date=iso-strict | \
      head -n $COUNT | grep -iE "$PATTERN" --color=always -z
}

# Get a spec_version number from the crate version
#
# ## inputs
#  - v1.12.0 or 1.12.0
#
# ## output:
# 1_012_000 or 1_012_001 if SUFFIX is set
function get_spec_version() {
    INPUT=$1
    SUFFIX=${SUFFIX:-000} #this variable makes it possible to set a specific ruuntime version like 93826 it can be intialised as sestem variable
    [[ $INPUT =~ .*([0-9]+\.[0-9]+\.[0-9]{1,2}).* ]]
    VERSION="${BASH_REMATCH[1]}"
    MATCH="${BASH_REMATCH[0]}"
    if [ -z $MATCH ]; then
        return 1
    else
        SPEC_VERSION="$(sed -e "s/\./_0/g" -e "s/_[^_]*\$/_$SUFFIX/" <<< $VERSION)"
        echo "$SPEC_VERSION"
        return 0
    fi
}

# Reorganize the prdoc files for the release
#
# input: VERSION (e.g. v1.0.0)
# output: none
reorder_prdocs() {
    VERSION="$1"

    printf "[+] ℹ️ Reordering prdocs:"

    VERSION=$(sed -E 's/^v([0-9]+\.[0-9]+\.[0-9]+).*$/\1/' <<< "$VERSION") #getting reed of the 'v' prefix
    mkdir -p "prdoc/$VERSION"
    mv prdoc/pr_*.prdoc prdoc/$VERSION
    git add -A
    commit_with_message "Reordering prdocs for the release $VERSION"
}

# Bump the binary version of the polkadot-parachain binary with the
# new bumped version and commit changes.
#
# input: version e.g. 1.16.0
set_polkadot_parachain_binary_version() {
    bumped_version="$1"
    cargo_toml_file="$2"

    set_version "\(^version = \)\".*\"" $bumped_version $cargo_toml_file

    cargo update --workspace --offline # we need this to update Cargo.loc with the new versions as well

    MESSAGE="Bump versions in: ${cargo_toml_file}"
    commit_with_message "$MESSAGE"
    git_show_log "$MESSAGE"
}


upload_s3_release() {
  alias aws='podman run --rm -it docker.io/paritytech/awscli -e AWS_ACCESS_KEY_ID -e AWS_SECRET_ACCESS_KEY -e AWS_BUCKET aws'

  product=$1
  version=$2

  echo "Working on product: $product "
  echo "Working on version: $version "

  echo "Current content, should be empty on new uploads:"
  aws s3 ls "s3://releases.parity.io/polkadot/${version}/" --recursive --human-readable --summarize || true
  echo "Content to be uploaded:"
  artifacts="artifacts/$product/"
  ls "$artifacts"
  aws s3 sync --acl public-read "$artifacts" "s3://releases.parity.io/polkadot/${version}/"
  echo "Uploaded files:"
  aws s3 ls "s3://releases.parity.io/polkadot/${version}/" --recursive --human-readable --summarize
  echo "✅ The release should be at https://releases.parity.io/polkadot/${version}"
}
