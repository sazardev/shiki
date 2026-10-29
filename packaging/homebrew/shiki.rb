class Shiki < Formula
  desc "TUI note-taking app with a Yazi-inspired three-pane layout and git-backed notebooks"
  homepage "https://github.com/sazardev/shiki"
  version "0.10.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.2/shiki-v0.10.2-aarch64-apple-darwin.tar.gz"
      sha256 "109c90babd5ae2d683ea212ba484ba078d80c2bf0b19efec4fa55fd8441d3354"
    end
    on_intel do
      url "https://github.com/sazardev/shiki/releases/download/v0.10.2/shiki-v0.10.2-x86_64-apple-darwin.tar.gz"
      sha256 "7c797a9b160b7245445c6619e4f7353296c66087b1c9dd38313c6bd0b40b6113"
    end
  end

  def install
    bin.install "shiki"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/shiki --version")
  end
end
