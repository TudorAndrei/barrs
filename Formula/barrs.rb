class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.2.3"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.3/barrs-v0.2.3-aarch64-apple-darwin.tar.gz"
    sha256 "05e601da354c641e1787bc9a9515739dbfdb8df88fec55d959a140fe2a100ee9"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.3/barrs-v0.2.3-x86_64-apple-darwin.tar.gz"
    sha256 "3ca846084476cfff8574a428aa44c78e1888a0dd7c85f1d540dfcbd2d1df57cb"
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
