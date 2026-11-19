class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.11.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.11.0/shiki-v0.11.0-aarch64-apple-darwin.tar.gz"
      sha256 "8a1a8bc9b1a93c062640fec384d0e1f9247f75d56f39c3ee15f812d4909ba445"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.11.0/shiki-v0.11.0-x86_64-apple-darwin.tar.gz"
      sha256 "17308ced8cdbdc648a3e12a7499b4191c78973a83dcba31bc69154c528b3e3ff"
    end
  end

  def install
    bin.install "shiki", "shiki-mcp"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
    assert_match version.to_s, shell_output("#{bin}/shiki-mcp --version")
  end
end
