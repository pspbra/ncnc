#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
output_dir="${project_dir}/dist"
output_file="${output_dir}/ncnc"

cleanup() {
    cargo clean --manifest-path "${project_dir}/Cargo.toml"
}
trap cleanup EXIT

mkdir -p "${output_dir}"
cargo build --manifest-path "${project_dir}/Cargo.toml" --release --locked
install -m 0755 "${project_dir}/target/release/ncnc" "${output_file}"

printf 'Release binary: %s\n' "${output_file}"
du -h "${output_file}"
