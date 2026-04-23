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

  service do
    run [opt_bin/"barrs", "run"]
    run_type :immediate
    log_path var/"log/barrs.log"
    error_log_path var/"log/barrs.log"
  end

  def caveats
    <<~EOS
      A sample configuration was installed to:
        #{pkgshare}/barrs.lua

      barrs writes its default config to:
        ~/.config/barrs/barrs.lua

      Start it as a launchd service with:
        brew services start barrs

      Or run it manually with:
        barrs start
    EOS
  end

  test do
    assert_match "Usage:", shell_output("#{bin}/barrs --help")
  end
end
EOF
