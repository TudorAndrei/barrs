class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.2.2"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.2/barrs-v0.2.2-aarch64-apple-darwin.tar.gz"
    sha256 "2c83f472e026e9e2cdee7c358fceb244b560b75f4bc6b0f8c077f1edaa6668a3"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.2/barrs-v0.2.2-x86_64-apple-darwin.tar.gz"
    sha256 "ed463b06710cb7426c37f919fb92e1cebda2a719ff74ae1a602c84a977b129b2"
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
