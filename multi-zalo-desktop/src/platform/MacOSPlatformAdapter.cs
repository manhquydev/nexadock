using System;
using System.Collections.Generic;
using MultiZaloDesktop.Core;

namespace MultiZaloDesktop.Platform
{
    /// <summary>
    /// Architectural adapter stub for macOS (Phase 3).
    /// Static research against Zalo's app.asar confirms macOS stores data under
    /// ~/Library/Application Support/ZaloData (app.getPath("appData") + "/ZaloData")
    /// and binds Unix domain sockets /tmp/socketzalosend2021 and /tmp/socketzalorecv2021
    /// for ZaloHelper.app/Contents/MacOS/ZaloCall.
    /// Runtime validation on macOS hardware is required before enabling launch support.
    /// </summary>
    public sealed class MacOSPlatformAdapter : IPlatformAdapter
    {
        public string PlatformName { get { return "macos"; } }
        public bool IsSupportedOnCurrentHost { get { return false; } }
        public string SupportSummary { get { return "Research / Future (macOS HOME isolation unverified on macOS hardware; see docs/compatibility.md)"; } }

        public ZaloInstallationInfo DiscoverOfficialInstallation(string customInstallRoot)
        {
            ZaloInstallationInfo info = new ZaloInstallationInfo();
            info.IsValid = false;
            info.InstallRoot = "/Applications/Zalo.app";
            info.Error = "macOS adapter is a research specification only and cannot execute on this host.";
            return info;
        }

        public PrimaryZaloStatus InspectPrimaryZaloStatus()
        {
            return new PrimaryZaloStatus();
        }

        public void EnsureProfileDirectories(string profilePath)
        {
            throw new NotSupportedException("macOS profile management requires running on macOS.");
        }

        public ProfileRuntimeState InspectProfileRuntime(ProfileRecord record, string profilePath)
        {
            ProfileRuntimeState state = new ProfileRuntimeState();
            state.ProfileId = record.Id;
            state.ProfileName = record.Name;
            state.ProfilePath = profilePath;
            state.StatusLabel = "Unsupported on host";
            return state;
        }

        public OperationResult LaunchProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath)
        {
            return OperationResult.Fail("UNSUPPORTED_PLATFORM", "macOS launch adapter is not supported on Windows.");
        }

        public OperationResult FocusOrRestoreProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath, ProfileRuntimeState state)
        {
            return OperationResult.Fail("UNSUPPORTED_PLATFORM", "macOS focus adapter is not supported on Windows.");
        }

        public OperationResult StopProfile(ProfileRuntimeState state, IEnumerable<int> protectedPrimaryPids)
        {
            return OperationResult.Fail("UNSUPPORTED_PLATFORM", "macOS stop adapter is not supported on Windows.");
        }

        public void OpenDirectoryInFileManager(string path)
        {
        }

        public long CalculateDirectorySize(string path)
        {
            return 0;
        }
    }
}
