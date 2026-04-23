class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.3"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.3/barrs-v0.1.3-aarch64-apple-darwin.tar.gz"
    sha256 "7de2c58d977f7ee0b4059a081426cd59a7285a455c56bd87a176ddb2c5611e04"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.3/barrs-v0.1.3-x86_64-apple-darwin.tar.gz"
    sha256 "fd2428d7711dbd3fdea32e9d92332108d0e8130f636cf82b7e7254007e1dda76"
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
