class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.8.9"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.8.9/shiki-v0.8.9-aarch64-apple-darwin.tar.gz"
      sha256 "c56402b439d354833fb2c9e82baa0c6c2c42bd35ddbc18765ebfbb34e3ac875f"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.8.9/shiki-v0.8.9-x86_64-apple-darwin.tar.gz"
      sha256 "15bd43420d50d7fd34b8c7a978d825c85fd5748c8b26f59fde091c1e2cde808c"
    end
  end

  def install
    bin.install "shiki"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
  end
end
