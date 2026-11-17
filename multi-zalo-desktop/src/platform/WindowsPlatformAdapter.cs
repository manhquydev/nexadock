using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography.X509Certificates;
using System.Text;
using System.Threading;
using MultiZaloDesktop.Core;

namespace MultiZaloDesktop.Platform
{
    public sealed class WindowsPlatformAdapter : IPlatformAdapter
    {
        public string PlatformName { get { return "windows"; } }
        public bool IsSupportedOnCurrentHost { get { return Environment.OSVersion.Platform == PlatformID.Win32NT; } }
        public string SupportSummary { get { return "Supported (Windows 10/11 Per-Process Environment & Shell-Folder Redirection)"; } }

        #region Win32 P/Invoke

        private const uint TH32CS_SNAPPROCESS = 0x00000002;
        private const uint WM_CLOSE = 0x0010;
        private const int SW_RESTORE = 9;
        private const int SW_SHOW = 5;
        private const int ERROR_MORE_DATA = 234;
        private const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
        private const uint CREATE_NEW_PROCESS_GROUP = 0x00000200;

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct STARTUPINFO
        {
            public int cb;
            public string lpReserved;
            public string lpDesktop;
            public string lpTitle;
            public int dwX;
            public int dwY;
            public int dwXSize;
            public int dwYSize;
            public int dwXCountChars;
            public int dwYCountChars;
            public int dwFillAttribute;
            public int dwFlags;
            public short wShowWindow;
            public short cbReserved2;
            public IntPtr lpReserved2;
            public IntPtr hStdInput;
            public IntPtr hStdOutput;
            public IntPtr hStdError;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct PROCESS_INFORMATION
        {
            public IntPtr hProcess;
            public IntPtr hThread;
            public uint dwProcessId;
            public uint dwThreadId;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CreateProcessW(
            string lpApplicationName,
            string lpCommandLine,
            IntPtr lpProcessAttributes,
            IntPtr lpThreadAttributes,
            [MarshalAs(UnmanagedType.Bool)] bool bInheritHandles,
            uint dwCreationFlags,
            IntPtr lpEnvironment,
            string lpCurrentDirectory,
            ref STARTUPINFO lpStartupInfo,
            out PROCESS_INFORMATION lpProcessInformation);

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct PROCESSENTRY32
        {
            public uint dwSize;
            public uint cntUsage;
            public uint th32ProcessID;
            public IntPtr th32DefaultHeapID;
            public uint th32ModuleID;
            public uint cntThreads;
            public uint th32ParentProcessID;
            public int pcPriClassBase;
            public uint dwFlags;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
            public string szExeFile;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct RM_UNIQUE_PROCESS
        {
            public int dwProcessId;
            public System.Runtime.InteropServices.ComTypes.FILETIME ProcessStartTime;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct RM_PROCESS_INFO
        {
            public RM_UNIQUE_PROCESS Process;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
            public string strAppName;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
            public string strServiceShortName;
            public uint ApplicationType;
            public uint AppStatus;
            public uint TSSessionId;
            [MarshalAs(UnmanagedType.Bool)]
            public bool bRestartable;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct RECT
        {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;
        }

        private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr CreateToolhelp32Snapshot(uint dwFlags, uint th32ProcessID);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool Process32First(IntPtr hSnapshot, ref PROCESSENTRY32 lppe);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool Process32Next(IntPtr hSnapshot, ref PROCESSENTRY32 lppe);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr hObject);

        [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
        private static extern int RmStartSession(out uint pSessionHandle, int dwSessionFlags, string strSessionKey);

        [DllImport("rstrtmgr.dll")]
        private static extern int RmEndSession(uint pSessionHandle);

        [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
        private static extern int RmRegisterResources(uint pSessionHandle, uint nFiles, string[] rgsFilenames, uint nApplications, [In] RM_UNIQUE_PROCESS[] rgApplications, uint nServices, string[] rgsServiceNames);

        [DllImport("rstrtmgr.dll")]
        private static extern int RmGetList(uint dwSessionHandle, out uint pnProcInfoNeeded, ref uint pnProcInfo, [In, Out] RM_PROCESS_INFO[] rgAffectedApps, ref uint lpdwRebootReasons);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindowVisible(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsIconic(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("user32.dll")]
        private static extern int GetWindowTextLength(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetForegroundWindow(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

        #endregion

        public ZaloInstallationInfo DiscoverOfficialInstallation(string customInstallRoot)
        {
            ZaloInstallationInfo info = new ZaloInstallationInfo();
            string root = customInstallRoot;
            if (string.IsNullOrEmpty(root))
            {
                string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                root = Path.Combine(localAppData, "Programs", "Zalo");
            }
            info.InstallRoot = root;

            if (!Directory.Exists(root))
            {
                info.IsValid = false;
                info.Error = "Khong tim thay thu muc cai dat Zalo chinh thuc tai: " + root;
                return info;
            }

            string stubExe = Path.Combine(root, "Zalo.exe");
            if (File.Exists(stubExe))
            {
                info.StubExePath = stubExe;
            }

            string[] dirs = Directory.GetDirectories(root, "Zalo-*");
            string bestDir = null;
            string bestVerStr = null;
            int[] bestParts = null;

            for (int i = 0; i < dirs.Length; i++)
            {
                string d = dirs[i];
                string folderName = Path.GetFileName(d);
                if (string.IsNullOrEmpty(folderName) || !folderName.StartsWith("Zalo-", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                string candidateExe = Path.Combine(d, "Zalo.exe");
                string candidateAsar = Path.Combine(d, "resources", "app.asar");
                if (!File.Exists(candidateExe) || !File.Exists(candidateAsar))
                {
                    continue;
                }

                string verStr = folderName.Substring(5);
                int[] parts = ParseVersionParts(verStr);
                if (bestParts == null || CompareVersionParts(parts, bestParts) > 0)
                {
                    bestParts = parts;
                    bestDir = d;
                    bestVerStr = verStr;
                }
            }

            if (bestDir == null)
            {
                info.IsValid = false;
                info.Error = "Khong tim thay phien ban Zalo-<version> hop le (co Zalo.exe va resources\\app.asar).";
                return info;
            }

            info.VersionDirName = Path.GetFileName(bestDir);
            info.Version = bestVerStr;
            info.ElectronExePath = Path.Combine(bestDir, "Zalo.exe");
            info.AppAsarPath = Path.Combine(bestDir, "resources", "app.asar");

            try
            {
                X509Certificate cert = X509Certificate.CreateFromSignedFile(info.ElectronExePath);
                info.SignerSubject = cert.Subject;
                info.AuthenticodeStatus = (!string.IsNullOrEmpty(cert.Subject) && cert.Subject.IndexOf("VNG", StringComparison.OrdinalIgnoreCase) >= 0)
                    ? "Signed (VNG)"
                    : "Signed";
            }
            catch (Exception ex)
            {
                info.AuthenticodeStatus = "Unverified (" + ex.Message + ")";
                info.SignerSubject = string.Empty;
            }

            info.IsValid = true;
            return info;
        }

        public PrimaryZaloStatus InspectPrimaryZaloStatus()
        {
            PrimaryZaloStatus status = new PrimaryZaloStatus();
            string roaming = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string zaloData = Path.Combine(roaming, "ZaloData");
            string lockfile = Path.Combine(zaloData, "lockfile");
            string configJson = Path.Combine(zaloData, "config.json");

            status.ZaloDataPath = zaloData;
            status.LockfilePath = lockfile;

            if (File.Exists(configJson))
            {
                status.LastConfigWriteUtc = File.GetLastWriteTimeUtc(configJson).ToString("o");
            }

            List<int> lockingPids = GetLockingPids(lockfile);
            if (lockingPids.Count > 0)
            {
                status.IsRunning = true;
                status.LockingPids.AddRange(lockingPids);

                ProcessTreeSnapshot snapshot = CaptureProcessTreeSnapshot();
                HashSet<int> tree = new HashSet<int>();
                for (int i = 0; i < lockingPids.Count; i++)
                {
                    CollectDescendants(snapshot, lockingPids[i], tree);
                }

                status.ProcessCount = tree.Count;
                long ws;
                long priv;
                SumProcessMemory(tree, out ws, out priv);
                status.TotalWorkingSetBytes = ws;
            }
            else
            {
                status.IsRunning = IsFileExclusivelyLocked(lockfile);
            }

            return status;
        }

        public void EnsureProfileDirectories(string profilePath)
        {
            string roaming = Path.Combine(profilePath, "AppData", "Roaming");
            string zaloData = Path.Combine(roaming, "ZaloData");
            string mediaDir = Path.Combine(zaloData, "media");
            string localTemp = Path.Combine(profilePath, "AppData", "Local", "Temp");
            string documents = Path.Combine(profilePath, "Documents", "Zalo Received Files");
            string desktop = Path.Combine(profilePath, "Desktop");

            Directory.CreateDirectory(mediaDir);
            Directory.CreateDirectory(localTemp);
            Directory.CreateDirectory(documents);
            Directory.CreateDirectory(desktop);

            // Pre-seed migration metadata so Zalo does not search legacy %USERPROFILE%\AppData\Local\ZaloPC
            // or run first-run migration logic against non-existent old folders.
            string zmetaPath = Path.Combine(roaming, "zmeta.json");
            if (!File.Exists(zmetaPath))
            {
                File.WriteAllText(zmetaPath, "{\"failedMigrateCount\":0}", new UTF8Encoding(false));
            }

            string migrateCfgPath = Path.Combine(zaloData, "migrate-config.json");
            if (!File.Exists(migrateCfgPath))
            {
                File.WriteAllText(migrateCfgPath, "{\"zalopc_m_c\":true}", new UTF8Encoding(false));
            }
        }

        public ProfileRuntimeState InspectProfileRuntime(ProfileRecord record, string profilePath)
        {
            return InspectProfileRuntimeWithSnapshot(record, profilePath, CaptureProcessTreeSnapshot());
        }

        internal ProfileRuntimeState InspectProfileRuntimeWithSnapshot(ProfileRecord record, string profilePath, ProcessTreeSnapshot snapshot)
        {
            ProfileRuntimeState state = new ProfileRuntimeState();
            state.ProfileId = record.Id;
            state.ProfileName = record.Name;
            state.ProfilePath = profilePath;
            state.ZaloDataPath = Path.Combine(profilePath, "AppData", "Roaming", "ZaloData");
            state.LockfilePath = Path.Combine(state.ZaloDataPath, "lockfile");
            state.LastLaunchedAt = record.LastLaunchedAt;
            state.StatusLabel = "Da dung";

            List<int> lockingPids = GetLockingPids(state.LockfilePath);
            int mainPid = 0;
            if (lockingPids.Count > 0)
            {
                mainPid = lockingPids[0];
            }
            else if (IsFileExclusivelyLocked(state.LockfilePath))
            {
                // Fallback to recorded runtime PID if Restart Manager did not return it
                int hintPid = ReadPidHint(profilePath);
                if (hintPid > 0 && IsZaloProcessAlive(hintPid))
                {
                    mainPid = hintPid;
                }
            }

            if (mainPid > 0)
            {
                state.IsRunning = true;
                state.MainPid = mainPid;
                state.StatusLabel = "Dang chay";

                HashSet<int> tree = new HashSet<int>();
                CollectDescendants(snapshot, mainPid, tree);
                if (tree.Count == 0)
                {
                    tree.Add(mainPid);
                }

                state.ProcessIds.AddRange(tree);
                state.ProcessCount = tree.Count;

                long ws;
                long priv;
                SumProcessMemory(tree, out ws, out priv);
                state.TotalWorkingSetBytes = ws;
                state.TotalPrivateBytes = priv;

                IntPtr hWnd;
                string title;
                if (FindMainWindowForPids(tree, out hWnd, out title))
                {
                    state.WindowHandleValue = hWnd.ToInt64();
                    state.WindowTitle = title;
                }
                else
                {
                    state.WindowTitle = "(Dang chay nen / khay he thong)";
                }
            }

            return state;
        }

        public OperationResult LaunchProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath)
        {
            if (install == null || !install.IsValid)
            {
                return OperationResult.Fail("INSTALL_NOT_FOUND", install != null ? install.Error : "Zalo installation not found.");
            }

            ProfileRuntimeState existing = InspectProfileRuntime(record, profilePath);
            if (existing.IsRunning)
            {
                return FocusOrRestoreProfile(install, record, profilePath, existing);
            }

            EnsureProfileDirectories(profilePath);

            try
            {
                int launchedPid = StartDetachedProcess(install, record, profilePath);
                if (launchedPid <= 0)
                {
                    return OperationResult.Fail("LAUNCH_FAILED", "Khong the khoi tao tien trinh Zalo.exe.");
                }

                WritePidHint(profilePath, launchedPid);
                return OperationResult.Ok("Da khoi chay phien Zalo cho profile '" + record.Name + "'.", new Dictionary<string, object>
                {
                    { "profileId", record.Id },
                    { "pid", launchedPid },
                    { "executable", install.ElectronExePath },
                    { "profilePath", profilePath }
                });
            }
            catch (Exception ex)
            {
                return OperationResult.Fail("LAUNCH_EXCEPTION", ex.Message);
            }
        }

        public OperationResult FocusOrRestoreProfile(ZaloInstallationInfo install, ProfileRecord record, string profilePath, ProfileRuntimeState state)
        {
            if (state != null && state.WindowHandleValue != 0)
            {
                IntPtr hWnd = new IntPtr(state.WindowHandleValue);
                if (IsIconic(hWnd))
                {
                    ShowWindow(hWnd, SW_RESTORE);
                }
                else
                {
                    ShowWindow(hWnd, SW_SHOW);
                }
                SetForegroundWindow(hWnd);
                return OperationResult.Ok("Da dua cua so profile '" + record.Name + "' len phia truoc.", new Dictionary<string, object>
                {
                    { "profileId", record.Id },
                    { "pid", state.MainPid },
                    { "method", "SetForegroundWindow" }
                });
            }

            // If running in tray (no visible HWND), spawn a transient instance with the same isolated environment;
            // Electron's second-instance handler restores the existing window of that specific profile and exits.
            if (install != null && install.IsValid)
            {
                try
                {
                    StartDetachedProcess(install, record, profilePath);
                    return OperationResult.Ok("Da gui tin hieu khoi phuc cua so cho profile '" + record.Name + "'.", new Dictionary<string, object>
                    {
                        { "profileId", record.Id },
                        { "pid", state != null ? state.MainPid : 0 },
                        { "method", "second-instance-signal" }
                    });
                }
                catch (Exception ex)
                {
                    return OperationResult.Fail("FOCUS_EXCEPTION", ex.Message);
                }
            }

            return OperationResult.Fail("FOCUS_UNAVAILABLE", "Khong tim thay cua so de dua len phia truoc.");
        }

        public OperationResult StopProfile(ProfileRuntimeState state, IEnumerable<int> protectedPrimaryPids)
        {
            if (state == null || !state.IsRunning || state.MainPid <= 0)
            {
                return OperationResult.Ok("Profile hien khong chay.", null);
            }

            HashSet<int> protectedSet = new HashSet<int>();
            if (protectedPrimaryPids != null)
            {
                foreach (int pid in protectedPrimaryPids)
                {
                    protectedSet.Add(pid);
                }
            }

            if (protectedSet.Contains(state.MainPid))
            {
                return OperationResult.Fail("PROTECTED_PRIMARY_PROCESS", "Tu choi dong tien trinh vi trung voi PID Zalo chinh tren he thong.");
            }

            for (int i = 0; i < state.ProcessIds.Count; i++)
            {
                if (protectedSet.Contains(state.ProcessIds[i]))
                {
                    return OperationResult.Fail("PROTECTED_PRIMARY_PROCESS", "Phat hien xung dot PID voi Zalo chinh; huy thao tac dong de bao ve du lieu.");
                }
            }

            // Step 1: Try graceful WM_CLOSE on visible windows of this profile
            if (state.WindowHandleValue != 0)
            {
                PostMessage(new IntPtr(state.WindowHandleValue), WM_CLOSE, IntPtr.Zero, IntPtr.Zero);
            }

            try
            {
                Process mainProc = Process.GetProcessById(state.MainPid);
                if (mainProc != null && !mainProc.HasExited)
                {
                    mainProc.CloseMainWindow();
                    mainProc.WaitForExit(1500);
                }
            }
            catch
            {
            }

            // Step 2: Capture fresh process tree rooted strictly at state.MainPid and stop remaining child/root processes
            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
            HashSet<int> toTerminate = new HashSet<int>();
            if (IsZaloProcessAlive(state.MainPid))
            {
                CollectDescendants(snap, state.MainPid, toTerminate);
            }
            for (int i = 0; i < state.ProcessIds.Count; i++)
            {
                int p = state.ProcessIds[i];
                if (!protectedSet.Contains(p) && IsZaloProcessAlive(p))
                {
                    toTerminate.Add(p);
                }
            }

            int killedCount = 0;
            foreach (int pid in toTerminate)
            {
                if (protectedSet.Contains(pid))
                {
                    continue;
                }
                try
                {
                    Process p = Process.GetProcessById(pid);
                    if (p != null && !p.HasExited)
                    {
                        string name = p.ProcessName;
                        if (name.StartsWith("Zalo", StringComparison.OrdinalIgnoreCase) ||
                            name.StartsWith("zocr", StringComparison.OrdinalIgnoreCase))
                        {
                            p.Kill();
                            p.WaitForExit(1500);
                            killedCount++;
                        }
                    }
                }
                catch
                {
                }
            }

            // Wait briefly for Windows to release lockfile
            for (int attempt = 0; attempt < 10; attempt++)
            {
                if (!IsFileExclusivelyLocked(state.LockfilePath))
                {
                    break;
                }
                Thread.Sleep(150);
            }

            RemovePidHint(state.ProfilePath);
            return OperationResult.Ok("Da dong phien Zalo cua profile '" + state.ProfileName + "'.", new Dictionary<string, object>
            {
                { "profileId", state.ProfileId },
                { "stoppedMainPid", state.MainPid },
                { "terminatedProcessCount", killedCount }
            });
        }

        public void OpenDirectoryInFileManager(string path)
        {
            if (Directory.Exists(path))
            {
                Process.Start("explorer.exe", "\"" + path + "\"");
            }
        }

        public long CalculateDirectorySize(string path)
        {
            if (string.IsNullOrEmpty(path) || !Directory.Exists(path))
            {
                return 0;
            }
            return SumDirectoryBytesSafe(path);
        }

        #region Internal Helpers

        private static int StartDetachedProcess(ZaloInstallationInfo install, ProfileRecord record, string profilePath)
        {
            string roaming = Path.Combine(profilePath, "AppData", "Roaming");
            string local = Path.Combine(profilePath, "AppData", "Local");
            string temp = Path.Combine(local, "Temp");

            SortedDictionary<string, string> env = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            System.Collections.IDictionary currentEnv = Environment.GetEnvironmentVariables();
            foreach (System.Collections.DictionaryEntry entry in currentEnv)
            {
                string k = entry.Key as string;
                string v = entry.Value as string;
                if (!string.IsNullOrEmpty(k) && v != null)
                {
                    env[k] = v;
                }
            }

            env["USERPROFILE"] = profilePath;
            env["APPDATA"] = roaming;
            env["LOCALAPPDATA"] = local;
            env["TEMP"] = temp;
            env["TMP"] = temp;

            string root = Path.GetPathRoot(profilePath);
            if (!string.IsNullOrEmpty(root) && root.Length >= 2 && root[1] == ':')
            {
                env["HOMEDRIVE"] = root.Substring(0, 2);
                env["HOMEPATH"] = profilePath.Substring(2);
            }

            env["MULTIZALO_PROFILE_ID"] = record.Id;
            env["MULTIZALO_PROFILE_NAME"] = record.Name;

            StringBuilder envBuilder = new StringBuilder();
            foreach (KeyValuePair<string, string> kv in env)
            {
                envBuilder.Append(kv.Key);
                envBuilder.Append('=');
                envBuilder.Append(kv.Value);
                envBuilder.Append('\0');
            }
            envBuilder.Append('\0');

            byte[] envBytes = Encoding.Unicode.GetBytes(envBuilder.ToString());
            IntPtr envPtr = Marshal.AllocHGlobal(envBytes.Length);
            try
            {
                Marshal.Copy(envBytes, 0, envPtr, envBytes.Length);

                STARTUPINFO si = new STARTUPINFO();
                si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
                PROCESS_INFORMATION pi;

                string cmdLine = "\"" + install.ElectronExePath + "\"";
                string workDir = Path.GetDirectoryName(install.ElectronExePath);
                uint flags = CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_PROCESS_GROUP;

                bool ok = CreateProcessW(
                    install.ElectronExePath,
                    cmdLine,
                    IntPtr.Zero,
                    IntPtr.Zero,
                    false,
                    flags,
                    envPtr,
                    workDir,
                    ref si,
                    out pi);

                if (!ok)
                {
                    int err = Marshal.GetLastWin32Error();
                    throw new System.ComponentModel.Win32Exception(err, "CreateProcessW failed with Win32 error " + err);
                }

                int pid = (int)pi.dwProcessId;
                if (pi.hThread != IntPtr.Zero) CloseHandle(pi.hThread);
                if (pi.hProcess != IntPtr.Zero) CloseHandle(pi.hProcess);
                return pid;
            }
            finally
            {
                Marshal.FreeHGlobal(envPtr);
            }
        }

        private static int[] ParseVersionParts(string versionStr)
        {
            string[] tokens = versionStr.Split('.');
            int[] nums = new int[tokens.Length];
            for (int i = 0; i < tokens.Length; i++)
            {
                int val;
                if (int.TryParse(tokens[i], out val))
                {
                    nums[i] = val;
                }
            }
            return nums;
        }

        private static int CompareVersionParts(int[] a, int[] b)
        {
            int max = Math.Max(a.Length, b.Length);
            for (int i = 0; i < max; i++)
            {
                int av = i < a.Length ? a[i] : 0;
                int bv = i < b.Length ? b[i] : 0;
                if (av != bv)
                {
                    return av.CompareTo(bv);
                }
            }
            return 0;
        }

        public List<int> GetLockingPids(string filePath)
        {
            List<int> result = new List<int>();
            if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
            {
                return result;
            }

            uint sessionHandle = 0;
            string sessionKey = Guid.NewGuid().ToString("N");
            int startRes = RmStartSession(out sessionHandle, 0, sessionKey);
            if (startRes != 0)
            {
                return result;
            }

            try
            {
                string[] resources = new string[] { filePath };
                int regRes = RmRegisterResources(sessionHandle, 1, resources, 0, null, 0, null);
                if (regRes != 0)
                {
                    return result;
                }

                uint needed = 0;
                uint count = 0;
                uint rebootReasons = 0;
                int listRes = RmGetList(sessionHandle, out needed, ref count, null, ref rebootReasons);
                if (listRes == ERROR_MORE_DATA && needed > 0)
                {
                    RM_PROCESS_INFO[] infos = new RM_PROCESS_INFO[needed];
                    count = needed;
                    listRes = RmGetList(sessionHandle, out needed, ref count, infos, ref rebootReasons);
                    if (listRes == 0)
                    {
                        for (uint i = 0; i < count; i++)
                        {
                            int pid = infos[i].Process.dwProcessId;
                            if (IsZaloProcessAlive(pid) && !result.Contains(pid))
                            {
                                result.Add(pid);
                            }
                        }
                    }
                }
            }
            finally
            {
                RmEndSession(sessionHandle);
            }

            return result;
        }

        private static bool IsFileExclusivelyLocked(string filePath)
        {
            if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
            {
                return false;
            }
            try
            {
                using (FileStream fs = new FileStream(filePath, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                {
                    return false;
                }
            }
            catch (IOException)
            {
                return true;
            }
            catch
            {
                return false;
            }
        }

        private static bool IsZaloProcessAlive(int pid)
        {
            try
            {
                Process p = Process.GetProcessById(pid);
                if (p == null || p.HasExited)
                {
                    return false;
                }
                return p.ProcessName.StartsWith("Zalo", StringComparison.OrdinalIgnoreCase);
            }
            catch
            {
                return false;
            }
        }

        internal sealed class ProcessTreeSnapshot
        {
            public Dictionary<int, List<int>> ChildrenByParent = new Dictionary<int, List<int>>();
            public Dictionary<int, string> ExeNameByPid = new Dictionary<int, string>();
        }

        internal ProcessTreeSnapshot CaptureProcessTreeSnapshot()
        {
            ProcessTreeSnapshot snap = new ProcessTreeSnapshot();
            IntPtr hSnap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if (hSnap == IntPtr.Zero || hSnap == new IntPtr(-1))
            {
                return snap;
            }

            try
            {
                PROCESSENTRY32 entry = new PROCESSENTRY32();
                entry.dwSize = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32));
                if (Process32First(hSnap, ref entry))
                {
                    do
                    {
                        int pid = (int)entry.th32ProcessID;
                        int ppid = (int)entry.th32ParentProcessID;
                        snap.ExeNameByPid[pid] = entry.szExeFile ?? string.Empty;

                        List<int> list;
                        if (!snap.ChildrenByParent.TryGetValue(ppid, out list))
                        {
                            list = new List<int>();
                            snap.ChildrenByParent[ppid] = list;
                        }
                        list.Add(pid);
                    } while (Process32Next(hSnap, ref entry));
                }
            }
            finally
            {
                CloseHandle(hSnap);
            }

            return snap;
        }

        private static void CollectDescendants(ProcessTreeSnapshot snap, int rootPid, HashSet<int> result)
        {
            Queue<int> q = new Queue<int>();
            q.Enqueue(rootPid);
            result.Add(rootPid);

            while (q.Count > 0)
            {
                int cur = q.Dequeue();
                List<int> children;
                if (snap.ChildrenByParent.TryGetValue(cur, out children))
                {
                    for (int i = 0; i < children.Count; i++)
                    {
                        int child = children[i];
                        string exe;
                        if (snap.ExeNameByPid.TryGetValue(child, out exe) &&
                            (exe.StartsWith("Zalo", StringComparison.OrdinalIgnoreCase) ||
                             exe.StartsWith("zocr", StringComparison.OrdinalIgnoreCase)))
                        {
                            if (result.Add(child))
                            {
                                q.Enqueue(child);
                            }
                        }
                    }
                }
            }
        }

        private static void SumProcessMemory(IEnumerable<int> pids, out long totalWorkingSet, out long totalPrivate)
        {
            totalWorkingSet = 0;
            totalPrivate = 0;
            foreach (int pid in pids)
            {
                try
                {
                    Process p = Process.GetProcessById(pid);
                    if (p != null && !p.HasExited)
                    {
                        totalWorkingSet += p.WorkingSet64;
                        totalPrivate += p.PrivateMemorySize64;
                    }
                }
                catch
                {
                }
            }
        }

        private static bool FindMainWindowForPids(HashSet<int> pids, out IntPtr foundHwnd, out string foundTitle)
        {
            IntPtr bestHwnd = IntPtr.Zero;
            string bestTitle = string.Empty;

            EnumWindows(delegate(IntPtr hWnd, IntPtr lParam)
            {
                if (!IsWindowVisible(hWnd))
                {
                    return true;
                }

                uint procId;
                GetWindowThreadProcessId(hWnd, out procId);
                if (!pids.Contains((int)procId))
                {
                    return true;
                }

                RECT rect;
                if (!GetWindowRect(hWnd, out rect))
                {
                    return true;
                }

                int width = rect.Right - rect.Left;
                int height = rect.Bottom - rect.Top;
                if (width < 120 || height < 120)
                {
                    return true;
                }

                int len = GetWindowTextLength(hWnd);
                StringBuilder sb = new StringBuilder(len + 1);
                if (len > 0)
                {
                    GetWindowText(hWnd, sb, sb.Capacity);
                }
                string title = sb.ToString();

                if (bestHwnd == IntPtr.Zero || (!string.IsNullOrEmpty(title) && string.IsNullOrEmpty(bestTitle)))
                {
                    bestHwnd = hWnd;
                    bestTitle = title;
                }
                return true;
            }, IntPtr.Zero);

            foundHwnd = bestHwnd;
            foundTitle = bestTitle;
            return foundHwnd != IntPtr.Zero;
        }

        private static void WritePidHint(string profilePath, int pid)
        {
            try
            {
                string hintPath = Path.Combine(profilePath, ".runtime-pid");
                File.WriteAllText(hintPath, pid.ToString(), new UTF8Encoding(false));
            }
            catch
            {
            }
        }

        private static int ReadPidHint(string profilePath)
        {
            try
            {
                string hintPath = Path.Combine(profilePath, ".runtime-pid");
                if (File.Exists(hintPath))
                {
                    int pid;
                    if (int.TryParse(File.ReadAllText(hintPath).Trim(), out pid))
                    {
                        return pid;
                    }
                }
            }
            catch
            {
            }
            return 0;
        }

        private static void RemovePidHint(string profilePath)
        {
            try
            {
                string hintPath = Path.Combine(profilePath, ".runtime-pid");
                if (File.Exists(hintPath))
                {
                    File.Delete(hintPath);
                }
            }
            catch
            {
            }
        }

        private static long SumDirectoryBytesSafe(string dirPath)
        {
            long total = 0;
            try
            {
                string[] files = Directory.GetFiles(dirPath);
                for (int i = 0; i < files.Length; i++)
                {
                    try
                    {
                        total += new FileInfo(files[i]).Length;
                    }
                    catch
                    {
                    }
                }
                string[] subdirs = Directory.GetDirectories(dirPath);
                for (int i = 0; i < subdirs.Length; i++)
                {
                    total += SumDirectoryBytesSafe(subdirs[i]);
                }
            }
            catch
            {
            }
            return total;
        }

        #endregion
    }
}
