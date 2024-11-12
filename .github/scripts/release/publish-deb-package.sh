#!/bin/sh
set -e
# sample use for polkadot:
# ./add-packages.sh release polkadot 0.9.15

repo="$1"
package="$2"
version="$3"
local_deb_repo_path="$4"

aws_deb_path="s3://releases-package-repos/deb"

# Add a deb to our apt repo
add_deb(){
  #alias aws='podman run --rm -it -e AWS_ACCESS_KEY_ID=${AWS_RELEASE_ACCESS_KEY_ID} -e AWS_SECRET_ACCESS_KEY=${AWS_RELEASE_SECRET_ACCESS_KEY} -e AWS_BUCKET docker.io/paritytech/awscli aws'
  export AWS_ACCESS_KEY_ID=${AWS_RELEASE_ACCESS_KEY_ID}
  export AWS_SECRET_ACCESS_KEY=${AWS_RELEASE_SECRET_ACCESS_KEY}

  # Download the current state of the deb repo
  aws s3 sync "$aws_deb_path/db" "$local_deb_repo_path/db"
  aws s3 sync "$aws_deb_path/pool" "$local_deb_repo_path/pool"
  aws s3 sync "$aws_deb_path/dists" "$local_deb_repo_path/dists"

  # Add the new deb to the repo
  reprepro -b "$local_deb_repo_path" includedeb "$repo" "$binpath/$debname"

  # Upload the updated repo - dists and pool should be publicly readable
  aws s3 sync "$local_deb_repo_path/pool" "$aws_deb_path/pool" --acl public-read
  aws s3 sync "$local_deb_repo_path/dists" "$aws_deb_path/dists" --acl public-read
  aws s3 sync "$local_deb_repo_path/db" "$aws_deb_path/db"
  aws s3 sync "$local_deb_repo_path/conf" "$aws_deb_path/conf"

  # Invalidate caches to make sure latest files are served
  aws cloudfront create-invalidation --distribution-id E36FKEYWDXAZYJ --paths '/deb/*'
}

# Add a deb to our apt repo using docker
add_deb_docker(){
  alias aws='podman run --rm -it docker.io/paritytech/awscli -e AWS_ACCESS_KEY_ID -e AWS_SECRET_ACCESS_KEY -e AWS_BUCKET aws'

  # Download the current state of the deb repo
  aws s3 sync "$aws_deb_path/db" "$local_deb_repo_path/db"
  aws s3 sync "$aws_deb_path/pool" "$local_deb_repo_path/pool"
  aws s3 sync "$aws_deb_path/dists" "$local_deb_repo_path/dists"

  # Add the new deb to the repo
  podman run --rm -it \
    -v "/run/user/$(id -u)/gnupg/S.gpg-agent:/home/nonroot/.gnupg/S.gpg-agent" \
    -v "${HOME}/${local_deb_repo_path}:/home/nonroot/${local_deb_repo_path}" \
    -v "${binpath}/${debname}:/home/nonroot/${debname}" \
    docker.io/paritytech/deb reprepro -b "${local_deb_repo_path}" includedeb "${repo}" "${debname}"

  # Upload the updated repo - dists and pool should be publicly readable
  aws s3 sync "$local_deb_repo_path/pool" "$aws_deb_path/pool" --acl public-read
  aws s3 sync "$local_deb_repo_path/dists" "$aws_deb_path/dists" --acl public-read
  aws s3 sync "$local_deb_repo_path/db" "$aws_deb_path/db"
  aws s3 sync "$local_deb_repo_path/conf" "$aws_deb_path/conf"

  # Invalidate caches to make sure latest files are served
  aws cloudfront create-invalidation --distribution-id E36FKEYWDXAZYJ --paths '/deb/*'
}

case "$package" in
  "polkadot")
    # Fetch the polkadot deb file
    debname="polkadot_${version}_amd64.deb"
    binpath="release-artifacts"
    add_deb
    ;;
  "parity-keyring")
  #   debname="parity-keyring_${version}_all.deb"
  #   binpath="/home/parity-keyring/parity-keyring/build/"
  #   add_deb
    ;;
esac
