class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.11"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.11/barrs-v0.1.11-aarch64-apple-darwin.tar.gz"
    sha256 "e6eaeae64fb92f6d8d08d50403fc180118c519734de1e18c3fd5501bb1febb10"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.11/barrs-v0.1.11-x86_64-apple-darwin.tar.gz"
    sha256 "3b2625df732eb8c8a4dc344c794a4554c7e901f3d6ee4400a1e0b05569f1f2c1"
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
