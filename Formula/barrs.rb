class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.12"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.12/barrs-v0.1.12-aarch64-apple-darwin.tar.gz"
    sha256 "08662a4aba5ef7ce8f8c14902df6e3911620c91cc30ecdb715c8f8110d877d33"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.12/barrs-v0.1.12-x86_64-apple-darwin.tar.gz"
    sha256 "75ca0c9b4ce72281d6867d428376a7c770d4c3422b07bc5cba247c46b41d2812"
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
