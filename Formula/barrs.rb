class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.5"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.5/barrs-v0.1.5-aarch64-apple-darwin.tar.gz"
    sha256 "a38c0669d10e8ff858f2ac097df8a9cf4e7d5d9a13be2291687052dc9c928e09"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.5/barrs-v0.1.5-x86_64-apple-darwin.tar.gz"
    sha256 "4cd48124006baf92e14cf6c5ccb96f9ae847d7f7e2ebfff0168a8cd3740a4dfa"
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
