class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.13"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.13/barrs-v0.1.13-aarch64-apple-darwin.tar.gz"
    sha256 "517d56ff66592eec31be3d1e3392eaffa007176e4a000966911b1dbfef3f3e6a"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.13/barrs-v0.1.13-x86_64-apple-darwin.tar.gz"
    sha256 "76bffde47ca1c13f23c9c756ee65271d658f1f5a79f99e6916d23e07b31ace20"
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
