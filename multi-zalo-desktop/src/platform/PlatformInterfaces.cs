using System.Collections.Generic;
using MultiZaloDesktop.Core;

namespace MultiZaloDesktop.Platform
{
    public interface IPlatformAdapter
    {
        string PlatformName { get; }
        bool IsSupportedOnCurrentHost { get; }
        string SupportSummary { get; }

        ZaloInstallationInfo DiscoverOfficialInstallation(string customInstallRoot);
        PrimaryZaloStatus InspectPrimaryZaloStatus();
        void EnsureProfileDirectories(string profilePath);
        ProfileRuntimeState InspectProfileRuntime(ProfileRecord record, string profilePath);
        OperationResult LaunchProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath);
        OperationResult FocusOrRestoreProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath, ProfileRuntimeState state);
        OperationResult StopProfile(ProfileRuntimeState state, IEnumerable<int> protectedPrimaryPids);
        void OpenDirectoryInFileManager(string path);
        long CalculateDirectorySize(string path);
    }
}
