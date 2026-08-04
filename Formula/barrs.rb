class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.2.5"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.5/barrs-v0.2.5-aarch64-apple-darwin.tar.gz"
    sha256 "b3f21e61855fb67ad00de863036977878563965665b6b9618940914d8d2336b2"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.2.5/barrs-v0.2.5-x86_64-apple-darwin.tar.gz"
    sha256 "c090686b5d61822a6052fc157f4b8b7672de6830d466cba8d7caf540504cb083"
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
