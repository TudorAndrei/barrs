class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.3.2"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.3.2/barrs-v0.3.2-aarch64-apple-darwin.tar.gz"
    sha256 "c18895a4287f82de8a8e5da9650758e9506c46186f36e662d8a69b57dcd74023"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.3.2/barrs-v0.3.2-x86_64-apple-darwin.tar.gz"
    sha256 "8379e81b6166eaddfee74fff629a4c9d02089b8e4fd18859be8a95133e672ce5"
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
