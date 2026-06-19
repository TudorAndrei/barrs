class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.2.0"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.0/barrs-v0.2.0-aarch64-apple-darwin.tar.gz"
    sha256 "43a7bb388e9126b684f9a8882d84ebe9fb5698551dd74787b4b4b34c5a70b348"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.0/barrs-v0.2.0-x86_64-apple-darwin.tar.gz"
    sha256 "3dff0270ea49b6fae3e3d859454e593545be6958fae6e8acfea270efcb7acacf"
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
