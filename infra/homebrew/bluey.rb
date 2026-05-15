class Bluey < Formula
  desc "Lightweight native AI work copilot"
  homepage "https://github.com/<org>/bluey"
  version "0.1.0"
  license "UNLICENSED"

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
    bin.install "cue-daemon" => "bluey-daemon"
    bin.install "cue-cli" => "bluey"
    # Install dashboard app bundle if present
    if File.directory?("Bluey.app")
      prefix.install "Bluey.app"
      (bin/"bluey-dashboard").write <<~SH
        #!/bin/sh
        open "#{prefix}/Bluey.app"
      SH
      (bin/"bluey-dashboard").chmod 0755
    end
  end

  def caveats
    <<~EOS
      On first launch, macOS will prompt for permissions:
        - Microphone access (for meeting audio capture)
        - Screen Recording (for screen-aware context)
        - Accessibility (for overlay positioning)

      Grant these in System Settings → Privacy & Security.

      Start the daemon:
        bluey-daemon &

      Open the dashboard:
        bluey-dashboard
    EOS
  end

  test do
    assert_match "bluey", shell_output("#{bin}/bluey --version")
  end
end
