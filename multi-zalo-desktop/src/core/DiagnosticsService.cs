using System;
using System.Collections.Generic;
using System.IO;

namespace MultiZaloDesktop.Core
{
    public sealed class IsolationCheckItem
    {
        public string CheckName { get; set; }
        public bool Passed { get; set; }
        public string Detail { get; set; }
    }

    public sealed class IsolationReport
    {
        public bool AllPassed { get; set; }
        public string TimestampUtc { get; set; }
        public List<IsolationCheckItem> Checks { get; set; }

        public IsolationReport()
        {
            Checks = new List<IsolationCheckItem>();
        }
    }

    public static class DiagnosticsService
    {
        public static IsolationReport VerifyIsolation(InstanceManager manager)
        {
            StatusSnapshot status = manager.CaptureStatus(false);
            IsolationReport report = new IsolationReport();
            report.TimestampUtc = DateTime.UtcNow.ToString("o");

            // 1. Official installation integrity
            report.Checks.Add(new IsolationCheckItem
            {
                CheckName = "OfficialZaloInstallation",
                Passed = status.Installation != null && status.Installation.IsValid,
                Detail = status.Installation != null && status.Installation.IsValid
                    ? string.Format("Version={0}, Exe={1}, Auth={2}", status.Installation.Version, status.Installation.ElectronExePath, status.Installation.AuthenticodeStatus)
                    : (status.Installation != null ? status.Installation.Error : "Missing")
            });

            // 2. Profile paths distinct from primary APPDATA\ZaloData
            string primaryPath = Path.GetFullPath(status.PrimaryZalo.ZaloDataPath).TrimEnd('\\');
            bool anyPrimaryPathOverlap = false;
            HashSet<string> seenZaloDataPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            bool anyDuplicatePath = false;

            HashSet<int> primaryPidSet = new HashSet<int>(status.PrimaryZalo.LockingPids);
            HashSet<int> seenRunningMainPids = new HashSet<int>();
            bool anyPidOverlap = false;

            for (int i = 0; i < status.Profiles.Count; i++)
            {
                ProfileRuntimeState p = status.Profiles[i];
                string pData = Path.GetFullPath(p.ZaloDataPath).TrimEnd('\\');
                if (string.Equals(pData, primaryPath, StringComparison.OrdinalIgnoreCase))
                {
                    anyPrimaryPathOverlap = true;
                }
                if (!seenZaloDataPaths.Add(pData))
                {
                    anyDuplicatePath = true;
                }

                if (p.IsRunning && p.MainPid > 0)
                {
                    if (primaryPidSet.Contains(p.MainPid) || !seenRunningMainPids.Add(p.MainPid))
                    {
                        anyPidOverlap = true;
                    }
                }
            }

            report.Checks.Add(new IsolationCheckItem
            {
                CheckName = "PrimaryZaloDataSeparation",
                Passed = !anyPrimaryPathOverlap,
                Detail = !anyPrimaryPathOverlap
                    ? "All managed profiles reside strictly outside primary " + primaryPath
                    : "At least one profile overlaps primary ZaloData path"
            });

            report.Checks.Add(new IsolationCheckItem
            {
                CheckName = "DistinctProfilePaths",
                Passed = !anyDuplicatePath,
                Detail = string.Format("Checked {0} profile(s), unique ZaloData paths={1}", status.Profiles.Count, seenZaloDataPaths.Count)
            });

            report.Checks.Add(new IsolationCheckItem
            {
                CheckName = "DistinctProcessAndLockfileOwnership",
                Passed = !anyPidOverlap,
                Detail = string.Format("Running managed instances={0}, Primary locking PIDs=[{1}]",
                    seenRunningMainPids.Count,
                    string.Join(",", status.PrimaryZalo.LockingPids))
            });

            bool allOk = true;
            for (int i = 0; i < report.Checks.Count; i++)
            {
                if (!report.Checks[i].Passed)
                {
                    allOk = false;
                    break;
                }
            }
            report.AllPassed = allOk;
            return report;
        }
    }
}
