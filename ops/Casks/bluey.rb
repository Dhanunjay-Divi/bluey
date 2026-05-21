cask "bluey" do
  arch arm: "aarch64", intel: "x86_64"
  version "0.2.0"
  sha256 :no_check

  url "https://bluey.dev/releases/v#{version}/Bluey-#{arch}.tar.gz"
  name "Bluey"
  desc "Quiet AI copilot — listens, suggests answers, stays out of the way"
  homepage "https://bluey.dev"

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

  binary "#{appdir}/Bluey.app/Contents/Resources/bluey-cli", target: "bluey"

  uninstall quit:    "com.bluey.app",
            delete:  [
                       "~/Library/Application Support/Bluey",
                       "~/Library/Logs/Bluey",
                       "~/Library/Preferences/com.bluey.app.plist",
                     ]

  zap trash: [
        "~/Library/Application Support/Bluey",
        "~/Library/Caches/com.bluey.app",
        "~/Library/Logs/Bluey",
        "~/Library/Preferences/com.bluey.app.plist",
      ]
end
