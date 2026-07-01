class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.2.1"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.1/barrs-v0.2.1-aarch64-apple-darwin.tar.gz"
    sha256 "acc3f98519c5c01c5e082018e87ab2c87de69693dcfaa7d86bc5232f91a79969"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.1/barrs-v0.2.1-x86_64-apple-darwin.tar.gz"
    sha256 "e1a3315ff5f0c52c8b95ac9b1b97919febf2944b357fa86da2f4ed1b0e985f31"
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
