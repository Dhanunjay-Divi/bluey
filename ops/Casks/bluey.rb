cask "bluey" do
  arch arm: "arm64"
  version "0.2.0"
  # TODO: replace :no_check with the release SHA256 once v0.2.0
  # artifacts are published from the release workflow.
  sha256 :no_check

  url "https://bluey.dev/releases/v#{version}/bluey-#{version}-darwin-#{arch}.tar.gz"
  name "Bluey"
  desc "Quiet AI copilot — listens, suggests answers, stays out of the way"
  homepage "https://bluey.dev"
  depends_on arch: :arm64

  app "Bluey.app"

  # Bluey ships ad-hoc signed (recognised by Gatekeeper as self-signed)
  # because the alpha distribution avoids the Apple Developer Program
  # cost. Brew normally would flag this; postflight strips quarantine
  # so first launch is clean. Same pattern Pinky uses.
  postflight do
    system_command "/usr/bin/codesign",
                   args: ["--force", "--deep", "--sign", "-",
                          "#{appdir}/Bluey.app"],
                   sudo: false
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine",
                          "#{appdir}/Bluey.app"],
                   sudo: false
  end

  binary "bin/bluey", target: "bluey"

  uninstall quit:    "com.bluey.dashboard",
            delete:  [
                       "~/Library/Application Support/Bluey",
                       "~/Library/Logs/Bluey",
                       "~/Library/Preferences/com.bluey.dashboard.plist",
                     ]

  zap trash: [
        "~/Library/Application Support/Bluey",
        "~/Library/Caches/com.bluey.dashboard",
        "~/Library/Logs/Bluey",
        "~/Library/Preferences/com.bluey.dashboard.plist",
      ]
end
