import "settings.just"

set working-directory := "../.."

macos_notary_profile := env_var_or_default("MACOS_NOTARY_PROFILE", "zz-notary")

[doc("Store Apple notarization credentials in Keychain (one-time interactive setup).")]
setup profile=macos_notary_profile:
    @if [[ "$(uname -s)" != "Darwin" ]]; then echo "just release mac setup requires macOS" >&2; exit 2; fi
    @xcrun notarytool store-credentials "{{ profile }}"

[doc("Check the Developer ID identity and saved notary profile without uploading anything.")]
check profile=macos_notary_profile:
    @MACOS_NOTARY_PROFILE="{{ profile }}" scripts/release-macos.sh preflight

[doc("Replace an existing bundle's ad-hoc signature with a Developer ID signature.")]
sign app="dist/zz/zz.app":
    @scripts/release-macos.sh sign-app "{{ app }}"

[doc("Sign, submit, staple, and verify an existing Developer ID-signed DMG.")]
notarize dmg profile=macos_notary_profile:
    @MACOS_NOTARY_PROFILE="{{ profile }}" scripts/release-macos.sh notarize-dmg "{{ dmg }}"

[doc("Recheck an already-notarized DMG locally without uploading it again.")]
verify dmg:
    @scripts/release-macos.sh verify-dmg "{{ dmg }}"

[doc("Build, Developer ID-sign, package, notarize, staple, and verify a versioned DMG.")]
build version profile=macos_notary_profile:
    @MACOS_NOTARY_PROFILE="{{ profile }}" scripts/release-macos.sh release "{{ version }}"
