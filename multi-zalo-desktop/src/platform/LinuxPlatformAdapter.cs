using System;
using System.Collections.Generic;
using MultiZaloDesktop.Core;

namespace MultiZaloDesktop.Platform
{
    /// <summary>
    /// Architectural adapter stub for Linux (Phase 4).
    /// Static inspection of Zalo 26.10.20 (main-dist/main.js) confirms VNG ships no
    /// official native Linux desktop runtime helpers (ZaloCall/ZaloCap exist only for win32 and darwin).
    /// Per project policy, Wine and Zalo Web are not treated as native Linux support.
    /// </summary>
    public sealed class LinuxPlatformAdapter : IPlatformAdapter
    {
        public string PlatformName { get { return "linux"; } }
        public bool IsSupportedOnCurrentHost { get { return false; } }
        public string SupportSummary { get { return "Unsupported (No official native Linux Zalo Desktop release exists)"; } }

        public ZaloInstallationInfo DiscoverOfficialInstallation(string customInstallRoot)
        {
            ZaloInstallationInfo info = new ZaloInstallationInfo();
            info.IsValid = false;
            info.Error = "No official native Linux Zalo Desktop binary is distributed by VNG.";
            return info;
        }

        public PrimaryZaloStatus InspectPrimaryZaloStatus()
        {
            return new PrimaryZaloStatus();
        }

        public void EnsureProfileDirectories(string profilePath)
        {
            throw new NotSupportedException("Linux native Zalo Desktop is not supported.");
        }

        public ProfileRuntimeState InspectProfileRuntime(ProfileRecord record, string profilePath)
        {
            ProfileRuntimeState state = new ProfileRuntimeState();
            state.ProfileId = record.Id;
            state.ProfileName = record.Name;
            state.ProfilePath = profilePath;
            state.StatusLabel = "Unsupported";
            return state;
        }

        public OperationResult LaunchProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath)
        {
            return OperationResult.Fail("UNSUPPORTED_PLATFORM", "Official native Linux Zalo Desktop is unavailable.");
        }

        public OperationResult FocusOrRestoreProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath, ProfileRuntimeState state)
        {
            return OperationResult.Fail("UNSUPPORTED_PLATFORM", "Official native Linux Zalo Desktop is unavailable.");
        }

        public OperationResult StopProfile(ProfileRuntimeState state, IEnumerable<int> protectedPrimaryPids)
        {
            return OperationResult.Fail("UNSUPPORTED_PLATFORM", "Official native Linux Zalo Desktop is unavailable.");
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
