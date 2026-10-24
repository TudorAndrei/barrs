class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.3.1"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.3.1/barrs-v0.3.1-aarch64-apple-darwin.tar.gz"
    sha256 "1fd9b44368039c651a53325ebb7bc55da5b3504b7764f3e855c312cc382dfe60"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.3.1/barrs-v0.3.1-x86_64-apple-darwin.tar.gz"
    sha256 "01723955d0c86822aa839ff8cab4b44039f9c9b2b187e696ceeeb350b3ba18a5"
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
