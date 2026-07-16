class Bluey < Formula
  desc "Lightweight native AI work copilot"
  homepage "https://github.com/<org>/bluey"
  version "0.1.0"
  license "UNLICENSED"

  # Artifact naming: bluey-{version}-{os}-{arch}.tar.gz
  # Archive layout: terminal CLI, daemon, and trusted native helpers under bin/.
  # The development dashboard is intentionally not distributed.
  # Keep in sync with: .github/workflows/release.yml, Makefile,
  #                     infra/scoop/bluey.json, INSTALL.md
  on_macos do
    on_arm do
      url "https://github.com/<org>/bluey/releases/download/v#{version}/bluey-#{version}-darwin-arm64.tar.gz"
      sha256 "REPLACE_AT_RELEASE"
    end
    on_intel do
      url "https://github.com/<org>/bluey/releases/download/v#{version}/bluey-#{version}-darwin-x86_64.tar.gz"
      sha256 "REPLACE_AT_RELEASE"
    end
  end

  def install
    bin.install Dir["bin/*"].select { |path| File.file?(path) }
  end

  def caveats
    <<~EOS
      On first launch, macOS will prompt for permissions:
        - Microphone access (for meeting audio capture)
        - Screen Recording (for screen-aware context)
        - Accessibility (for overlay positioning)

      Grant these in System Settings → Privacy & Security.

      Start Bluey:
        bluey on
    EOS
  end

  test do
    assert_match "bluey", shell_output("#{bin}/bluey --version")
  end
end
