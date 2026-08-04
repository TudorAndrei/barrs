class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.2.6"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.6/barrs-v0.2.6-aarch64-apple-darwin.tar.gz"
    sha256 "d1434c3469366e5fe6092ada297194e470afbaf01cdcb1a84785b7602161ca09"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.6/barrs-v0.2.6-x86_64-apple-darwin.tar.gz"
    sha256 "77ea895a14e90696bfadda42af6463a0fed5e4ac6c335e39f30a33ed8c920e85"
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
