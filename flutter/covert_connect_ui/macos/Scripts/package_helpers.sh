#!/bin/bash
# Packages the Rust helper binaries (cc-client, cc-tray) into the macOS .app
# bundle produced by Xcode.
#
# - cc-client: copied into Contents/MacOS. It is invoked via PrivilegedCommand
#   (sudo prompt) for `install`/`uninstall` and otherwise runs under launchd, so
#   it does not register with LaunchServices and a separate bundle is not
#   needed.
# - cc-tray:   packaged as its own helper .app inside
#     Contents/Library/LoginItems/CovertConnectTray.app
#   This gives it a distinct CFBundleIdentifier and LSUIElement=1, so it does
#   not collide with the parent .app in LaunchServices (which would otherwise
#   route a second open of Covert Connect.app to the running tray instead of
#   launching the UI). It is also the canonical location for SMAppService
#   login items, which the tray uses for auto-launch.
#
# Invoked from the "Copy client service and tray" build phase of Runner.xcodeproj.
# Inherits ${SRCROOT}, ${BUILT_PRODUCTS_DIR}, ${FULL_PRODUCT_NAME},
# ${CONFIGURATION}, ${EXPANDED_CODE_SIGN_IDENTITY} from Xcode.

set -e

RUST_PROJECT_PATH="${SRCROOT}/../../../"
APP_CONTENTS="${BUILT_PRODUCTS_DIR}/${FULL_PRODUCT_NAME}/Contents"
MACOS_DIR="${APP_CONTENTS}/MacOS"
TRAY_HELPER_APP="${APP_CONTENTS}/Library/LoginItems/CovertConnectTray.app"
TRAY_HELPER_MACOS="${TRAY_HELPER_APP}/Contents/MacOS"
TRAY_HELPER_PLIST_SRC="${SRCROOT}/Helpers/CovertConnectTray/Info.plist"

if [ "$CONFIGURATION" = "Debug" ] || [ "$CONFIGURATION" = "DebugDevelopment" ]; then
    RUST_BIN_DIR="${RUST_PROJECT_PATH}/target/debug"
    SIGN_IDENTITY="-"
else
    RUST_BIN_DIR="${RUST_PROJECT_PATH}/target/release-prod"
    # Replace with "${EXPANDED_CODE_SIGN_IDENTITY}" once Developer ID signing
    # is configured.
    SIGN_IDENTITY="-"
fi

mkdir -p "${MACOS_DIR}"

copy_tool_to_macos() {
    local tool_name="$1"
    local src_path="${RUST_BIN_DIR}/${tool_name}"
    if [ ! -f "${src_path}" ]; then
        echo "error: ${tool_name} not found at ${src_path}"
        exit 1
    fi
    cp -f "${src_path}" "${MACOS_DIR}/${tool_name}"
    chmod +x "${MACOS_DIR}/${tool_name}"
    echo "Copied ${tool_name} -> ${MACOS_DIR}/${tool_name}"
    if [ -n "${SIGN_IDENTITY}" ]; then
        codesign --force --timestamp --sign "${SIGN_IDENTITY}" "${MACOS_DIR}/${tool_name}"
    fi
}

copy_tool_to_macos "cc-client"

# Package cc-tray as a helper .app bundle.
src_tray="${RUST_BIN_DIR}/cc-tray"
if [ ! -f "${src_tray}" ]; then
    echo "error: cc-tray not found at ${src_tray}"
    exit 1
fi
if [ ! -f "${TRAY_HELPER_PLIST_SRC}" ]; then
    echo "error: tray helper Info.plist not found at ${TRAY_HELPER_PLIST_SRC}"
    exit 1
fi

rm -rf "${TRAY_HELPER_APP}"
mkdir -p "${TRAY_HELPER_MACOS}"
cp -f "${TRAY_HELPER_PLIST_SRC}" "${TRAY_HELPER_APP}/Contents/Info.plist"
cp -f "${src_tray}" "${TRAY_HELPER_MACOS}/cc-tray"
chmod +x "${TRAY_HELPER_MACOS}/cc-tray"
echo "Packaged cc-tray -> ${TRAY_HELPER_APP}"

# Sign inside-out: nested binary first, then the helper bundle.
if [ -n "${SIGN_IDENTITY}" ]; then
    codesign --force --timestamp --sign "${SIGN_IDENTITY}" "${TRAY_HELPER_MACOS}/cc-tray"
    codesign --force --timestamp --sign "${SIGN_IDENTITY}" "${TRAY_HELPER_APP}"
fi
