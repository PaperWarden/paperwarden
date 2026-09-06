#!/usr/bin/env bash
set -euo pipefail

readonly flutter_version="3.47.2"
readonly flutter_commit="d3b14c876900e553bc736ca19295fc09e3853e8e"
readonly flutter_dir="${RUNNER_TEMP}/flutter"

git clone --branch "${flutter_version}" --depth 1 \
  https://github.com/flutter/flutter.git "${flutter_dir}"

actual_commit="$(git -C "${flutter_dir}" rev-parse HEAD)"
if [[ "${actual_commit}" != "${flutter_commit}" ]]; then
  echo "Flutter SDK integrity check failed: expected ${flutter_commit}, got ${actual_commit}" >&2
  exit 1
fi

echo "${flutter_dir}/bin" >> "${GITHUB_PATH}"
