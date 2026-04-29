class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.8"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.8/barrs-v0.1.8-aarch64-apple-darwin.tar.gz"
    sha256 "67d4adba4d8f234a57bf052bca3ed23774810c068e95e90e3eb203996dcc14aa"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.8/barrs-v0.1.8-x86_64-apple-darwin.tar.gz"
    sha256 "d985268876ff6c5437474cb382e969b6def3fa5f5a8b063591cd81123498920e"
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
