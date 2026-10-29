import "settings.just"

set working-directory := "../.."

[doc("Run the app in a simulator, attached to `just ios rig` or a running zz Dev daemon.")]
run family="iPhone":
    @scripts/ios.sh {{ family }} run

[doc("Build the simulator app without launching it.")]
build family="iPhone":
    @scripts/ios.sh {{ family }} build

[doc("Install a signed release build on a paired, unlocked device and launch it against this Mac.")]
device family="iPhone":
    @scripts/ios.sh {{ family }} device

[doc("Measure frames on a paired, unlocked device without touching it.")]
bench family="iPhone":
    @scripts/ios.sh {{ family }} bench

[doc("Start a throwaway daemon with a sample session for simulator runs, or stop it.")]
rig action="start":
    @scripts/ios-rig.sh {{ action }}

[doc("Upload a signed dev.zz.ios build for internal TestFlight testing.")]
testflight:
    @scripts/ios.sh iPhone testflight
