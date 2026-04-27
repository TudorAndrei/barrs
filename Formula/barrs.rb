class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.6"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.6/barrs-v0.1.6-aarch64-apple-darwin.tar.gz"
    sha256 "6243c64e3a076a0e2364151cf0117428a29bccb58cd2a8a13f7178c2c11ca3d3"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.6/barrs-v0.1.6-x86_64-apple-darwin.tar.gz"
    sha256 "e3c8387559b8ea101dcc7246387fcc31eb04d0641fa1ca4f1808403e20d40ebc"
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
