class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.10.5"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.5/shiki-v0.10.5-aarch64-apple-darwin.tar.gz"
      sha256 "abff6d4689889b25e3633db13ccd96d2b314968f81d558f8ca938bb94ebc635c"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.5/shiki-v0.10.5-x86_64-apple-darwin.tar.gz"
      sha256 "cf609aa0ab8395bd40803a169a73d5cd55121fc1eb2c1e6b88408e2bb0f9c194"
    end
  end

  def install
    bin.install "shiki"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
  end
end
