class Gust < Formula
  desc "Blazing fast Swift package manager written in Rust"
  homepage "https://github.com/quantbagel/gust"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/quantbagel/gust/releases/download/v#{version}/gust-aarch64-apple-darwin.tar.gz"
      sha256 "f605b39fd6734403a5bb1d92f869071825470925cac02d1bbab06233933df30d"
    end

    on_intel do
      url "https://github.com/quantbagel/gust/releases/download/v#{version}/gust-x86_64-apple-darwin.tar.gz"
      sha256 "2a4333d93cc8553612be80c83858c2c03b67901a7852fc6aa8941d09862ea8d3"
    end
  end

  def install
    bin.install "gust"
  end

  test do
    system "#{bin}/gust", "--version"

    # Test creating a new package
    system "#{bin}/gust", "new", "test-pkg", "--type", "lib"
    assert_predicate testpath/"test-pkg/Gust.toml", :exist?
  end
end
