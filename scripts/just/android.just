import "settings.just"

set working-directory := "../.."

[doc("Install the Android SDK, NDK, emulator and system image, the Rust targets, and the zz-phone AVD.")]
setup:
    @scripts/android.sh setup

[doc("Boot the zz-phone emulator if none is running.")]
emulator:
    @scripts/android.sh emulator

[doc("Build, install and launch on the emulator, attached to `just android rig` when it is up.")]
run:
    @scripts/android.sh run

[doc("Build the emulator APK without installing it.")]
build:
    @scripts/android.sh build

[doc("Build a release arm64 APK, install it on a USB or wireless device, and launch it.")]
device:
    @scripts/android.sh device

[doc("Follow the running app's log.")]
logs:
    @scripts/android.sh logs

[doc("Start the throwaway daemon the iOS rig uses, or stop it; `run` reaches it through adb reverse.")]
rig action="start":
    @scripts/ios-rig.sh {{ action }}
