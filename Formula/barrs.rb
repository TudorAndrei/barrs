class Barrs < Formula
  desc "Native macOS status bar for Rift"
  homepage "https://github.com/TudorAndrei/barrs"
  version "0.1.7"
  license "Apache-2.0"

  if Hardware::CPU.arm?
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.7/barrs-v0.1.7-aarch64-apple-darwin.tar.gz"
    sha256 "86916e6152b82016a5163f3113332d8970d4fc1befa661473b6ca8cb7f15251d"
  else
    url "https://github.com/TudorAndrei/barrs/releases/download/v0.1.7/barrs-v0.1.7-x86_64-apple-darwin.tar.gz"
    sha256 "e0024a870c26dcedbc9cb07d1e654069dbeb7a2362e1f48a2b49abd5ad65d472"
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
