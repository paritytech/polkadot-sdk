#!/usr/bin/env bash

# Set the new version by replacing the value of the constant given as pattern
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
    grep_filters=("runtime.*" "test|template|starters|substrate|docs")

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
    SUFFIX=${SUFFIX:-000} #this variable makes it possible to set a specific runtime version like 93826 it can be initialised as system variable
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

    printf "[+] ℹ️ Reordering prdocs:\n"

    VERSION=$(sed -E 's/^v([0-9]+\.[0-9]+\.[0-9]+).*$/\1/' <<< "$VERSION") #getting reed of the 'v' prefix

    # Check if there are any prdoc files to move
    if ls prdoc/pr_*.prdoc 1> /dev/null 2>&1; then
        mkdir -p "prdoc/$VERSION"
        mv prdoc/pr_*.prdoc prdoc/$VERSION
        git add -A
        commit_with_message "Reordering prdocs for the release $VERSION"
        echo "✅ Successfully reordered prdocs"
    else
        echo "⚠️ No prdoc files found to reorder"
    fi
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
    target=$3

    echo "Working on product:  $product "
    echo "Working on version:  $version "
    echo "Working on platform: $target "

    URL_BASE=$(get_s3_url_base $product)

    echo "Current content, should be empty on new uploads:"
    aws s3 ls "s3://${URL_BASE}/${version}/${target}" --recursive --human-readable --summarize || true
    echo "Content to be uploaded:"
    artifacts="release-artifacts/$target/$product/"
    ls "$artifacts"
    aws s3 sync --acl public-read "$artifacts" "s3://${URL_BASE}/${version}/${target}"
    echo "Uploaded files:"
    aws s3 ls "s3://${URL_BASE}/${version}/${target}" --recursive --human-readable --summarize
    echo "✅ The release should be at https://${URL_BASE}/${version}/${target}"
}

# Upload runtimes artifacts to s3 release bucket
#
# input: version (stable release tage.g. polkadot-stable2412 or polkadot-stable2412-rc1)
# output: none
upload_s3_runtimes_release_artifacts() {
  alias aws='podman run --rm -it docker.io/paritytech/awscli -e AWS_ACCESS_KEY_ID -e AWS_SECRET_ACCESS_KEY -e AWS_BUCKET aws'

  version=$1

  echo "Working on version: $version "

  echo "Current content, should be empty on new uploads:"
  aws s3 ls "s3://releases.parity.io/polkadot/runtimes/${version}/" --recursive --human-readable --summarize || true
  echo "Content to be uploaded:"
  artifacts="artifacts/runtimes/"
  ls "$artifacts"
  aws s3 sync --acl public-read "$artifacts" "s3://releases.parity.io/polkadot/runtimes/${version}/"
  echo "Uploaded files:"
  aws s3 ls "s3://releases.parity.io/polkadot/runtimes/${version}/" --recursive --human-readable --summarize
  echo "✅ The release should be at https://releases.parity.io/polkadot/runtimes/${version}"
}


# Pass the name of the binary as input, it will
# return the s3 base url
function get_s3_url_base() {
    name=$1
    case $name in
      polkadot | polkadot-execute-worker | polkadot-prepare-worker )
        printf "releases.parity.io/polkadot"
        ;;

      polkadot-parachain)
        printf "releases.parity.io/polkadot-parachain"
        ;;

      polkadot-omni-node)
        printf "releases.parity.io/polkadot-omni-node"
        ;;

      chain-spec-builder)
        printf "releases.parity.io/chain-spec-builder"
        ;;

      frame-omni-bencher)
        printf "releases.parity.io/frame-omni-bencher"
        ;;
      *)
        printf "UNSUPPORTED BINARY $name"
        exit 1
        ;;
    esac
}

# Bump spec_version in a runtime file based on release type
# For patch release: bump last 3 digits (patch part) by 1
# For new stable release: bump middle part (minor) by 1, reset patch to 0
#
# input:
#   - file: path to the runtime file
#   - is_patch_release: "true" for patch release, "false" for new stable
# output: prints the new spec_version, modifies file in place
bump_spec_version() {
    local file=$1
    local is_patch_release=$2

    # Extract current spec_version from file (format: X_YYY_ZZZ)
    local current_spec=$(grep -oP 'spec_version:\s*\K[0-9]+_[0-9]+_[0-9]+' "$file" | head -1)

    if [ -z "$current_spec" ]; then
        echo "⚠️  Warning: Could not find spec_version in $file"
        return 1
    fi

    # Parse the spec_version (format: X_YYY_ZZZ)
    local major=$(echo "$current_spec" | cut -d'_' -f1)
    local minor=$(echo "$current_spec" | cut -d'_' -f2)
    local patch=$(echo "$current_spec" | cut -d'_' -f3)

    # Remove leading zeros for arithmetic
    minor=$((10#$minor))
    patch=$((10#$patch))

    if [ "$is_patch_release" = "true" ]; then
        # Patch release: bump patch part by 1
        patch=$((patch + 1))
    else
        # New stable release: bump minor by 1, reset patch to 0
        minor=$((minor + 1))
        patch=0
    fi

    # Format back to X_YYY_ZZZ format (with proper zero padding)
    local new_spec=$(printf "%d_%03d_%03d" "$major" "$minor" "$patch")

    # Replace in file
    sed -ri "s/spec_version: ${current_spec},/spec_version: ${new_spec},/" "$file"

    echo "$new_spec"
}
