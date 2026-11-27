#!/usr/bin/env bash
set -euo pipefail

# musl + rustls 静态构建：产物不依赖 glibc、libssl，可在任意 x86-64 Linux 上运行。
# 需要：rustup target add x86_64-unknown-linux-musl、apt-get install musl-tools lld
# 参考开发说明 §7.1。链接器必须使用 ld.lld，binutils ld 链接出的 static-pie 会启动即崩。

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
output_dir="${project_dir}/dist"
output_file="${output_dir}/ncnc"
target="x86_64-unknown-linux-musl"

cleanup() {
    cargo clean --manifest-path "${project_dir}/Cargo.toml" --target "${target}"
}
trap cleanup EXIT

export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-musl-gcc}"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-musl-gcc}"
export RUSTFLAGS="-C linker=ld.lld -C target-feature=+crt-static"

mkdir -p "${output_dir}"
cargo build --manifest-path "${project_dir}/Cargo.toml" --release --locked --target "${target}"
install -m 0755 "${project_dir}/target/${target}/release/ncnc" "${output_file}"

file "${output_file}"
printf 'Release binary: %s\n' "${output_file}"
du -h "${output_file}"
