class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.10.4"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.4/shiki-v0.10.4-aarch64-apple-darwin.tar.gz"
      sha256 "88b04c081d6151adfcbfdbd1fcf82173d8e18a44c888132ad91305e2b5de5945"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.4/shiki-v0.10.4-x86_64-apple-darwin.tar.gz"
      sha256 "345e959d4ac3bb1407cb1e869e5c6815b2f383bac304f796cfa1aa5e0f831411"
    end
  end

  def install
    bin.install "shiki"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
  end
end
