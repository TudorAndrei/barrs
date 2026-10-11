class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.3.0"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.3.0/barrs-v0.3.0-aarch64-apple-darwin.tar.gz"
    sha256 "74f93d521c7ca498329f7538a20cafe63c3166047a221a8e799d4232b4ec53f2"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.3.0/barrs-v0.3.0-x86_64-apple-darwin.tar.gz"
    sha256 "1497ffd4b46b570c3a4d6b517fbbe6ca657ef7ddf2b8f234528e754f6a541545"
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
