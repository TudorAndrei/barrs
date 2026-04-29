class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.10"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.10/barrs-v0.1.10-aarch64-apple-darwin.tar.gz"
    sha256 "10569bff260146768e04671fd6a0fbf324b10f16f4c1fbc37df71a6b1230be8e"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.10/barrs-v0.1.10-x86_64-apple-darwin.tar.gz"
    sha256 "0d48d9ca8226e34d05e200eb3d0c9575b7c0929b97f21494b19082c53d90bf6a"
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
