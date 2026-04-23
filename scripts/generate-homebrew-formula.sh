#!/usr/bin/env bash

set -euo pipefail

version="$1"
repository="$2"
arm_archive="$3"
arm_sha="$4"
intel_archive="$5"
intel_sha="$6"

cat <<EOF
class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/${repository}"
  version "${version}"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/${repository}/releases/download/v${version}/${arm_archive}"
    sha256 "${arm_sha}"
  else
    url "https://github.com/${repository}/releases/download/v${version}/${intel_archive}"
    sha256 "${intel_sha}"
  end

  def install
    bin.install "barrs"
    pkgshare.install "barrs.lua"
  end

  def caveats
    <<~EOS
      A sample configuration was installed to:
        #{pkgshare}/barrs.lua

      Start the native renderer with:
        barrs start --renderer native --config #{pkgshare}/barrs.lua
    EOS
  end

  test do
    assert_match "Usage:", shell_output("#{bin}/barrs --help")
  end
end
EOF
