class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.4"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.4/barrs-v0.1.4-aarch64-apple-darwin.tar.gz"
    sha256 "4c6227b56f1a824ccbb7ebc1f3604dcc08558665f130bc6eb1312f4df5c53cf6"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.4/barrs-v0.1.4-x86_64-apple-darwin.tar.gz"
    sha256 "8437c7d295adda87a7b9a576494586d119627ed18e85ddd9bf6b711ed2efee37"
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
