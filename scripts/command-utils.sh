#!/usr/bin/env bash

if [ "${LOADED_UTILS_SH:-}" ]; then
  return
else
  export LOADED_UTILS_SH=true
fi

export ARTIFACTS_DIR="$PWD/.git/.artifacts"

die() {
  if [ "${1:-}" ]; then
    >&2 echo "$1"
  fi
  exit 1
}

get_arg() {
  local arg_type="$1"
  shift

  local is_required
  case "$arg_type" in
    required|required-many)
      is_required=true
    ;;
    optional|optional-many) ;;
    *)
      die "Invalid is_required argument \"$2\" in get_arg"
    ;;
  esac

  local has_many_values
  if [ "${arg_type: -6}" == "-many" ]; then
    has_many_values=true
  fi

  local option_arg="$1"
  shift

  local args=("$@")

  unset out
  out=()

  local get_next_arg
  for arg in "${args[@]}"; do
    if [ "${get_next_arg:-}" ]; then
      out+=("$arg")
      unset get_next_arg
      if [ ! "${has_many_values:-}" ]; then
        break
      fi
    # --foo=bar (get the value after '=')
    elif [ "${arg:0:$(( ${#option_arg} + 1 ))}" == "$option_arg=" ]; then
      out+=("${arg:$(( ${#option_arg} + 1 ))}")
      if [ ! "${has_many_values:-}" ]; then
        break
      fi
    # --foo bar (get the next argument)
    elif [ "$arg" == "$option_arg" ]; then
      get_next_arg=true
    fi
  done

  # arg list ended with --something but no argument was provided next
  if [ "${get_next_arg:-}" ]; then
    die "Expected argument after \"${args[-1]}"\"
  fi

  if [ "${out[0]:-}" ]; then
    if [ ! "${has_many_values:-}" ]; then
      out="${out[0]}"
    fi
  elif [ "${is_required:-}" ]; then
    die "Argument $option_arg is required, but was not found"
  else
    unset out
  fi
}
