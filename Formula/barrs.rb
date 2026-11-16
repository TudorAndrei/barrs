class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.4.0"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.4.0/barrs-v0.4.0-aarch64-apple-darwin.tar.gz"
    sha256 "d43230e9bea8b8d4b820804bcbb18df78fb6ffc100b413edf5bde2d83e0d9c22"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.4.0/barrs-v0.4.0-x86_64-apple-darwin.tar.gz"
    sha256 "7a366fdfe97dd4f7078daf5218bc03278630a5e18852b0cebdaa2a58c45f991a"
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
