#!/bin/zsh
# Builds build/Snowdrift.app for Apple silicon and Intel.
# Signs ad hoc for local use; set IDENTITY to a "Developer ID Application" identity for a release build.
set -euo pipefail
cd "$(dirname "$0")"

flags=(-c release --arch arm64 --arch x86_64)
swift build "${flags[@]}"
products="$(swift build "${flags[@]}" --show-bin-path)"

app=build/Snowdrift.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# Swift Build stamps the binary with the deployment target as its SDK, and macOS then draws the app in its old
# look, without Liquid Glass.
minimum=$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' Resources/Info.plist)
vtool -set-build-version macos "$minimum" "$(xcrun --sdk macosx --show-sdk-version)" -replace \
    -output "$app/Contents/MacOS/Snowdrift" "$products/Snowdrift"
cp Resources/Info.plist "$app/Contents/"
VERSION="$(tr -d '[:space:]' < VERSION)"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" -c "Set :CFBundleVersion $VERSION" "$app/Contents/Info.plist"
cp Resources/AppIcon.icns "$app/Contents/Resources/"
for strings in locales/*.strings; do
    lproj="$app/Contents/Resources/$(basename "$strings" .strings).lproj"
    mkdir -p "$lproj"
    plutil -convert binary1 -o "$lproj/Localizable.strings" "$strings"
done

if [[ -n "${IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$IDENTITY" "$app"
else
    codesign --force --deep --sign - "$app"
fi
codesign --verify --deep --strict "$app"
echo "Built $app"
