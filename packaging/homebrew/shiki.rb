class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.10.3"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.3/shiki-v0.10.3-aarch64-apple-darwin.tar.gz"
      sha256 "909faaaf84121d87147a33d1ff36f3bbd3aa4e00ee247c6564c9a7c2998c2b3a"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.3/shiki-v0.10.3-x86_64-apple-darwin.tar.gz"
      sha256 "236cac3b5d1b0109b4889d551b86535db94a5a4135c202fcb641aa038278b003"
    end
  end

  def install
    bin.install "shiki"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
  end
end
