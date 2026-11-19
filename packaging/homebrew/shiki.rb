class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.10.6"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.6/shiki-v0.10.6-aarch64-apple-darwin.tar.gz"
      sha256 "f5bd52625622463a8e8fc3948c43b97760738979b51dd1e374e42169982337b5"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.6/shiki-v0.10.6-x86_64-apple-darwin.tar.gz"
      sha256 "45265b211cac631938bdb35fb8719120f9dffbea536d3dafa42a30b70d4b5e59"
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
