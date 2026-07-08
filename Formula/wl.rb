class Wl < Formula
  desc "Keyboard-first worklog TUI"
  homepage "https://github.com/sdavisde/worklog"
  version "0.1.0"
  on_macos do
    on_arm do
      url "https://github.com/sdavisde/worklog/releases/download/v0.1.0/wl-aarch64-apple-darwin.tar.gz"
      sha256 "PUT_TARBALL_SHA256_HERE"
    end
  end

  def install
    bin.install "wl"
  end

  test do
    system "#{bin}/wl", "--version"
  end
end
