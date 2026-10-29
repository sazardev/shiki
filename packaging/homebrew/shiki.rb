class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.10.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.1/shiki-v0.10.1-aarch64-apple-darwin.tar.gz"
      sha256 "3e9377005038af386a8935e9db780438e44ad68f4987de337f47bb60f012a396"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.1/shiki-v0.10.1-x86_64-apple-darwin.tar.gz"
      sha256 "bb23cc52ac97142f4b9d9b6b71d248b8f89586753cf905c0298d8f7a13cd19fe"
    end
  end

  def install
    bin.install "shiki"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
  end
end
