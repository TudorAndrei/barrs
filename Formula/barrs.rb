class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.9"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.9/barrs-v0.1.9-aarch64-apple-darwin.tar.gz"
    sha256 "27e2bfd1556ff2097cdc34904a4ee6a8054d05f2e92ee413688ac08f4b0ff33e"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.9/barrs-v0.1.9-x86_64-apple-darwin.tar.gz"
    sha256 "0b7ce1526b7abeaec30ee7eb2f5d6c2b748d1d10f409d36afcd80cb053c78647"
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
