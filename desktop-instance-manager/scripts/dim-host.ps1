$ErrorActionPreference = "Stop"
$CliArgs = @($args)

# Ensure UTF-8 I/O across Windows PowerShell 5.1 and Node.js/Tauri callers
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir

$csharpSource = @'
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Web.Script.Serialization;
using Microsoft.Win32;

namespace DesktopInstanceManager.Host
{
    public sealed class ProfileRecord
    {
        public string id { get; set; }
        public string name { get; set; }
        public string directory_name { get; set; }
        public string created_at { get; set; }
        public string last_launched_at { get; set; }
        public string zalo_user_id { get; set; }
        public string notes { get; set; }
    }

    public sealed class ProfileStoreFile
    {
        public int schema_version { get; set; }
        public List<ProfileRecord> profiles { get; set; }

        public ProfileStoreFile()
        {
            schema_version = 1;
            profiles = new List<ProfileRecord>();
        }
    }

    public static class NativeWin32
    {
        public const uint TH32CS_SNAPPROCESS = 0x00000002;
        public const uint WM_CLOSE = 0x0010;
        public const int SW_RESTORE = 9;
        public const int SW_SHOW = 5;
        public const int ERROR_MORE_DATA = 234;
        public const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
        public const uint CREATE_NEW_PROCESS_GROUP = 0x00000200;
        public const uint CREATE_NO_WINDOW = 0x08000000;

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        public struct STARTUPINFO
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
        public struct PROCESS_INFORMATION
        {
            public IntPtr hProcess;
            public IntPtr hThread;
            public uint dwProcessId;
            public uint dwThreadId;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        public struct PROCESSENTRY32
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
        public struct RM_UNIQUE_PROCESS
        {
            public int dwProcessId;
            public System.Runtime.InteropServices.ComTypes.FILETIME ProcessStartTime;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        public struct RM_PROCESS_INFO
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
        public struct RECT
        {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;
        }

        public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool CreateProcessW(
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

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern IntPtr CreateToolhelp32Snapshot(uint dwFlags, uint th32ProcessID);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool Process32First(IntPtr hSnapshot, ref PROCESSENTRY32 lppe);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool Process32Next(IntPtr hSnapshot, ref PROCESSENTRY32 lppe);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool CloseHandle(IntPtr hObject);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool RemoveDirectoryW(string lpPathName);

        public const uint LOCKFILE_FAIL_IMMEDIATELY = 0x00000001;
        public const uint LOCKFILE_EXCLUSIVE_LOCK = 0x00000002;
        public const uint MOVEFILE_REPLACE_EXISTING = 0x00000001;
        public const uint MOVEFILE_WRITE_THROUGH = 0x00000008;

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool LockFileEx(
            Microsoft.Win32.SafeHandles.SafeFileHandle hFile,
            uint dwFlags,
            uint dwReserved,
            uint nNumberOfBytesToLockLow,
            uint nNumberOfBytesToLockHigh,
            ref NativeOverlapped lpOverlapped);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool UnlockFileEx(
            Microsoft.Win32.SafeHandles.SafeFileHandle hFile,
            uint dwReserved,
            uint nNumberOfBytesToUnlockLow,
            uint nNumberOfBytesToUnlockHigh,
            ref NativeOverlapped lpOverlapped);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool MoveFileExW(
            string lpExistingFileName,
            string lpNewFileName,
            uint dwFlags);

        public const uint FILE_SHARE_READ_WRITE_DELETE = 0x00000007;
        public const uint OPEN_EXISTING = 3;
        public const uint FILE_FLAG_BACKUP_SEMANTICS = 0x02000000;

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFileW(
            string lpFileName,
            uint dwDesiredAccess,
            uint dwShareMode,
            IntPtr lpSecurityAttributes,
            uint dwCreationDisposition,
            uint dwFlagsAndAttributes,
            IntPtr hTemplateFile);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern uint GetFinalPathNameByHandleW(
            Microsoft.Win32.SafeHandles.SafeFileHandle hFile,
            StringBuilder lpszFilePath,
            uint cchFilePath,
            uint dwFlags);

        [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
        public static extern int RmStartSession(out uint pSessionHandle, int dwSessionFlags, string strSessionKey);

        [DllImport("rstrtmgr.dll")]
        public static extern int RmEndSession(uint pSessionHandle);

        [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
        public static extern int RmRegisterResources(uint pSessionHandle, uint nFiles, string[] rgsFilenames, uint nApplications, [In] RM_UNIQUE_PROCESS[] rgApplications, uint nServices, string[] rgsServiceNames);

        [DllImport("rstrtmgr.dll")]
        public static extern int RmGetList(uint dwSessionHandle, out uint pnProcInfoNeeded, ref uint pnProcInfo, [In, Out] RM_PROCESS_INFO[] rgAffectedApps, ref uint lpdwRebootReasons);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool IsWindowVisible(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool IsIconic(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("user32.dll")]
        public static extern int GetWindowTextLength(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        public const uint GW_OWNER = 4;
        public const int GWL_EXSTYLE = -20;
        public const long WS_EX_TOOLWINDOW = 0x00000080L;

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

        [DllImport("user32.dll")]
        public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);

        [DllImport("user32.dll", EntryPoint = "GetWindowLongW")]
        public static extern int GetWindowLong32(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
        public static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

        public static long GetWindowExStyle(IntPtr hWnd)
        {
            if (IntPtr.Size == 8) return GetWindowLongPtr64(hWnd, GWL_EXSTYLE).ToInt64();
            return (long)GetWindowLong32(hWnd, GWL_EXSTYLE);
        }

        [DllImport("user32.dll")]
        public static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr hWnd);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        public struct WINTRUST_FILE_INFO
        {
            public uint cbStruct;
            public IntPtr pcwszFilePath;
            public IntPtr hFile;
            public IntPtr pgKnownSubject;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        public struct WINTRUST_DATA
        {
            public uint cbStruct;
            public IntPtr pPolicyCallbackData;
            public IntPtr pSIPClientData;
            public uint dwUIChoice;
            public uint fdwRevocationChecks;
            public uint dwUnionChoice;
            public IntPtr pFile;
            public uint dwStateAction;
            public IntPtr hWVTStateData;
            public IntPtr pwszURLReference;
            public uint dwProvFlags;
            public uint dwUIContext;
            public IntPtr pSignatureSettings;
        }

        [DllImport("wintrust.dll", ExactSpelling = true, SetLastError = false, CharSet = CharSet.Unicode)]
        public static extern int WinVerifyTrust(IntPtr hwnd, [MarshalAs(UnmanagedType.LPStruct)] Guid pgActionID, IntPtr pWVTData);
    }

    public sealed class ProcessTreeSnapshot
    {
        public Dictionary<int, List<int>> ChildrenByParent = new Dictionary<int, List<int>>();
        public Dictionary<int, string> ExeNameByPid = new Dictionary<int, string>();
    }

    public sealed class HostEngine
    {
        private readonly string _repoRoot;
        private readonly string _dataDir;
        private readonly string _profilesRoot;
        private readonly string _backupsRoot;
        private readonly string _metadataFile;
        private readonly string _lockFile;
        private readonly string _logFile;
        private readonly JavaScriptSerializer _json;
        private readonly object _storeLock = new object();
        private FileStream _heldLockStream = null;
        private int _heldLockDepth = 0;
        private readonly Dictionary<string, object> _profileLocks = new Dictionary<string, object>(StringComparer.OrdinalIgnoreCase);

        private static readonly string[] JunctionFolderNames = new string[]
        {
            "Desktop", "Documents", "Downloads", "Pictures", "Videos", "Music"
        };

        public HostEngine(string repoRoot, string dataDir)
        {
            _repoRoot = Path.GetFullPath(repoRoot);
            string resolvedData = dataDir;
            if (string.IsNullOrEmpty(resolvedData))
            {
                string envDir = Environment.GetEnvironmentVariable("DIM_DATA_DIR");
                if (!string.IsNullOrEmpty(envDir))
                {
                    resolvedData = envDir;
                }
                else
                {
                    string localApp = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                    resolvedData = Path.Combine(localApp, "MultiZaloDesktop");
                }
            }

            if (HasTraversalOrInvalidSyntax(resolvedData))
            {
                throw new ArgumentException("PATH_TRAVERSAL_REJECTED: Unsafe data directory path.");
            }

            _dataDir = Path.GetFullPath(resolvedData);
            string primaryZaloData = Path.GetFullPath(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ZaloData")).TrimEnd('\\');
            string trimmedDataDir = _dataDir.TrimEnd('\\');
            if (string.Equals(trimmedDataDir, primaryZaloData, StringComparison.OrdinalIgnoreCase)
                || trimmedDataDir.StartsWith(primaryZaloData + "\\", StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("PROTECTED_PRIMARY_PATH: Data directory cannot point to or reside inside primary ZaloData.");
            }

            _profilesRoot = Path.Combine(_dataDir, "Profiles");
            _backupsRoot = Path.Combine(_dataDir, "Backups");
            _metadataFile = Path.Combine(_dataDir, "profiles.json");
            _lockFile = Path.Combine(_dataDir, "profiles.lock");
            _logFile = Path.Combine(_dataDir, "logs", "dim-host.log");

            _json = new JavaScriptSerializer();
            _json.MaxJsonLength = 64 * 1024 * 1024;
        }

        public string DataDir { get { return _dataDir; } }

        private sealed class StoreLockScope : IDisposable
        {
            private HostEngine _owner;
            public StoreLockScope(HostEngine owner) { _owner = owner; }
            public void Dispose()
            {
                HostEngine o = Interlocked.Exchange(ref _owner, null);
                if (o != null) o.ReleaseStoreLockScope();
            }
        }

        private sealed class ProfileLockScope : IDisposable
        {
            private object _syncObj;
            private FileStream _stream;
            public ProfileLockScope(object syncObj, FileStream stream)
            {
                _syncObj = syncObj;
                _stream = stream;
            }
            public void Dispose()
            {
                FileStream fs = Interlocked.Exchange(ref _stream, null);
                if (fs != null)
                {
                    try
                    {
                        NativeOverlapped ov = new NativeOverlapped();
                        NativeWin32.UnlockFileEx(fs.SafeFileHandle, 0, 1, 0, ref ov);
                    }
                    catch { }
                    finally { fs.Dispose(); }
                }
                object sync = Interlocked.Exchange(ref _syncObj, null);
                if (sync != null)
                {
                    Monitor.Exit(sync);
                }
            }
        }

        private IDisposable AcquireProfileLockScope(string profilePath, int timeoutMs)
        {
            Directory.CreateDirectory(profilePath);
            string opLockPath = Path.Combine(profilePath, ".dim-profile.lock");
            object syncObj;
            lock (_profileLocks)
            {
                if (!_profileLocks.TryGetValue(opLockPath, out syncObj))
                {
                    syncObj = new object();
                    _profileLocks[opLockPath] = syncObj;
                }
            }

            bool entered = false;
            try
            {
                Monitor.Enter(syncObj, ref entered);
                Stopwatch sw = Stopwatch.StartNew();
                int sleepMs = 2;
                while (true)
                {
                    FileStream fs = null;
                    try
                    {
                        fs = new FileStream(
                            opLockPath,
                            FileMode.OpenOrCreate,
                            FileAccess.ReadWrite,
                            FileShare.ReadWrite | FileShare.Delete);
                        NativeOverlapped ov = new NativeOverlapped();
                        if (NativeWin32.LockFileEx(
                            fs.SafeFileHandle,
                            NativeWin32.LOCKFILE_EXCLUSIVE_LOCK | NativeWin32.LOCKFILE_FAIL_IMMEDIATELY,
                            0, 1, 0, ref ov))
                        {
                            FileStream acquired = fs;
                            fs = null;
                            return new ProfileLockScope(syncObj, acquired);
                        }
                        int winErr = Marshal.GetLastWin32Error();
                        if (winErr != 32 && winErr != 33)
                        {
                            throw new InvalidOperationException("IO_ERROR: LockFileEx failed on .dim-profile.lock with Win32 error " + winErr);
                        }
                    }
                    finally
                    {
                        if (fs != null) fs.Dispose();
                    }

                    if (timeoutMs <= 0 || sw.ElapsedMilliseconds >= timeoutMs)
                    {
                        throw new InvalidOperationException("PROFILE_RUNNING: Profile operation lock (.dim-profile.lock) is currently held by another process.");
                    }
                    Thread.Sleep(sleepMs);
                    sleepMs = Math.Min(20, sleepMs * 2);
                }
            }
            catch
            {
                if (entered) Monitor.Exit(syncObj);
                throw;
            }
        }

        private IDisposable AcquireStoreLockScope()
        {
            bool monitorEntered = false;
            try
            {
                Monitor.Enter(_storeLock, ref monitorEntered);
                if (_heldLockDepth == 0)
                {
                    Directory.CreateDirectory(_dataDir);
                    Stopwatch sw = Stopwatch.StartNew();
                    int sleepMs = 2;
                    while (true)
                    {
                        FileStream fs = null;
                        try
                        {
                            fs = new FileStream(
                                _lockFile,
                                FileMode.OpenOrCreate,
                                FileAccess.ReadWrite,
                                FileShare.ReadWrite | FileShare.Delete);
                            NativeOverlapped ov = new NativeOverlapped();
                            if (NativeWin32.LockFileEx(
                                fs.SafeFileHandle,
                                NativeWin32.LOCKFILE_EXCLUSIVE_LOCK | NativeWin32.LOCKFILE_FAIL_IMMEDIATELY,
                                0, 1, 0, ref ov))
                            {
                                _heldLockStream = fs;
                                fs = null;
                                break;
                            }
                            int winErr = Marshal.GetLastWin32Error();
                            if (winErr != 32 && winErr != 33)
                            {
                                throw new InvalidOperationException("IO_ERROR: LockFileEx failed on profiles.lock with Win32 error " + winErr);
                            }
                        }
                        finally
                        {
                            if (fs != null) fs.Dispose();
                        }

                        if (sw.ElapsedMilliseconds >= 10000)
                        {
                            throw new InvalidOperationException("IO_ERROR: Timed out acquiring exclusive lock on profiles.lock after 10000ms.");
                        }
                        Thread.Sleep(sleepMs);
                        sleepMs = Math.Min(20, sleepMs * 2);
                    }
                }
                _heldLockDepth++;
                return new StoreLockScope(this);
            }
            catch
            {
                if (monitorEntered && _heldLockDepth == 0)
                {
                    Monitor.Exit(_storeLock);
                }
                throw;
            }
        }

        private void ReleaseStoreLockScope()
        {
            try
            {
                _heldLockDepth--;
                if (_heldLockDepth <= 0)
                {
                    _heldLockDepth = 0;
                    if (_heldLockStream != null)
                    {
                        try
                        {
                            NativeOverlapped ov = new NativeOverlapped();
                            NativeWin32.UnlockFileEx(_heldLockStream.SafeFileHandle, 0, 1, 0, ref ov);
                        }
                        catch
                        {
                        }
                        finally
                        {
                            _heldLockStream.Dispose();
                            _heldLockStream = null;
                        }
                    }
                }
            }
            finally
            {
                Monitor.Exit(_storeLock);
            }
        }

        private void EnsureStorageInitialized()
        {
            using (AcquireStoreLockScope())
            {
                Directory.CreateDirectory(_dataDir);
                Directory.CreateDirectory(_profilesRoot);
                Directory.CreateDirectory(_backupsRoot);
                Directory.CreateDirectory(Path.GetDirectoryName(_logFile));
                if (!File.Exists(_metadataFile) && !Directory.Exists(_metadataFile))
                {
                    SaveStore(new ProfileStoreFile());
                }
                SweepAbandonedStagingDirs();
            }
        }

        private void SweepAbandonedStagingDirs()
        {
            DateTime cutoff = DateTime.UtcNow.AddHours(-12);
            SweepDirectoryPattern(_backupsRoot, null, ".partial", cutoff);
            SweepDirectoryPattern(_profilesRoot, ".restore-", null, cutoff);
        }

        private static void SweepDirectoryPattern(string rootDir, string prefix, string suffix, DateTime cutoffUtc)
        {
            if (string.IsNullOrEmpty(rootDir) || !Directory.Exists(rootDir)) return;
            try
            {
                string[] dirs = Directory.GetDirectories(rootDir);
                for (int i = 0; i < dirs.Length; i++)
                {
                    string d = dirs[i];
                    string name = Path.GetFileName(d);
                    if (string.IsNullOrEmpty(name)) continue;
                    bool matchesPrefix = string.IsNullOrEmpty(prefix) || name.StartsWith(prefix, StringComparison.OrdinalIgnoreCase);
                    bool matchesSuffix = string.IsNullOrEmpty(suffix) || name.EndsWith(suffix, StringComparison.OrdinalIgnoreCase);
                    if (!matchesPrefix || !matchesSuffix) continue;
                    try
                    {
                        if (Directory.GetLastWriteTimeUtc(d) < cutoffUtc)
                        {
                            UnlinkJunctionsRecursively(d);
                            VerifyZeroReparsePointsRemaining(d);
                            Directory.Delete(d, true);
                        }
                    }
                    catch
                    {
                    }
                }
            }
            catch
            {
            }
        }

        private ProfileStoreFile LoadStore()
        {
            using (AcquireStoreLockScope())
            {
                EnsureStorageInitialized();
                if (Directory.Exists(_metadataFile) || IsReparsePoint(_metadataFile))
                {
                    throw new InvalidOperationException("JSON_ERROR: corrupted profiles.json (path is a directory or reparse point); original preserved untouched for recovery.");
                }
                byte[] rawBytes = File.ReadAllBytes(_metadataFile);
                if (rawBytes == null || rawBytes.Length == 0)
                {
                    throw new InvalidOperationException("JSON_ERROR: corrupted profiles.json (file is 0 bytes); original file preserved untouched for recovery (restore from backup or repair JSON manually).");
                }
                string raw;
                try
                {
                    UTF8Encoding strictUtf8 = new UTF8Encoding(false, true);
                    raw = strictUtf8.GetString(rawBytes);
                }
                catch (Exception ex)
                {
                    throw new InvalidOperationException("JSON_ERROR: corrupted profiles.json (invalid UTF-8: " + ex.Message + "); original file preserved untouched for recovery.");
                }
                if (raw.Length > 0 && raw[0] == '\uFEFF')
                {
                    raw = raw.Substring(1);
                }
                if (string.IsNullOrWhiteSpace(raw))
                {
                    throw new InvalidOperationException("JSON_ERROR: corrupted profiles.json (whitespace-only); original file preserved untouched for recovery.");
                }

                Dictionary<string, object> rootObj;
                try
                {
                    rootObj = _json.DeserializeObject(raw) as Dictionary<string, object>;
                }
                catch (Exception ex)
                {
                    throw new InvalidOperationException("JSON_ERROR: corrupted profiles.json (invalid JSON syntax: " + ex.Message + "); original file preserved untouched for recovery.");
                }
                if (rootObj == null)
                {
                    throw new InvalidOperationException("JSON_ERROR: corrupted profiles.json (root value must be a JSON object); original file preserved untouched for recovery.");
                }
                if (!rootObj.ContainsKey("schema_version") || rootObj["schema_version"] == null)
                {
                    throw new InvalidOperationException("SCHEMA_ERROR: missing schema_version in profiles.json; original file preserved untouched for recovery.");
                }
                int schemaVer;
                try
                {
                    schemaVer = Convert.ToInt32(rootObj["schema_version"], CultureInfo.InvariantCulture);
                }
                catch
                {
                    throw new InvalidOperationException("SCHEMA_ERROR: invalid non-integer schema_version in profiles.json; original file preserved untouched for recovery.");
                }
                if (schemaVer != 1)
                {
                    throw new InvalidOperationException("SCHEMA_ERROR: unsupported profiles.json schema_version " + schemaVer + " (expected 1); original file preserved untouched for recovery.");
                }

                ProfileStoreFile store;
                try
                {
                    store = _json.Deserialize<ProfileStoreFile>(raw);
                }
                catch (Exception ex)
                {
                    throw new InvalidOperationException("JSON_ERROR: malformed profiles.json schema (" + ex.Message + "); original file preserved untouched for recovery.");
                }
                if (store == null)
                {
                    throw new InvalidOperationException("JSON_ERROR: deserialized profiles.json was null; original file preserved untouched for recovery.");
                }
                if (store.profiles == null)
                {
                    store.profiles = new List<ProfileRecord>();
                }
                return store;
            }
        }

        private void CleanStaleProfilesTmpFiles()
        {
            try
            {
                string[] files = Directory.GetFiles(_dataDir, "profiles.json*.tmp", SearchOption.TopDirectoryOnly);
                for (int i = 0; i < files.Length; i++)
                {
                    string f = files[i];
                    string name = Path.GetFileName(f);
                    if (string.Equals(name, "profiles.json.tmp", StringComparison.OrdinalIgnoreCase) ||
                        (name.StartsWith("profiles.json.", StringComparison.OrdinalIgnoreCase) && name.EndsWith(".tmp", StringComparison.OrdinalIgnoreCase)))
                    {
                        if (!IsReparsePoint(f))
                        {
                            try { File.Delete(f); } catch { }
                        }
                    }
                }
            }
            catch
            {
            }
        }

        private void SaveStore(ProfileStoreFile store)
        {
            using (AcquireStoreLockScope())
            {
                Directory.CreateDirectory(_dataDir);
                CleanStaleProfilesTmpFiles();
                int pid = Process.GetCurrentProcess().Id;
                string uniqueToken = Guid.NewGuid().ToString("N");
                string tmpPath = _metadataFile + "." + pid.ToString(CultureInfo.InvariantCulture) + "." + uniqueToken + ".tmp";
                byte[] utf8Bytes = new UTF8Encoding(false).GetBytes(PrettyPrintJson(_json.Serialize(store)));

                try
                {
                    using (FileStream fs = new FileStream(tmpPath, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                    {
                        fs.Write(utf8Bytes, 0, utf8Bytes.Length);
                        fs.Flush(true);
                    }

                    uint flags = NativeWin32.MOVEFILE_REPLACE_EXISTING | NativeWin32.MOVEFILE_WRITE_THROUGH;
                    int[] retryMs = new int[] { 1, 2, 4, 6, 6, 6 };
                    int lastErr = 0;
                    bool moved = false;
                    for (int attempt = 0; attempt <= retryMs.Length; attempt++)
                    {
                        if (NativeWin32.MoveFileExW(tmpPath, _metadataFile, flags))
                        {
                            moved = true;
                            break;
                        }
                        lastErr = Marshal.GetLastWin32Error();
                        if ((lastErr == 5 || lastErr == 32) && attempt < retryMs.Length)
                        {
                            Thread.Sleep(retryMs[attempt]);
                            continue;
                        }
                        break;
                    }
                    if (!moved)
                    {
                        throw new InvalidOperationException("IO_ERROR: MoveFileExW atomic metadata commit failed with Win32 error " + lastErr);
                    }
                }
                catch
                {
                    try { if (File.Exists(tmpPath)) File.Delete(tmpPath); } catch { }
                    throw;
                }
            }
        }

        private static string PrettyPrintJson(string json)
        {
            if (string.IsNullOrEmpty(json)) return "{}";
            StringBuilder sb = new StringBuilder();
            int indent = 0;
            bool inQuotes = false;
            bool escaped = false;
            for (int i = 0; i < json.Length; i++)
            {
                char ch = json[i];
                if (escaped)
                {
                    sb.Append(ch);
                    escaped = false;
                    continue;
                }
                if (ch == '\\' && inQuotes)
                {
                    sb.Append(ch);
                    escaped = true;
                    continue;
                }
                if (ch == '"')
                {
                    inQuotes = !inQuotes;
                    sb.Append(ch);
                    continue;
                }
                if (inQuotes)
                {
                    sb.Append(ch);
                    continue;
                }
                if (ch == '{' || ch == '[')
                {
                    sb.Append(ch);
                    if (i + 1 < json.Length && json[i + 1] != '}' && json[i + 1] != ']')
                    {
                        sb.Append("\r\n");
                        indent++;
                        sb.Append(' ', indent * 2);
                    }
                }
                else if (ch == '}' || ch == ']')
                {
                    if (i > 0 && json[i - 1] != '{' && json[i - 1] != '[')
                    {
                        sb.Append("\r\n");
                        indent = Math.Max(0, indent - 1);
                        sb.Append(' ', indent * 2);
                    }
                    sb.Append(ch);
                }
                else if (ch == ',')
                {
                    sb.Append(",\r\n");
                    sb.Append(' ', indent * 2);
                }
                else if (ch == ':')
                {
                    sb.Append(": ");
                }
                else if (!char.IsWhiteSpace(ch))
                {
                    sb.Append(ch);
                }
            }
            sb.Append("\r\n");
            return sb.ToString();
        }

        public static bool HasTraversalOrInvalidSyntax(string pathValue)
        {
            if (string.IsNullOrEmpty(pathValue)) return true;
            if (pathValue.IndexOf('\0') >= 0) return true;
            char[] invalid = Path.GetInvalidPathChars();
            if (pathValue.IndexOfAny(invalid) >= 0) return true;
            string[] segments = pathValue.Split(new char[] { '/', '\\' }, StringSplitOptions.None);
            for (int i = 0; i < segments.Length; i++)
            {
                if (segments[i] == "..") return true;
            }
            return false;
        }

        private static readonly HashSet<string> ReservedWindowsNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "CON", "PRN", "AUX", "NUL",
            "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
            "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"
        };

        public static string ResolveSafeChild(string baseRoot, string childRelative)
        {
            if (string.IsNullOrEmpty(baseRoot) || string.IsNullOrEmpty(childRelative))
            {
                throw new ArgumentException("PATH_TRAVERSAL: Empty path argument.");
            }
            if (HasTraversalOrInvalidSyntax(childRelative))
            {
                throw new ArgumentException("PATH_TRAVERSAL: Path traversal or invalid character detected.");
            }
            if (childRelative.StartsWith("/", StringComparison.Ordinal) ||
                childRelative.StartsWith("\\", StringComparison.Ordinal) ||
                Path.IsPathRooted(childRelative) ||
                childRelative.IndexOf(':') >= 0)
            {
                throw new ArgumentException("PATH_TRAVERSAL: Absolute or drive-qualified child path rejected.");
            }

            string[] segs = childRelative.Split(new char[] { '/', '\\' }, StringSplitOptions.None);
            for (int i = 0; i < segs.Length; i++)
            {
                string seg = segs[i];
                if (string.IsNullOrEmpty(seg) || seg == "." || seg == "..")
                {
                    throw new ArgumentException("PATH_TRAVERSAL: Invalid path segment '" + seg + "'.");
                }
                if (seg.EndsWith(" ", StringComparison.Ordinal) || seg.EndsWith(".", StringComparison.Ordinal))
                {
                    throw new ArgumentException("PATH_TRAVERSAL: Segment cannot end with space or dot.");
                }
                string stem = seg.Split('.')[0];
                if (ReservedWindowsNames.Contains(stem))
                {
                    throw new ArgumentException("PATH_TRAVERSAL: Reserved Windows device name rejected.");
                }
            }

            string normalizedRoot = Path.GetFullPath(baseRoot).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            string combined = Path.GetFullPath(Path.Combine(normalizedRoot, childRelative));
            string prefix = normalizedRoot + Path.DirectorySeparatorChar;
            if (!combined.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("PATH_TRAVERSAL: Resolved path escapes root boundary.");
            }
            return combined;
        }

        public static string SlugifyName(string input)
        {
            if (string.IsNullOrWhiteSpace(input)) return "profile";
            string replaced = input.Replace('\u0110', 'd').Replace('\u0111', 'd'); // Đ -> d, đ -> d
            string formD = replaced.Normalize(NormalizationForm.FormD);
            StringBuilder clean = new StringBuilder();
            bool prevDash = false;
            for (int i = 0; i < formD.Length; i++)
            {
                char ch = formD[i];
                UnicodeCategory uc = CharUnicodeInfo.GetUnicodeCategory(ch);
                if (uc == UnicodeCategory.NonSpacingMark)
                {
                    continue;
                }
                char lower = char.ToLowerInvariant(ch);
                if ((lower >= 'a' && lower <= 'z') || (lower >= '0' && lower <= '9'))
                {
                    clean.Append(lower);
                    prevDash = false;
                }
                else if (!prevDash && clean.Length > 0)
                {
                    clean.Append('-');
                    prevDash = true;
                }
                if (clean.Length >= 40) break;
            }
            string slug = clean.ToString().Trim('-');
            return string.IsNullOrEmpty(slug) ? "profile" : slug;
        }

        private static bool ProfileNamesEqualCaseInsensitive(string a, string b)
        {
            string fa = (a ?? string.Empty).Trim().Replace('\u0110', '\u0111').ToLowerInvariant();
            string fb = (b ?? string.Empty).Trim().Replace('\u0110', '\u0111').ToLowerInvariant();
            return string.Equals(fa, fb, StringComparison.Ordinal);
        }

        private static int CountUnicodeScalars(string s)
        {
            if (string.IsNullOrEmpty(s)) return 0;
            int count = 0;
            for (int i = 0; i < s.Length; i++)
            {
                if (char.IsHighSurrogate(s[i]) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1]))
                {
                    i++;
                }
                count++;
            }
            return count;
        }

        private static string ClampUnicodeScalars(string s, int maxScalars)
        {
            if (string.IsNullOrEmpty(s) || maxScalars <= 0) return string.Empty;
            StringBuilder sb = new StringBuilder();
            int count = 0;
            for (int i = 0; i < s.Length && count < maxScalars; i++)
            {
                if (char.IsHighSurrogate(s[i]) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1]))
                {
                    sb.Append(s[i]);
                    sb.Append(s[i + 1]);
                    i++;
                }
                else
                {
                    sb.Append(s[i]);
                }
                count++;
            }
            return sb.ToString();
        }

        private string GetProfilePath(ProfileRecord record)
        {
            if (record == null || string.IsNullOrEmpty(record.directory_name))
            {
                throw new ArgumentException("PATH_TRAVERSAL_REJECTED: Invalid profile record.");
            }
            return ResolveSafeChild(_profilesRoot, record.directory_name);
        }

        private ProfileRecord FindProfile(ProfileStoreFile store, string idOrName)
        {
            if (string.IsNullOrWhiteSpace(idOrName)) return null;
            string needle = idOrName.Trim();
            for (int i = 0; i < store.profiles.Count; i++)
            {
                if (string.Equals(store.profiles[i].id, needle, StringComparison.OrdinalIgnoreCase))
                {
                    return store.profiles[i];
                }
            }
            for (int i = 0; i < store.profiles.Count; i++)
            {
                if (ProfileNamesEqualCaseInsensitive(store.profiles[i].name, needle))
                {
                    return store.profiles[i];
                }
            }
            return null;
        }

        private static string NowRfc3339()
        {
            return DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ", CultureInfo.InvariantCulture);
        }

        public static string SanitizeText(string text)
        {
            if (string.IsNullOrEmpty(text)) return string.Empty;
            string s = text;
            string[] envNames = new string[] { "APPDATA", "LOCALAPPDATA", "USERPROFILE" };
            for (int i = 0; i < envNames.Length; i++)
            {
                string val = Environment.GetEnvironmentVariable(envNames[i]);
                if (!string.IsNullOrEmpty(val))
                {
                    s = Regex.Replace(s, Regex.Escape(val), "<" + envNames[i] + ">", RegexOptions.IgnoreCase);
                }
            }
            string user = Environment.UserName;
            if (!string.IsNullOrEmpty(user) && user.Length >= 2)
            {
                s = Regex.Replace(s, @"\b" + Regex.Escape(user) + @"\b", "<USER>", RegexOptions.IgnoreCase);
            }
            s = Regex.Replace(s, @"(?i)(token|cookie|password|secret|authorization|session_key)\s*[:=]\s*[^\s,;""']+", "$1=[REDACTED]");
            return s;
        }

        private void LogEvent(string level, string action, string message)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(_logFile));
                Dictionary<string, object> entry = new Dictionary<string, object>();
                entry["ts"] = NowRfc3339();
                entry["level"] = level;
                entry["action"] = action;
                entry["message"] = SanitizeText(message);
                File.AppendAllText(_logFile, _json.Serialize(entry) + "\r\n", new UTF8Encoding(false));
            }
            catch
            {
            }
        }

        public Dictionary<string, object> CheckFolderRedirection()
        {
            Dictionary<string, object> res = new Dictionary<string, object>();
            string rawValue = null;
            string expandedValue = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            try
            {
                using (RegistryKey key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders", false))
                {
                    if (key != null)
                    {
                        object v = key.GetValue("AppData", null, RegistryValueOptions.DoNotExpandEnvironmentNames);
                        if (v != null) rawValue = v.ToString();
                    }
                }
            }
            catch
            {
            }

            string expectedExpanded = Path.GetFullPath(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "AppData", "Roaming")).TrimEnd('\\');
            string currentExpanded = Path.GetFullPath(expandedValue).TrimEnd('\\');
            bool isUnc = (!string.IsNullOrEmpty(rawValue) && rawValue.StartsWith(@"\\", StringComparison.Ordinal)) ||
                         currentExpanded.StartsWith(@"\\", StringComparison.Ordinal);
            bool redirected = isUnc || !string.Equals(currentExpanded, expectedExpanded, StringComparison.OrdinalIgnoreCase);

            res["redirected"] = redirected;
            res["is_unc"] = isUnc;
            res["registry_value_sanitized"] = SanitizeText(rawValue ?? currentExpanded);
            res["warning"] = redirected
                ? "HKCU User Shell Folders\\AppData points outside %USERPROFILE%\\AppData\\Roaming; verify SHGetKnownFolderPath isolation."
                : null;
            return res;
        }

        public Dictionary<string, object> DiscoverApp(string appPath, bool nodeFixture)
        {
            Dictionary<string, object> info = new Dictionary<string, object>();
            if (nodeFixture || (!string.IsNullOrEmpty(appPath) &&
                (appPath.EndsWith(".mjs", StringComparison.OrdinalIgnoreCase) || appPath.EndsWith(".js", StringComparison.OrdinalIgnoreCase))))
            {
                string scriptPath = appPath;
                if (string.IsNullOrEmpty(scriptPath))
                {
                    scriptPath = Path.Combine(_repoRoot, "fixtures", "test-desktop-app", "fixture-app.mjs");
                }
                if (HasTraversalOrInvalidSyntax(scriptPath))
                {
                    info["valid"] = false;
                    info["mode"] = "node-fixture";
                    info["error"] = "PATH_TRAVERSAL_REJECTED";
                    return info;
                }
                string fullScript = Path.GetFullPath(scriptPath);
                string nodeExe = FindNodeExe();
                bool valid = !string.IsNullOrEmpty(nodeExe) && File.Exists(fullScript);
                info["ok"] = true;
                info["valid"] = valid;
                info["mode"] = "node-fixture";
                info["executable"] = nodeExe;
                info["script_path"] = fullScript;
                info["version"] = valid ? GetNodeVersion(nodeExe) : null;
                string nodeAuth = !string.IsNullOrEmpty(nodeExe) ? InspectAuthenticode(nodeExe, false) : "Missing";
                info["authenticode_status"] = nodeAuth;
                info["valid"] = valid && nodeAuth.StartsWith("Signed", StringComparison.Ordinal);
                info["executable_sha256"] = !string.IsNullOrEmpty(nodeExe) && File.Exists(nodeExe) ? ComputeSha256(nodeExe) : null;
                info["error"] = (bool)info["valid"]
                    ? null
                    : (!valid ? "Fixture script or node.exe not found." : "node.exe Authenticode verification failed: " + nodeAuth);
                return info;
            }

            if (!string.IsNullOrEmpty(appPath))
            {
                if (HasTraversalOrInvalidSyntax(appPath))
                {
                    info["ok"] = false;
                    info["valid"] = false;
                    info["error"] = "PATH_TRAVERSAL_REJECTED";
                    return info;
                }
                string fullExe = Path.GetFullPath(appPath);
                bool exists = File.Exists(fullExe);
                string customAuth = exists ? InspectAuthenticode(fullExe, true) : "Missing";
                bool customValid = exists && customAuth.StartsWith("Signed", StringComparison.Ordinal);
                info["ok"] = true;
                info["valid"] = customValid;
                info["mode"] = "custom-exe";
                info["executable"] = fullExe;
                info["authenticode_status"] = customAuth;
                info["executable_sha256"] = exists ? ComputeSha256(fullExe) : null;
                info["error"] = !exists
                    ? ("Executable not found: " + SanitizeText(fullExe))
                    : (customValid ? null : ("Executable Authenticode validation failed: " + customAuth));
                return info;
            }

            string localApp = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string installRoot = Path.Combine(localApp, "Programs", "Zalo");
            info["ok"] = true;
            info["mode"] = "official-zalo";
            info["install_root"] = installRoot;

            if (!Directory.Exists(installRoot))
            {
                info["valid"] = false;
                info["error"] = "Official Zalo installation directory not found.";
                return info;
            }

            string stubExe = Path.Combine(installRoot, "Zalo.exe");
            info["stub_exe_path"] = File.Exists(stubExe) ? stubExe : null;

            string[] dirs = Directory.GetDirectories(installRoot, "Zalo-*");
            string bestDir = null;
            string bestVerStr = null;
            int[] bestParts = null;

            for (int i = 0; i < dirs.Length; i++)
            {
                string d = dirs[i];
                string folderName = Path.GetFileName(d);
                if (string.IsNullOrEmpty(folderName) || !folderName.StartsWith("Zalo-", StringComparison.OrdinalIgnoreCase)) continue;
                string exe = Path.Combine(d, "Zalo.exe");
                string asar = Path.Combine(d, "resources", "app.asar");
                if (!File.Exists(exe) || !File.Exists(asar)) continue;
                string ver = folderName.Substring(5);
                int[] parts = ParseVersionParts(ver);
                if (bestParts == null || CompareVersionParts(parts, bestParts) > 0)
                {
                    bestParts = parts;
                    bestDir = d;
                    bestVerStr = ver;
                }
            }

            if (bestDir == null)
            {
                info["valid"] = false;
                info["error"] = "No valid Zalo-<version>\\Zalo.exe + resources\\app.asar found.";
                return info;
            }

            string electronExe = Path.Combine(bestDir, "Zalo.exe");
            string appAsar = Path.Combine(bestDir, "resources", "app.asar");
            string signerSubject;
            string authStatus = InspectAuthenticodeWithSubject(electronExe, out signerSubject);
            bool zaloValid = string.Equals(authStatus, "Signed (VNG)", StringComparison.Ordinal);

            info["valid"] = zaloValid;
            info["version_dir"] = Path.GetFileName(bestDir);
            info["version"] = bestVerStr;
            info["executable"] = electronExe;
            info["app_asar_path"] = appAsar;
            info["executable_sha256"] = ComputeSha256(electronExe);
            info["app_asar_sha256"] = ComputeSha256(appAsar);
            info["authenticode_status"] = authStatus;
            info["signer_subject"] = signerSubject;
            info["error"] = zaloValid ? null : ("Official Zalo Authenticode validation failed: " + authStatus);
            return info;
        }

        private static string FindNodeExe()
        {
            string pathEnv = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            string[] dirs = pathEnv.Split(';');
            for (int i = 0; i < dirs.Length; i++)
            {
                if (string.IsNullOrWhiteSpace(dirs[i])) continue;
                try
                {
                    string candidate = Path.Combine(dirs[i].Trim(), "node.exe");
                    if (File.Exists(candidate)) return candidate;
                }
                catch
                {
                }
            }
            string pf = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "nodejs", "node.exe");
            return File.Exists(pf) ? pf : null;
        }

        private static string GetNodeVersion(string nodeExe)
        {
            try
            {
                ProcessStartInfo psi = new ProcessStartInfo(nodeExe, "--version");
                psi.UseShellExecute = false;
                psi.RedirectStandardOutput = true;
                psi.CreateNoWindow = true;
                using (Process p = Process.Start(psi))
                {
                    string outStr = p.StandardOutput.ReadToEnd().Trim();
                    p.WaitForExit(2000);
                    return outStr;
                }
            }
            catch
            {
                return null;
            }
        }

        private static string InspectAuthenticode(string filePath, bool checkVng)
        {
            string subj;
            return InspectAuthenticodeWithSubject(filePath, out subj, checkVng);
        }

        private static int VerifyWinVerifyTrust(string filePath)
        {
            Guid actionId = new Guid("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");
            IntPtr filePathPtr = IntPtr.Zero;
            IntPtr fileInfoPtr = IntPtr.Zero;
            IntPtr wvtDataPtr = IntPtr.Zero;

            try
            {
                filePathPtr = Marshal.StringToHGlobalUni(filePath);
                NativeWin32.WINTRUST_FILE_INFO fileInfo = new NativeWin32.WINTRUST_FILE_INFO();
                fileInfo.cbStruct = (uint)Marshal.SizeOf(typeof(NativeWin32.WINTRUST_FILE_INFO));
                fileInfo.pcwszFilePath = filePathPtr;
                fileInfo.hFile = IntPtr.Zero;
                fileInfo.pgKnownSubject = IntPtr.Zero;

                fileInfoPtr = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(NativeWin32.WINTRUST_FILE_INFO)));
                Marshal.StructureToPtr(fileInfo, fileInfoPtr, false);

                NativeWin32.WINTRUST_DATA wvtData = new NativeWin32.WINTRUST_DATA();
                wvtData.cbStruct = (uint)Marshal.SizeOf(typeof(NativeWin32.WINTRUST_DATA));
                wvtData.dwUIChoice = 2; // WTD_UI_NONE
                wvtData.fdwRevocationChecks = 0; // WTD_REVOKE_NONE
                wvtData.dwUnionChoice = 1; // WTD_CHOICE_FILE
                wvtData.pFile = fileInfoPtr;
                wvtData.dwStateAction = 1; // WTD_STATEACTION_VERIFY
                wvtData.dwProvFlags = 0x00000100; // WTD_SAFER_FLAG

                wvtDataPtr = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(NativeWin32.WINTRUST_DATA)));
                Marshal.StructureToPtr(wvtData, wvtDataPtr, false);

                int result = NativeWin32.WinVerifyTrust(new IntPtr(-1), actionId, wvtDataPtr);

                NativeWin32.WINTRUST_DATA closeData = (NativeWin32.WINTRUST_DATA)Marshal.PtrToStructure(wvtDataPtr, typeof(NativeWin32.WINTRUST_DATA));
                closeData.dwStateAction = 2; // WTD_STATEACTION_CLOSE
                Marshal.StructureToPtr(closeData, wvtDataPtr, false);
                NativeWin32.WinVerifyTrust(new IntPtr(-1), actionId, wvtDataPtr);

                return result;
            }
            finally
            {
                if (wvtDataPtr != IntPtr.Zero) Marshal.FreeHGlobal(wvtDataPtr);
                if (fileInfoPtr != IntPtr.Zero) Marshal.FreeHGlobal(fileInfoPtr);
                if (filePathPtr != IntPtr.Zero) Marshal.FreeHGlobal(filePathPtr);
            }
        }

        private static string InspectAuthenticodeWithSubject(string filePath, out string subject, bool checkVng = true)
        {
            subject = string.Empty;
            try
            {
                int wvtStatus = VerifyWinVerifyTrust(filePath);
                if (wvtStatus != 0)
                {
                    return "Unverified (WinVerifyTrust=0x" + wvtStatus.ToString("X8", CultureInfo.InvariantCulture) + ")";
                }

                System.Security.Cryptography.X509Certificates.X509Certificate cert =
                    System.Security.Cryptography.X509Certificates.X509Certificate.CreateFromSignedFile(filePath);
                subject = cert.Subject ?? string.Empty;
                if (checkVng && Regex.IsMatch(subject, @"(?:^|[,;\s])(?:CN|O)\s*=\s*""?VNG GROUP JSC""?(?:$|[,;])", RegexOptions.IgnoreCase))
                {
                    return "Signed (VNG)";
                }
                return "Signed";
            }
            catch (Exception ex)
            {
                return "Unverified (" + ex.Message + ")";
            }
        }

        private static string ComputeSha256(string filePath)
        {
            using (FileStream fs = File.OpenRead(filePath))
            using (SHA256 sha = SHA256.Create())
            {
                byte[] hash = sha.ComputeHash(fs);
                StringBuilder sb = new StringBuilder(hash.Length * 2);
                for (int i = 0; i < hash.Length; i++)
                {
                    sb.Append(hash[i].ToString("x2", CultureInfo.InvariantCulture));
                }
                return sb.ToString();
            }
        }

        private static int[] ParseVersionParts(string ver)
        {
            string[] tokens = ver.Split('.');
            int[] nums = new int[tokens.Length];
            for (int i = 0; i < tokens.Length; i++)
            {
                int val;
                if (int.TryParse(tokens[i], out val)) nums[i] = val;
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
                if (av != bv) return av.CompareTo(bv);
            }
            return 0;
        }

        public ProcessTreeSnapshot CaptureProcessTreeSnapshot()
        {
            ProcessTreeSnapshot snap = new ProcessTreeSnapshot();
            IntPtr hSnap = NativeWin32.CreateToolhelp32Snapshot(NativeWin32.TH32CS_SNAPPROCESS, 0);
            if (hSnap == IntPtr.Zero || hSnap == new IntPtr(-1)) return snap;
            try
            {
                NativeWin32.PROCESSENTRY32 entry = new NativeWin32.PROCESSENTRY32();
                entry.dwSize = (uint)Marshal.SizeOf(typeof(NativeWin32.PROCESSENTRY32));
                if (NativeWin32.Process32First(hSnap, ref entry))
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
                    } while (NativeWin32.Process32Next(hSnap, ref entry));
                }
            }
            finally
            {
                NativeWin32.CloseHandle(hSnap);
            }
            return snap;
        }

        private static bool TryGetProcessStartFileTimeUtc(int pid, out long fileTimeUtc)
        {
            fileTimeUtc = 0L;
            if (pid <= 0) return false;
            try
            {
                Process p = Process.GetProcessById(pid);
                if (p == null || p.HasExited) return false;
                fileTimeUtc = p.StartTime.ToFileTimeUtc();
                return fileTimeUtc > 0L;
            }
            catch
            {
                return false;
            }
        }

        private static void CollectDescendants(ProcessTreeSnapshot snap, int rootPid, HashSet<int> result, bool includeNodeAndFixture)
        {
            Queue<int> q = new Queue<int>();
            q.Enqueue(rootPid);
            result.Add(rootPid);
            while (q.Count > 0)
            {
                int cur = q.Dequeue();
                long parentFt;
                if (!TryGetProcessStartFileTimeUtc(cur, out parentFt)) continue;

                List<int> children;
                if (snap.ChildrenByParent.TryGetValue(cur, out children))
                {
                    for (int i = 0; i < children.Count; i++)
                    {
                        int child = children[i];
                        if (child <= 0 || child == cur) continue;
                        string exe;
                        if (snap.ExeNameByPid.TryGetValue(child, out exe) &&
                            (exe.StartsWith("Zalo", StringComparison.OrdinalIgnoreCase) ||
                             exe.StartsWith("zocr", StringComparison.OrdinalIgnoreCase) ||
                             (includeNodeAndFixture && (exe.StartsWith("node", StringComparison.OrdinalIgnoreCase) || exe.StartsWith("dim-test-fixture", StringComparison.OrdinalIgnoreCase) || exe.StartsWith("xf-desktop-fixture", StringComparison.OrdinalIgnoreCase)))))
                        {
                            long childFt;
                            if (TryGetProcessStartFileTimeUtc(child, out childFt) && childFt >= parentFt)
                            {
                                if (result.Add(child)) q.Enqueue(child);
                            }
                        }
                    }
                }
            }
        }

        private static readonly object _rmSessionLock = new object();

        public List<int> GetLockingPids(string filePath)
        {
            List<int> result = new List<int>();
            if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath)) return result;

            lock (_rmSessionLock)
            {
                uint sessionHandle = 0;
                string sessionKey = Guid.NewGuid().ToString("N");
                int startRes = NativeWin32.RmStartSession(out sessionHandle, 0, sessionKey);
                if (startRes != 0) return result;

                try
                {
                    string[] resources = new string[] { filePath };
                    int regRes = NativeWin32.RmRegisterResources(sessionHandle, 1, resources, 0, null, 0, null);
                    if (regRes != 0) return result;

                    uint needed = 0;
                    uint count = 0;
                    uint rebootReasons = 0;
                    int listRes = NativeWin32.RmGetList(sessionHandle, out needed, ref count, null, ref rebootReasons);
                    if (listRes == NativeWin32.ERROR_MORE_DATA && needed > 0)
                    {
                        NativeWin32.RM_PROCESS_INFO[] infos = new NativeWin32.RM_PROCESS_INFO[needed];
                        count = needed;
                        listRes = NativeWin32.RmGetList(sessionHandle, out needed, ref count, infos, ref rebootReasons);
                        if (listRes == 0)
                        {
                            for (uint i = 0; i < count; i++)
                            {
                                int pid = infos[i].Process.dwProcessId;
                                if (IsManagedAppProcessAlive(pid) && !result.Contains(pid))
                                {
                                    result.Add(pid);
                                }
                            }
                        }
                    }
                }
                finally
                {
                    NativeWin32.RmEndSession(sessionHandle);
                }
            }
            return result;
        }

        private static bool IsFileExclusivelyLocked(string filePath)
        {
            if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath)) return false;
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

        private static bool IsManagedAppProcessAlive(int pid)
        {
            if (pid <= 0 || pid == Process.GetCurrentProcess().Id) return false;
            try
            {
                Process p = Process.GetProcessById(pid);
                if (p == null || p.HasExited) return false;
                string name = p.ProcessName;
                return name.StartsWith("Zalo", StringComparison.OrdinalIgnoreCase) ||
                       name.Equals("node", StringComparison.OrdinalIgnoreCase) ||
                       name.StartsWith("dim-test-fixture", StringComparison.OrdinalIgnoreCase) ||
                       name.StartsWith("xf-desktop-fixture", StringComparison.OrdinalIgnoreCase);
            }
            catch
            {
                return false;
            }
        }

        private bool IsLockfileReady(string lockfilePath)
        {
            if (string.IsNullOrEmpty(lockfilePath) || !File.Exists(lockfilePath)) return false;
            if (IsFileExclusivelyLocked(lockfilePath)) return true;
            List<int> rm = GetLockingPids(lockfilePath);
            if (rm != null && rm.Count > 0) return true;
            try
            {
                FileInfo fi = new FileInfo(lockfilePath);
                if (fi.Exists && fi.Length > 0 && ReadLockfilePid(lockfilePath) > 0) return true;
            }
            catch
            {
            }
            return false;
        }

        private int VerifyLiveLeasePid(string profilePath, string expectedProfileId)
        {
            string leasePath = Path.Combine(profilePath, ".runtime-lease.json");
            if (!File.Exists(leasePath) || IsReparsePoint(leasePath)) return 0;
            try
            {
                string raw = File.ReadAllText(leasePath, Encoding.UTF8).Trim();
                if (string.IsNullOrEmpty(raw)) return 0;
                Dictionary<string, object> obj = _json.Deserialize<Dictionary<string, object>>(raw);
                if (obj == null) return 0;
                string profId = obj.ContainsKey("profile_id") && obj["profile_id"] != null ? obj["profile_id"].ToString() : string.Empty;
                if (!string.Equals(profId, expectedProfileId, StringComparison.OrdinalIgnoreCase)) return 0;
                int pid = obj.ContainsKey("pid") && obj["pid"] != null ? Convert.ToInt32(obj["pid"], CultureInfo.InvariantCulture) : 0;
                if (pid <= 0 || pid == Process.GetCurrentProcess().Id) return 0;
                long expectedFt = obj.ContainsKey("process_create_time") && obj["process_create_time"] != null
                    ? Convert.ToInt64(obj["process_create_time"], CultureInfo.InvariantCulture)
                    : 0L;
                string expectedExe = obj.ContainsKey("executable_path") && obj["executable_path"] != null
                    ? obj["executable_path"].ToString()
                    : string.Empty;

                Process p = Process.GetProcessById(pid);
                if (p == null || p.HasExited) return 0;
                long liveFt = p.StartTime.ToFileTimeUtc();
                if (expectedFt <= 0 || liveFt <= 0 || expectedFt != liveFt) return 0;
                if (!string.IsNullOrEmpty(expectedExe))
                {
                    string expectedName = Path.GetFileNameWithoutExtension(expectedExe);
                    if (!string.IsNullOrEmpty(expectedName) && !string.Equals(p.ProcessName, expectedName, StringComparison.OrdinalIgnoreCase))
                    {
                        return 0;
                    }
                }
                return pid;
            }
            catch
            {
                return 0;
            }
        }

        private int ReadLockfilePid(string lockfilePath)
        {
            if (!File.Exists(lockfilePath)) return 0;
            try
            {
                string raw = File.ReadAllText(lockfilePath, Encoding.UTF8).Trim();
                if (string.IsNullOrEmpty(raw)) return 0;
                int direct;
                if (int.TryParse(raw, out direct)) return direct;
                if (raw.StartsWith("{"))
                {
                    Dictionary<string, object> obj = _json.Deserialize<Dictionary<string, object>>(raw);
                    if (obj != null && obj.ContainsKey("pid") && obj["pid"] != null)
                    {
                        return Convert.ToInt32(obj["pid"], CultureInfo.InvariantCulture);
                    }
                }
            }
            catch
            {
            }
            return 0;
        }

        private Dictionary<string, object> ReadRuntimeHint(string profilePath)
        {
            string hintPath = Path.Combine(profilePath, ".runtime-pid");
            if (!File.Exists(hintPath)) return null;
            try
            {
                string raw = File.ReadAllText(hintPath, Encoding.UTF8).Trim();
                if (string.IsNullOrEmpty(raw)) return null;
                int pid;
                if (int.TryParse(raw, out pid))
                {
                    Dictionary<string, object> d = new Dictionary<string, object>();
                    d["pid"] = pid;
                    return d;
                }
                if (raw.StartsWith("{"))
                {
                    return _json.Deserialize<Dictionary<string, object>>(raw);
                }
            }
            catch
            {
            }
            return null;
        }

        private void WriteRuntimeHint(string profilePath, string profileId, int pid, string mode, string appPath, string exeToRun)
        {
            string startedAt = NowRfc3339();
            long createFt = 0L;
            string resolvedExe = exeToRun ?? string.Empty;
            try
            {
                Process p = Process.GetProcessById(pid);
                if (p != null && !p.HasExited)
                {
                    createFt = p.StartTime.ToFileTimeUtc();
                    try
                    {
                        if (p.MainModule != null && !string.IsNullOrEmpty(p.MainModule.FileName))
                        {
                            resolvedExe = p.MainModule.FileName;
                        }
                    }
                    catch { }
                }
            }
            catch { }

            if (createFt > 0 && !string.IsNullOrEmpty(profileId))
            {
                string leasePath = Path.Combine(profilePath, ".runtime-lease.json");
                Dictionary<string, object> lease = new Dictionary<string, object>();
                lease["pid"] = pid;
                lease["process_create_time"] = createFt;
                lease["executable_path"] = resolvedExe;
                lease["profile_id"] = profileId;
                lease["spawned_at_utc"] = startedAt;
                File.WriteAllText(leasePath, PrettyPrintJson(_json.Serialize(lease)) + "\n", new UTF8Encoding(false));
            }

            string hintPath = Path.Combine(profilePath, ".runtime-pid");
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["pid"] = pid;
            d["mode"] = mode;
            d["app_path"] = appPath;
            d["started_at"] = startedAt;
            File.WriteAllText(hintPath, _json.Serialize(d), new UTF8Encoding(false));
        }

        private void RemoveRuntimeHint(string profilePath)
        {
            try
            {
                string leasePath = Path.Combine(profilePath, ".runtime-lease.json");
                if (File.Exists(leasePath)) File.Delete(leasePath);
            }
            catch
            {
            }
            try
            {
                string hintPath = Path.Combine(profilePath, ".runtime-pid");
                if (File.Exists(hintPath)) File.Delete(hintPath);
            }
            catch
            {
            }
        }

        private static string StripVerbatimPrefix(string path)
        {
            if (string.IsNullOrEmpty(path)) return string.Empty;
            if (path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
            {
                return @"\\" + path.Substring(8);
            }
            if (path.StartsWith(@"\\?\", StringComparison.OrdinalIgnoreCase))
            {
                return path.Substring(4);
            }
            return path;
        }

        private static string TryGetFinalPathByHandle(string existingPath)
        {
            try
            {
                using (Microsoft.Win32.SafeHandles.SafeFileHandle h = NativeWin32.CreateFileW(
                    existingPath,
                    0u,
                    NativeWin32.FILE_SHARE_READ_WRITE_DELETE,
                    IntPtr.Zero,
                    NativeWin32.OPEN_EXISTING,
                    NativeWin32.FILE_FLAG_BACKUP_SEMANTICS,
                    IntPtr.Zero))
                {
                    if (h == null || h.IsInvalid) return null;
                    StringBuilder sb = new StringBuilder(2048);
                    uint len = NativeWin32.GetFinalPathNameByHandleW(h, sb, (uint)sb.Capacity, 0u);
                    if (len > 0 && len < sb.Capacity)
                    {
                        return StripVerbatimPrefix(sb.ToString());
                    }
                }
            }
            catch
            {
            }
            return null;
        }

        private static string ResolveEffectivePath(string baseDir, string candidate)
        {
            string raw = StripVerbatimPrefix((candidate ?? string.Empty).Trim());
            string baseClean = StripVerbatimPrefix((baseDir ?? string.Empty).Trim());
            string combined = Path.IsPathRooted(raw) ? raw : Path.Combine(baseClean, raw);
            string lexical = StripVerbatimPrefix(Path.GetFullPath(combined));

            if (Directory.Exists(lexical) || File.Exists(lexical))
            {
                string directFinal = TryGetFinalPathByHandle(lexical);
                if (!string.IsNullOrEmpty(directFinal))
                {
                    return Path.GetFullPath(directFinal).TrimEnd('\\', '/');
                }
            }

            string root = Path.GetPathRoot(lexical) ?? string.Empty;
            string remainder = lexical.Substring(root.Length).TrimStart('\\', '/');
            string[] parts = remainder.Length == 0
                ? new string[0]
                : remainder.Split(new char[] { '\\', '/' }, StringSplitOptions.RemoveEmptyEntries);

            string cur = root;
            int deepestIdx = -1;
            string deepestResolved = root;
            for (int i = 0; i < parts.Length; i++)
            {
                cur = Path.Combine(cur, parts[i]);
                if (Directory.Exists(cur) || File.Exists(cur))
                {
                    string resolvedCur = TryGetFinalPathByHandle(cur);
                    if (!string.IsNullOrEmpty(resolvedCur))
                    {
                        cur = Path.GetFullPath(resolvedCur);
                    }
                    deepestIdx = i;
                    deepestResolved = cur;
                }
            }

            if (deepestIdx >= 0)
            {
                string result = deepestResolved;
                for (int j = deepestIdx + 1; j < parts.Length; j++)
                {
                    result = Path.Combine(result, parts[j]);
                }
                return Path.GetFullPath(result).TrimEnd('\\', '/');
            }

            return lexical.TrimEnd('\\', '/');
        }

        private static bool IsPathTrulyContainedInProfile(string profilePath, string candidatePath)
        {
            string resolvedProf = ResolveEffectivePath(profilePath, profilePath);
            string resolvedCand = ResolveEffectivePath(profilePath, candidatePath);
            if (string.IsNullOrEmpty(resolvedProf) || string.IsNullOrEmpty(resolvedCand)) return false;
            if (string.Equals(resolvedCand, resolvedProf, StringComparison.OrdinalIgnoreCase)) return true;
            return resolvedCand.StartsWith(resolvedProf + "\\", StringComparison.OrdinalIgnoreCase);
        }

        private static bool PathsOverlapOrEqual(string resolvedA, string resolvedB)
        {
            if (string.IsNullOrEmpty(resolvedA) || string.IsNullOrEmpty(resolvedB)) return false;
            string a = resolvedA.TrimEnd('\\', '/');
            string b = resolvedB.TrimEnd('\\', '/');
            if (string.Equals(a, b, StringComparison.OrdinalIgnoreCase)) return true;
            return a.StartsWith(b + "\\", StringComparison.OrdinalIgnoreCase) ||
                   b.StartsWith(a + "\\", StringComparison.OrdinalIgnoreCase);
        }

        private static List<string> ReadConfiguredStorageLocationsRaw(string zaloDataPath)
        {
            List<string> rawList = new List<string>();
            if (string.IsNullOrEmpty(zaloDataPath)) return rawList;
            string cfgPath = Path.Combine(zaloDataPath, "config.json");
            if (!File.Exists(cfgPath)) return rawList;
            try
            {
                JavaScriptSerializer ser = new JavaScriptSerializer();
                Dictionary<string, object> cfg = ser.Deserialize<Dictionary<string, object>>(File.ReadAllText(cfgPath, Encoding.UTF8));
                if (cfg == null) return rawList;

                if (cfg.ContainsKey("db_path") && cfg["db_path"] != null)
                {
                    Dictionary<string, object> dbObj = cfg["db_path"] as Dictionary<string, object>;
                    if (dbObj != null && dbObj.ContainsKey("location") && dbObj["location"] != null)
                    {
                        string loc = dbObj["location"].ToString().Trim();
                        if (loc.Length > 0) rawList.Add(Path.IsPathRooted(loc) ? loc : Path.Combine(zaloDataPath, loc));
                    }
                    else if (cfg["db_path"] is string)
                    {
                        string loc = cfg["db_path"].ToString().Trim();
                        if (loc.Length > 0) rawList.Add(Path.IsPathRooted(loc) ? loc : Path.Combine(zaloDataPath, loc));
                    }
                }

                if (cfg.ContainsKey("resource") && cfg["resource"] is Dictionary<string, object>)
                {
                    Dictionary<string, object> resObj = (Dictionary<string, object>)cfg["resource"];
                    if (resObj.ContainsKey("location") && resObj["location"] != null)
                    {
                        Dictionary<string, object> locObj = resObj["location"] as Dictionary<string, object>;
                        if (locObj != null && locObj.ContainsKey("path") && locObj["path"] != null)
                        {
                            string p = locObj["path"].ToString().Trim();
                            if (p.Length > 0) rawList.Add(Path.IsPathRooted(p) ? p : Path.Combine(zaloDataPath, p));
                        }
                        else if (resObj["location"] is string)
                        {
                            string p = resObj["location"].ToString().Trim();
                            if (p.Length > 0) rawList.Add(Path.IsPathRooted(p) ? p : Path.Combine(zaloDataPath, p));
                        }
                    }
                }
            }
            catch
            {
            }
            return rawList;
        }

        private static List<string> GetExternalPathsOutsideProfile(string profilePath)
        {
            List<string> externals = new List<string>();
            string[] zdirs = new string[]
            {
                Path.Combine(profilePath, "AppData", "Roaming", "ZaloData"),
                Path.Combine(profilePath, "Library", "Application Support", "ZaloData"),
                Path.Combine(profilePath, ".config", "ZaloData")
            };

            for (int z = 0; z < zdirs.Length; z++)
            {
                string zdir = zdirs[z];
                List<string> rawPaths = ReadConfiguredStorageLocationsRaw(zdir);
                for (int i = 0; i < rawPaths.Count; i++)
                {
                    string resolved = ResolveEffectivePath(zdir, rawPaths[i]);
                    if (!IsPathTrulyContainedInProfile(profilePath, resolved))
                    {
                        bool seen = false;
                        for (int k = 0; k < externals.Count; k++)
                        {
                            if (string.Equals(externals[k], resolved, StringComparison.OrdinalIgnoreCase))
                            {
                                seen = true;
                                break;
                            }
                        }
                        if (!seen) externals.Add(resolved);
                    }
                }
            }
            return externals;
        }

        private static string DetectZaloUserId(string zaloDataPath)
        {
            try
            {
                string cfgPath = Path.Combine(zaloDataPath, "config.json");
                if (File.Exists(cfgPath))
                {
                    JavaScriptSerializer ser = new JavaScriptSerializer();
                    Dictionary<string, object> cfg = ser.Deserialize<Dictionary<string, object>>(File.ReadAllText(cfgPath, Encoding.UTF8));
                    if (cfg != null && cfg.ContainsKey("db_path") && cfg["db_path"] is Dictionary<string, object>)
                    {
                        Dictionary<string, object> dbPath = (Dictionary<string, object>)cfg["db_path"];
                        string loc = dbPath.ContainsKey("location") && dbPath["location"] != null ? dbPath["location"].ToString() : null;
                        if (!string.IsNullOrWhiteSpace(loc) && Directory.Exists(loc))
                        {
                            string fromCfg = PickNewestUidDir(loc, @"^zalo_(\d{6,})$");
                            if (!string.IsNullOrEmpty(fromCfg)) return fromCfg;
                            string nestedProd = Path.Combine(loc, "Database", "_production");
                            if (Directory.Exists(nestedProd))
                            {
                                string fromNested = PickNewestUidDir(nestedProd, @"^(\d{6,})$");
                                if (!string.IsNullOrEmpty(fromNested)) return fromNested;
                            }
                        }
                    }
                }
            }
            catch
            {
            }

            string prodDir = Path.Combine(zaloDataPath, "Database", "_production");
            if (!Directory.Exists(prodDir)) return null;
            return PickNewestUidDir(prodDir, @"^(\d{6,})$");
        }

        private static string PickNewestUidDir(string parentDir, string regexPattern)
        {
            try
            {
                string[] dirs = Directory.GetDirectories(parentDir);
                DateTime bestTime = DateTime.MinValue;
                string bestUid = null;
                for (int i = 0; i < dirs.Length; i++)
                {
                    string name = Path.GetFileName(dirs[i]);
                    if (string.IsNullOrEmpty(name)) continue;
                    Match m = Regex.Match(name, regexPattern, RegexOptions.IgnoreCase);
                    if (!m.Success) continue;
                    string uid = m.Groups[1].Value;
                    DateTime mtime = Directory.GetLastWriteTimeUtc(dirs[i]);
                    if (bestUid == null || mtime > bestTime || (mtime == bestTime && string.CompareOrdinal(uid, bestUid) > 0))
                    {
                        bestTime = mtime;
                        bestUid = uid;
                    }
                }
                return bestUid;
            }
            catch
            {
                return null;
            }
        }

        public Dictionary<string, object> InspectPrimaryZalo(ProcessTreeSnapshot snap)
        {
            string roaming = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string zaloData = Path.Combine(roaming, "ZaloData");
            string lockfile = Path.Combine(zaloData, "lockfile");

            List<int> lockingPids = GetLockingPids(lockfile);
            HashSet<int> tree = new HashSet<int>();
            for (int i = 0; i < lockingPids.Count; i++)
            {
                CollectDescendants(snap, lockingPids[i], tree, false);
            }

            bool isRunning = lockingPids.Count > 0 || IsFileExclusivelyLocked(lockfile);
            long ws, priv;
            SumProcessMemory(tree, out ws, out priv);

            Dictionary<string, object> primary = new Dictionary<string, object>();
            primary["is_running"] = isRunning;
            primary["zalo_data_path"] = zaloData;
            primary["lockfile_path"] = lockfile;
            primary["locking_pids"] = lockingPids;
            primary["all_pids"] = new List<int>(tree);
            primary["process_count"] = tree.Count;
            primary["total_working_set_bytes"] = ws;
            primary["zalo_user_id"] = DetectZaloUserId(zaloData);
            return primary;
        }

        private static void SumProcessMemory(IEnumerable<int> pids, out long totalWs, out long totalPriv)
        {
            totalWs = 0;
            totalPriv = 0;
            foreach (int pid in pids)
            {
                try
                {
                    Process p = Process.GetProcessById(pid);
                    if (p != null && !p.HasExited)
                    {
                        totalWs += p.WorkingSet64;
                        totalPriv += p.PrivateMemorySize64;
                    }
                }
                catch
                {
                }
            }
        }

        public static bool FindMainWindowForPids(HashSet<int> pids, out IntPtr foundHwnd, out string foundTitle)
        {
            return FindMainWindowForProfile(0, pids, out foundHwnd, out foundTitle);
        }

        public static bool FindMainWindowForProfile(int mainPid, HashSet<int> pids, out IntPtr foundHwnd, out string foundTitle)
        {
            IntPtr bestHwnd = IntPtr.Zero;
            string bestTitle = string.Empty;

            NativeWin32.EnumWindows(delegate(IntPtr hWnd, IntPtr lParam)
            {
                if (!NativeWin32.IsWindowVisible(hWnd)) return true;
                uint procId;
                NativeWin32.GetWindowThreadProcessId(hWnd, out procId);
                int pid = (int)procId;
                if (pid <= 0 || (!pids.Contains(pid) && pid != mainPid)) return true;

                StringBuilder clsSb = new StringBuilder(256);
                int clsLen = NativeWin32.GetClassName(hWnd, clsSb, clsSb.Capacity);
                string clsName = clsLen > 0 ? clsSb.ToString() : string.Empty;
                if (clsName != "Chrome_WidgetWin_1" && clsName != "DimTestMainWindow") return true;

                if (NativeWin32.GetWindow(hWnd, NativeWin32.GW_OWNER) != IntPtr.Zero) return true;
                if ((NativeWin32.GetWindowExStyle(hWnd) & NativeWin32.WS_EX_TOOLWINDOW) != 0L) return true;

                NativeWin32.RECT rect;
                if (!NativeWin32.GetWindowRect(hWnd, out rect)) return true;
                int w = rect.Right - rect.Left;
                int h = rect.Bottom - rect.Top;
                if (w < 400 || h < 400) return true;

                int len = NativeWin32.GetWindowTextLength(hWnd);
                if (len <= 0 || len >= 4096) return true;
                StringBuilder sb = new StringBuilder(len + 1);
                NativeWin32.GetWindowText(hWnd, sb, sb.Capacity);
                string title = sb.ToString();
                if (string.IsNullOrWhiteSpace(title)) return true;

                if (mainPid > 0 && pid == mainPid)
                {
                    bestHwnd = hWnd;
                    bestTitle = title;
                    return false;
                }

                if (bestHwnd == IntPtr.Zero)
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

        private Dictionary<string, object> InspectProfileRuntime(ProfileRecord rec, ProcessTreeSnapshot snap, bool includeSize)
        {
            string profilePath = GetProfilePath(rec);
            string zaloDataPath = Path.Combine(profilePath, "AppData", "Roaming", "ZaloData");
            string lockfilePath = Path.Combine(zaloDataPath, "lockfile");

            List<int> rmPids = GetLockingPids(lockfilePath);
            int lockJsonPid = ReadLockfilePid(lockfilePath);
            Dictionary<string, object> hint = ReadRuntimeHint(profilePath);
            int hintPid = (hint != null && hint.ContainsKey("pid") && hint["pid"] != null)
                ? Convert.ToInt32(hint["pid"], CultureInfo.InvariantCulture)
                : 0;
            int verifiedLeasePid = VerifyLiveLeasePid(profilePath, rec.id);

            int mainPid = 0;
            string lifecycleState = "stopped";
            if (rmPids.Count > 0)
            {
                mainPid = rmPids[0];
                lifecycleState = "running";
            }
            else if (lockJsonPid > 0 && IsManagedAppProcessAlive(lockJsonPid))
            {
                mainPid = lockJsonPid;
                lifecycleState = "running";
            }
            else if (IsFileExclusivelyLocked(lockfilePath) && hintPid > 0 && IsManagedAppProcessAlive(hintPid))
            {
                mainPid = hintPid;
                lifecycleState = "running";
            }
            else if (verifiedLeasePid > 0 && IsManagedAppProcessAlive(verifiedLeasePid))
            {
                mainPid = verifiedLeasePid;
                lifecycleState = "starting";
            }

            bool isRunning = mainPid > 0;
            bool staleLock = !isRunning && (File.Exists(lockfilePath) || File.Exists(Path.Combine(profilePath, ".runtime-lease.json")) || File.Exists(Path.Combine(profilePath, ".runtime-pid")));

            HashSet<int> tree = new HashSet<int>();
            long ws = 0;
            long priv = 0;
            long hwndVal = 0;
            string windowTitle = null;

            if (isRunning)
            {
                CollectDescendants(snap, mainPid, tree, true);
                SumProcessMemory(tree, out ws, out priv);
                IntPtr hWnd;
                string title;
                if (FindMainWindowForProfile(mainPid, tree, out hWnd, out title))
                {
                    hwndVal = hWnd.ToInt64();
                    windowTitle = title;
                }
            }

            string detectedUserId = DetectZaloUserId(zaloDataPath) ?? rec.zalo_user_id;

            Dictionary<string, object> state = new Dictionary<string, object>();
            state["id"] = rec.id;
            state["name"] = rec.name;
            state["directory_name"] = rec.directory_name;
            state["created_at"] = rec.created_at;
            state["last_launched_at"] = rec.last_launched_at;
            state["zalo_user_id"] = detectedUserId;
            state["notes"] = rec.notes ?? string.Empty;
            state["profile_path"] = profilePath;
            state["zalo_data_path"] = zaloDataPath;
            state["lockfile_path"] = lockfilePath;
            state["lifecycle_state"] = lifecycleState;
            state["running"] = isRunning;
            state["main_pid"] = isRunning ? (object)mainPid : null;
            state["process_ids"] = new List<int>(tree);
            state["process_count"] = tree.Count;
            state["window_handle"] = hwndVal;
            state["window_title"] = windowTitle;
            state["total_working_set_bytes"] = ws;
            state["total_private_bytes"] = priv;
            state["stale_lockfile_detected"] = staleLock;
            state["hint_pid"] = hintPid > 0 ? (object)hintPid : null;
            state["lockfile_pid"] = lockJsonPid > 0 ? (object)lockJsonPid : null;
            state["mode"] = (hint != null && hint.ContainsKey("mode")) ? hint["mode"] : null;
            state["app_path"] = (hint != null && hint.ContainsKey("app_path")) ? hint["app_path"] : null;

            if (includeSize && Directory.Exists(profilePath))
            {
                long bytes;
                int files;
                MeasureDirectorySafe(profilePath, out bytes, out files);
                state["data_size_bytes"] = bytes;
                state["data_file_count"] = files;
            }
            return state;
        }

        private static bool IsReparsePoint(string path)
        {
            try
            {
                FileAttributes attrs = File.GetAttributes(path);
                return (attrs & FileAttributes.ReparsePoint) == FileAttributes.ReparsePoint;
            }
            catch
            {
                return false;
            }
        }

        private static void MeasureDirectorySafe(string root, out long totalBytes, out int totalFiles)
        {
            totalBytes = 0;
            totalFiles = 0;
            if (!Directory.Exists(root) || IsReparsePoint(root)) return;
            try
            {
                string[] files = Directory.GetFiles(root);
                for (int i = 0; i < files.Length; i++)
                {
                    try
                    {
                        totalBytes += new FileInfo(files[i]).Length;
                        totalFiles++;
                    }
                    catch
                    {
                    }
                }
                string[] dirs = Directory.GetDirectories(root);
                for (int i = 0; i < dirs.Length; i++)
                {
                    long subBytes;
                    int subFiles;
                    MeasureDirectorySafe(dirs[i], out subBytes, out subFiles);
                    totalBytes += subBytes;
                    totalFiles += subFiles;
                }
            }
            catch
            {
            }
        }

        private static void EnsureProfileDirectoriesAndSeeds(string profilePath, string profileId)
        {
            string roaming = Path.Combine(profilePath, "AppData", "Roaming");
            string zaloData = Path.Combine(roaming, "ZaloData");
            string media = Path.Combine(zaloData, "media");
            string localTemp = Path.Combine(profilePath, "AppData", "Local", "Temp");
            string documents = Path.Combine(profilePath, "Documents", "Zalo Received Files");
            string desktop = Path.Combine(profilePath, "Desktop");

            Directory.CreateDirectory(media);
            Directory.CreateDirectory(localTemp);
            Directory.CreateDirectory(documents);
            string[] userFolders = new string[] { "Desktop", "Downloads", "Pictures", "Videos", "Music" };
            for (int i = 0; i < userFolders.Length; i++)
            {
                string uf = Path.Combine(profilePath, userFolders[i]);
                if (!Directory.Exists(uf) && !File.Exists(uf) && !IsReparsePoint(uf))
                {
                    Directory.CreateDirectory(uf);
                }
            }

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

            string markerPath = Path.Combine(zaloData, ".zalo-profile-id");
            File.WriteAllText(markerPath, profileId, new UTF8Encoding(false));
        }

        private static void VerifyZeroReparsePointsRemaining(string rootPath)
        {
            if (string.IsNullOrEmpty(rootPath) || (!Directory.Exists(rootPath) && !File.Exists(rootPath))) return;
            if (IsReparsePoint(rootPath))
            {
                throw new InvalidOperationException("IO_ERROR: Aborted recursive delete because root path '" + rootPath + "' is still a reparse point.");
            }
            if (!Directory.Exists(rootPath)) return;

            Stack<string> stack = new Stack<string>();
            stack.Push(rootPath);
            while (stack.Count > 0)
            {
                string cur = stack.Pop();
                string[] files = Directory.GetFiles(cur);
                for (int i = 0; i < files.Length; i++)
                {
                    if (IsReparsePoint(files[i]))
                    {
                        throw new InvalidOperationException("IO_ERROR: Aborted recursive delete because remaining reparse point '" + files[i] + "' could not be unlinked.");
                    }
                }
                string[] dirs = Directory.GetDirectories(cur);
                for (int i = 0; i < dirs.Length; i++)
                {
                    if (IsReparsePoint(dirs[i]))
                    {
                        throw new InvalidOperationException("IO_ERROR: Aborted recursive delete because remaining directory junction '" + dirs[i] + "' could not be unlinked.");
                    }
                    stack.Push(dirs[i]);
                }
            }
        }

        private static void DeleteDirectoryContentsPreservingOpLock(string dirPath)
        {
            string[] files = Directory.GetFiles(dirPath);
            for (int i = 0; i < files.Length; i++)
            {
                string name = Path.GetFileName(files[i]);
                if (string.Equals(name, ".dim-profile.lock", StringComparison.OrdinalIgnoreCase)) continue;
                if (IsReparsePoint(files[i]))
                {
                    throw new InvalidOperationException("IO_ERROR: Unexpected reparse point encountered during delete: " + files[i]);
                }
                File.Delete(files[i]);
            }
            string[] dirs = Directory.GetDirectories(dirPath);
            for (int i = 0; i < dirs.Length; i++)
            {
                if (IsReparsePoint(dirs[i]))
                {
                    throw new InvalidOperationException("IO_ERROR: Unexpected directory junction encountered during delete: " + dirs[i]);
                }
                Directory.Delete(dirs[i], true);
            }
        }

        private static int UnlinkJunctionsRecursively(string dirPath)
        {
            if (!Directory.Exists(dirPath)) return 0;
            int unlinked = 0;

            for (int i = 0; i < JunctionFolderNames.Length; i++)
            {
                string candidate = Path.Combine(dirPath, JunctionFolderNames[i]);
                if (Directory.Exists(candidate) && IsReparsePoint(candidate))
                {
                    if (NativeWin32.RemoveDirectoryW(candidate))
                    {
                        unlinked++;
                    }
                    else
                    {
                        Directory.Delete(candidate, false);
                        unlinked++;
                    }
                }
            }

            string[] files;
            try
            {
                files = Directory.GetFiles(dirPath);
            }
            catch
            {
                files = new string[0];
            }
            for (int i = 0; i < files.Length; i++)
            {
                if (IsReparsePoint(files[i]))
                {
                    File.Delete(files[i]);
                    unlinked++;
                }
            }

            string[] subdirs;
            try
            {
                subdirs = Directory.GetDirectories(dirPath);
            }
            catch
            {
                return unlinked;
            }

            for (int i = 0; i < subdirs.Length; i++)
            {
                string sub = subdirs[i];
                if (IsReparsePoint(sub))
                {
                    if (NativeWin32.RemoveDirectoryW(sub))
                    {
                        unlinked++;
                    }
                    else
                    {
                        Directory.Delete(sub, false);
                        unlinked++;
                    }
                }
                else
                {
                    unlinked += UnlinkJunctionsRecursively(sub);
                }
            }
            return unlinked;
        }

        private static int StartIsolatedProcess(string exePath, string arguments, string workingDir, string profilePath, ProfileRecord rec, bool hideConsoleWindow)
        {
            string roaming = Path.Combine(profilePath, "AppData", "Roaming");
            string local = Path.Combine(profilePath, "AppData", "Local");
            string temp = Path.Combine(local, "Temp");

            SortedDictionary<string, string> env = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (DictionaryEntry entry in Environment.GetEnvironmentVariables())
            {
                string k = entry.Key as string;
                string v = entry.Value as string;
                if (!string.IsNullOrEmpty(k) && v != null) env[k] = v;
            }

            env["USERPROFILE"] = profilePath;
            env["APPDATA"] = roaming;
            env["LOCALAPPDATA"] = local;
            env["TEMP"] = temp;
            env["TMP"] = temp;
            env["XDG_CONFIG_HOME"] = roaming;
            env["XDG_DATA_HOME"] = local;
            env["HOME"] = profilePath;

            string root = Path.GetPathRoot(profilePath);
            if (!string.IsNullOrEmpty(root) && root.Length >= 2 && root[1] == ':')
            {
                env["HOMEDRIVE"] = root.Substring(0, 2);
                env["HOMEPATH"] = profilePath.Substring(2);
            }

            env["DIM_PROFILE_ID"] = rec.id;
            env["MULTIZALO_PROFILE_ID"] = rec.id;
            env["MULTIZALO_PROFILE_NAME"] = rec.name;

            StringBuilder envBuilder = new StringBuilder();
            foreach (KeyValuePair<string, string> kv in env)
            {
                envBuilder.Append(kv.Key).Append('=').Append(kv.Value).Append('\0');
            }
            envBuilder.Append('\0');

            byte[] envBytes = Encoding.Unicode.GetBytes(envBuilder.ToString());
            IntPtr envPtr = Marshal.AllocHGlobal(envBytes.Length);
            try
            {
                Marshal.Copy(envBytes, 0, envPtr, envBytes.Length);

                NativeWin32.STARTUPINFO si = new NativeWin32.STARTUPINFO();
                si.cb = Marshal.SizeOf(typeof(NativeWin32.STARTUPINFO));
                NativeWin32.PROCESS_INFORMATION pi;

                string cmdLine = QuoteArg(exePath) + (string.IsNullOrEmpty(arguments) ? "" : " " + arguments);
                uint flags = NativeWin32.CREATE_UNICODE_ENVIRONMENT | NativeWin32.CREATE_NEW_PROCESS_GROUP;
                if (hideConsoleWindow) flags |= NativeWin32.CREATE_NO_WINDOW;

                bool ok = NativeWin32.CreateProcessW(
                    exePath,
                    cmdLine,
                    IntPtr.Zero,
                    IntPtr.Zero,
                    false,
                    flags,
                    envPtr,
                    workingDir,
                    ref si,
                    out pi);

                if (!ok)
                {
                    int err = Marshal.GetLastWin32Error();
                    throw new Win32Exception(err, "CreateProcessW failed with Win32 error " + err);
                }

                int pid = (int)pi.dwProcessId;
                if (pi.hThread != IntPtr.Zero) NativeWin32.CloseHandle(pi.hThread);
                if (pi.hProcess != IntPtr.Zero) NativeWin32.CloseHandle(pi.hProcess);
                return pid;
            }
            finally
            {
                Marshal.FreeHGlobal(envPtr);
            }
        }

        private static string QuoteArg(string value)
        {
            if (string.IsNullOrEmpty(value)) return "\"\"";
            bool needsQuotes = false;
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                if (c == ' ' || c == '\t' || c == '"' || c == '\n' || c == '\r')
                {
                    needsQuotes = true;
                    break;
                }
            }
            if (!needsQuotes) return value;

            StringBuilder sb = new StringBuilder();
            sb.Append('"');
            int backslashes = 0;
            for (int i = 0; i < value.Length; i++)
            {
                char c = value[i];
                if (c == '\\')
                {
                    backslashes++;
                }
                else if (c == '"')
                {
                    sb.Append('\\', backslashes * 2 + 1);
                    sb.Append('"');
                    backslashes = 0;
                }
                else
                {
                    sb.Append('\\', backslashes);
                    sb.Append(c);
                    backslashes = 0;
                }
            }
            sb.Append('\\', backslashes * 2);
            sb.Append('"');
            return sb.ToString();
        }

        private static bool TryValidateProfileName(string rawName, out string validated, out string errorMsg)
        {
            validated = (rawName ?? string.Empty).Trim();
            errorMsg = null;
            int scalarLen = CountUnicodeScalars(validated);
            if (scalarLen == 0 || scalarLen > 64)
            {
                errorMsg = "Profile name must be 1..64 characters.";
                return false;
            }
            for (int i = 0; i < validated.Length; i++)
            {
                char ch = validated[i];
                if (ch == '\0' || char.IsControl(ch))
                {
                    errorMsg = "Profile name contains NUL or control characters.";
                    return false;
                }
            }
            if (validated.Contains("..") || validated.Contains("/") || validated.Contains("\\"))
            {
                errorMsg = "Profile name cannot contain path separators or '..'.";
                return false;
            }
            return true;
        }

        public Dictionary<string, object> Status(bool includeSize, string appPath, bool nodeFixture)
        {
            EnsureStorageInitialized();
            ProfileStoreFile store = LoadStore();
            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();

            List<Dictionary<string, object>> list = new List<Dictionary<string, object>>();
            for (int i = 0; i < store.profiles.Count; i++)
            {
                list.Add(InspectProfileRuntime(store.profiles[i], snap, includeSize));
            }

            Dictionary<string, object> res = new Dictionary<string, object>();
            res["ok"] = true;
            res["timestamp_utc"] = NowRfc3339();
            res["platform"] = "windows";
            res["data_dir"] = _dataDir;
            res["installation"] = DiscoverApp(appPath, nodeFixture);
            res["primary_zalo"] = InspectPrimaryZalo(snap);
            res["folder_redirection"] = CheckFolderRedirection();
            res["profiles"] = list;
            return res;
        }

        public Dictionary<string, object> CreateProfile(string rawName, string notes)
        {
            string name;
            string nameErr;
            if (!TryValidateProfileName(rawName, out name, out nameErr))
            {
                return Fail("INVALID_NAME", nameErr);
            }

            using (AcquireStoreLockScope())
            {
                ProfileStoreFile store = LoadStore();
                for (int i = 0; i < store.profiles.Count; i++)
                {
                    if (ProfileNamesEqualCaseInsensitive(store.profiles[i].name, name))
                    {
                        return Fail("DUPLICATE_NAME", "Profile '" + name + "' already exists.");
                    }
                }

                string id = Guid.NewGuid().ToString("N").Substring(0, 8);
                string slug = SlugifyName(name);
                string dirName = slug + "_" + id;
                string profilePath = ResolveSafeChild(_profilesRoot, dirName);

                ProfileRecord rec = new ProfileRecord();
                rec.id = id;
                rec.name = name;
                rec.directory_name = dirName;
                rec.created_at = NowRfc3339();
                rec.last_launched_at = null;
                rec.zalo_user_id = null;
                rec.notes = (notes ?? string.Empty).Trim();

                EnsureProfileDirectoriesAndSeeds(profilePath, id);
                store.profiles.Add(rec);
                SaveStore(store);

                LogEvent("info", "create", "Created profile id=" + id + " dir=" + dirName);
                return Ok("Created profile '" + name + "'.", rec);
            }
        }

        public Dictionary<string, object> RenameProfile(string idOrName, string newRawName)
        {
            string newName;
            string nameErr;
            if (!TryValidateProfileName(newRawName, out newName, out nameErr))
            {
                return Fail("INVALID_NAME", nameErr);
            }

            using (AcquireStoreLockScope())
            {
                ProfileStoreFile store = LoadStore();
                ProfileRecord target = FindProfile(store, idOrName);
                if (target == null) return Fail("NOT_FOUND", "Profile not found.");

                for (int i = 0; i < store.profiles.Count; i++)
                {
                    if (!string.Equals(store.profiles[i].id, target.id, StringComparison.OrdinalIgnoreCase) &&
                        ProfileNamesEqualCaseInsensitive(store.profiles[i].name, newName))
                    {
                        return Fail("DUPLICATE_NAME", "Another profile already has that name.");
                    }
                }

                target.name = newName;
                SaveStore(store);
                LogEvent("info", "rename", "Renamed profile id=" + target.id);
                return Ok("Renamed profile.", target);
            }
        }

        public Dictionary<string, object> DeleteProfile(string idOrName, bool confirmed)
        {
            if (!confirmed)
            {
                return Fail("CONFIRM_REQUIRED", "Pass --confirm to delete a profile and its isolated directory.");
            }

            using (AcquireStoreLockScope())
            {
                ProfileStoreFile store = LoadStore();
                ProfileRecord target = FindProfile(store, idOrName);
                if (target == null) return Fail("NOT_FOUND", "Profile not found.");

                ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
                Dictionary<string, object> state = InspectProfileRuntime(target, snap, false);
                if ((bool)state["running"])
                {
                    return Fail("PROFILE_RUNNING", "Stop the running instance before deleting profile '" + target.name + "'.");
                }

                string profilePath = GetProfilePath(target);
                string primaryZaloData = Path.GetFullPath(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ZaloData")).TrimEnd('\\');
                string trimmedProfile = Path.GetFullPath(profilePath).TrimEnd('\\');
                if (string.Equals(trimmedProfile, primaryZaloData, StringComparison.OrdinalIgnoreCase) ||
                    trimmedProfile.StartsWith(primaryZaloData + "\\", StringComparison.OrdinalIgnoreCase))
                {
                    return Fail("PROTECTED_PRIMARY_PATH", "Refused delete: target path overlaps primary ZaloData.");
                }

                int unlinked = 0;
                if (Directory.Exists(profilePath))
                {
                    if (IsReparsePoint(profilePath))
                    {
                        if (!NativeWin32.RemoveDirectoryW(profilePath))
                        {
                            Directory.Delete(profilePath, false);
                        }
                        unlinked = 1;
                    }
                    else
                    {
                        IDisposable opLock = null;
                        try
                        {
                            opLock = AcquireProfileLockScope(profilePath, 0);
                        }
                        catch (Exception ex)
                        {
                            return Fail("PROFILE_RUNNING", ex.Message);
                        }
                        try
                        {
                            ProcessTreeSnapshot recheckSnap = CaptureProcessTreeSnapshot();
                            Dictionary<string, object> recheckState = InspectProfileRuntime(target, recheckSnap, false);
                            if ((bool)recheckState["running"])
                            {
                                return Fail("PROFILE_RUNNING", "Stop the running instance before deleting profile '" + target.name + "'.");
                            }

                            string markerPath = Path.Combine(profilePath, "AppData", "Roaming", "ZaloData", ".zalo-profile-id");
                            if (File.Exists(markerPath))
                            {
                                string markerVal = File.ReadAllText(markerPath, Encoding.UTF8).Trim();
                                if (!string.IsNullOrEmpty(markerVal) && !string.Equals(markerVal, target.id, StringComparison.OrdinalIgnoreCase))
                                {
                                    return Fail("PROFILE_MARKER_MISMATCH", "Profile directory marker belongs to another profile (" + markerVal + ").");
                                }
                            }

                            unlinked = UnlinkJunctionsRecursively(profilePath);
                            VerifyZeroReparsePointsRemaining(profilePath);
                            DeleteDirectoryContentsPreservingOpLock(profilePath);
                        }
                        finally
                        {
                            if (opLock != null) opLock.Dispose();
                        }
                        string opLockFile = Path.Combine(profilePath, ".dim-profile.lock");
                        try { if (File.Exists(opLockFile)) File.Delete(opLockFile); } catch { }
                        Directory.Delete(profilePath, false);
                    }
                }

                store.profiles.Remove(target);
                SaveStore(store);
                LogEvent("info", "delete", "Deleted profile id=" + target.id + " unlinked_junctions=" + unlinked);

                Dictionary<string, object> data = new Dictionary<string, object>();
                data["deleted_profile_id"] = target.id;
                data["unlinked_junctions"] = unlinked;
                return Ok("Deleted profile '" + target.name + "'.", data);
            }
        }

        private static string DeriveDeterministicFixtureUserId(string profileId)
        {
            uint hash = 2166136261u;
            for (int i = 0; i < profileId.Length; i++)
            {
                hash ^= (byte)profileId[i];
                hash *= 16777619u;
            }
            uint num = 1000000u + (hash % 8999999u);
            return num.ToString(CultureInfo.InvariantCulture);
        }

        public Dictionary<string, object> LaunchProfile(string idOrName, string appPath, bool nodeFixture, bool crashFlag, int crashDelayMs, string fixtureUserId)
        {
            ProfileStoreFile store = LoadStore();
            ProfileRecord target = FindProfile(store, idOrName);
            if (target == null) return Fail("NOT_FOUND", "Profile not found.");

            Dictionary<string, object> install = DiscoverApp(appPath, nodeFixture);
            string discoveredMode = install.ContainsKey("mode") && install["mode"] != null ? install["mode"].ToString() : string.Empty;
            string authStatus = install.ContainsKey("authenticode_status") && install["authenticode_status"] != null
                ? install["authenticode_status"].ToString()
                : string.Empty;
            string discoveredExe = install.ContainsKey("executable") && install["executable"] != null ? install["executable"].ToString() : null;

            if (!string.IsNullOrEmpty(discoveredExe) && File.Exists(discoveredExe))
            {
                if (discoveredMode == "official-zalo" && !string.Equals(authStatus, "Signed (VNG)", StringComparison.Ordinal))
                {
                    return Fail("UNVERIFIED_BINARY", "Official Zalo Authenticode validation failed: " + authStatus);
                }
                if (discoveredMode != "official-zalo" && !authStatus.StartsWith("Signed", StringComparison.Ordinal))
                {
                    return Fail("UNVERIFIED_BINARY", "Executable Authenticode validation failed: " + authStatus);
                }
            }

            if (!install.ContainsKey("valid") || !(bool)install["valid"])
            {
                string err = install.ContainsKey("error") && install["error"] != null ? install["error"].ToString() : "App executable not found.";
                return Fail("INSTALL_NOT_FOUND", err);
            }

            if (discoveredMode == "official-zalo")
            {
                Dictionary<string, object> folderRedir = CheckFolderRedirection();
                if ((bool)folderRedir["redirected"])
                {
                    return Fail("FOLDER_REDIRECTION_DETECTED", folderRedir["warning"] as string ?? "AppData Folder Redirection detected.");
                }
            }

            string profilePath = GetProfilePath(target);
            using (AcquireProfileLockScope(profilePath, 10000))
            {
            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
            Dictionary<string, object> runtime = InspectProfileRuntime(target, snap, false);
            if ((bool)runtime["running"])
            {
                return FocusProfileInternal(target, runtime, install);
            }

            // External DB / resource collision guard (IR-WIN-04 / F-CORE-01) — runs before UID check
            // because sharing an external DB directory also exposes the owner's `_production/<uid>`.
            List<string> targetExtPaths = GetExternalPathsOutsideProfile(profilePath);
            string resolvedTargetProf = ResolveEffectivePath(profilePath, profilePath);
            for (int i = 0; i < store.profiles.Count; i++)
            {
                ProfileRecord other = store.profiles[i];
                if (string.Equals(other.id, target.id, StringComparison.OrdinalIgnoreCase)) continue;
                Dictionary<string, object> otherRuntime = InspectProfileRuntime(other, snap, false);
                if (!(bool)otherRuntime["running"]) continue;

                string otherProfPath = GetProfilePath(other);
                string resolvedOtherProf = ResolveEffectivePath(otherProfPath, otherProfPath);
                List<string> otherExtPaths = GetExternalPathsOutsideProfile(otherProfPath);

                for (int tIdx = 0; tIdx < targetExtPaths.Count; tIdx++)
                {
                    string tExt = targetExtPaths[tIdx];
                    if (PathsOverlapOrEqual(tExt, resolvedOtherProf))
                    {
                        return Fail("SHARED_EXTERNAL_DB_PATH",
                            "External Zalo storage path '" + tExt + "' overlaps running profile '" + other.name + "'.");
                    }
                    for (int oIdx = 0; oIdx < otherExtPaths.Count; oIdx++)
                    {
                        if (PathsOverlapOrEqual(tExt, otherExtPaths[oIdx]))
                        {
                            return Fail("SHARED_EXTERNAL_DB_PATH",
                                "External Zalo storage path '" + tExt + "' is already used by running profile '" + other.name + "'.");
                        }
                    }
                }
                for (int oIdx = 0; oIdx < otherExtPaths.Count; oIdx++)
                {
                    if (PathsOverlapOrEqual(otherExtPaths[oIdx], resolvedTargetProf))
                    {
                        return Fail("SHARED_EXTERNAL_DB_PATH",
                            "Running profile '" + other.name + "' uses external storage overlapping '" + target.name + "'.");
                    }
                }
            }

            if (targetExtPaths.Count > 0)
            {
                string envPrimZalo = Environment.GetEnvironmentVariable("DIM_PROTECTED_PRIMARY_ZALO_DATA");
                string primZaloDir = !string.IsNullOrWhiteSpace(envPrimZalo)
                    ? envPrimZalo.Trim()
                    : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ZaloData");
                if (!string.IsNullOrEmpty(primZaloDir))
                {
                    List<string> primProtected = new List<string>();
                    primProtected.Add(ResolveEffectivePath(primZaloDir, primZaloDir));
                    List<string> primRawExt = ReadConfiguredStorageLocationsRaw(primZaloDir);
                    for (int pIdx = 0; pIdx < primRawExt.Count; pIdx++)
                    {
                        primProtected.Add(ResolveEffectivePath(primZaloDir, primRawExt[pIdx]));
                    }
                    for (int tIdx = 0; tIdx < targetExtPaths.Count; tIdx++)
                    {
                        for (int pIdx = 0; pIdx < primProtected.Count; pIdx++)
                        {
                            if (PathsOverlapOrEqual(targetExtPaths[tIdx], primProtected[pIdx]))
                            {
                                return Fail("SHARED_EXTERNAL_DB_PATH",
                                    "External Zalo storage path '" + targetExtPaths[tIdx] + "' is already used by Primary Zalo Desktop.");
                            }
                        }
                    }
                }
            }

            // Duplicate zalo_user_id guard
            string zaloDataPath = Path.Combine(profilePath, "AppData", "Roaming", "ZaloData");
            string explicitUid = !string.IsNullOrWhiteSpace(fixtureUserId) ? fixtureUserId.Trim() : null;
            string targetUserId = explicitUid ?? DetectZaloUserId(zaloDataPath) ?? target.zalo_user_id;
            if (!string.IsNullOrEmpty(targetUserId))
            {
                for (int i = 0; i < store.profiles.Count; i++)
                {
                    ProfileRecord other = store.profiles[i];
                    if (string.Equals(other.id, target.id, StringComparison.OrdinalIgnoreCase)) continue;
                    Dictionary<string, object> otherRuntime = InspectProfileRuntime(other, snap, false);
                    if ((bool)otherRuntime["running"])
                    {
                        string otherUid = otherRuntime["zalo_user_id"] as string;
                        if (string.Equals(otherUid, targetUserId, StringComparison.Ordinal))
                        {
                            LogEvent("warn", "launch", "Blocked duplicate zalo_user_id=" + targetUserId + " across profiles");
                            return Fail("DUPLICATE_ZALO_USER_ID",
                                "Account ID " + targetUserId + " is already logged in on running profile '" + other.name + "'.");
                        }
                    }
                }

                Dictionary<string, object> primary = InspectPrimaryZalo(snap);
                if ((bool)primary["is_running"] && string.Equals(primary["zalo_user_id"] as string, targetUserId, StringComparison.Ordinal))
                {
                    return Fail("DUPLICATE_ZALO_USER_ID",
                        "Account ID " + targetUserId + " is already logged in on primary Zalo Desktop.");
                }
            }

            // Verify .zalo-profile-id ownership if present
            string markerPath = Path.Combine(zaloDataPath, ".zalo-profile-id");
            if (File.Exists(markerPath))
            {
                string markerVal = File.ReadAllText(markerPath, Encoding.UTF8).Trim();
                if (!string.IsNullOrEmpty(markerVal) && !string.Equals(markerVal, target.id, StringComparison.OrdinalIgnoreCase))
                {
                    return Fail("PROFILE_MARKER_MISMATCH", "Profile directory marker belongs to another profile (" + markerVal + ").");
                }
            }

            // Stale lockfile crash recovery
            bool crashRecovered = false;
            string lockfilePath = Path.Combine(zaloDataPath, "lockfile");
            if ((bool)runtime["stale_lockfile_detected"] && File.Exists(lockfilePath))
            {
                try
                {
                    File.Delete(lockfilePath);
                    crashRecovered = true;
                }
                catch
                {
                }
                RemoveRuntimeHint(profilePath);
                try
                {
                    string qSig = Path.Combine(zaloDataPath, ".quit-signal");
                    if (File.Exists(qSig)) File.Delete(qSig);
                }
                catch
                {
                }
                LogEvent("info", "crash-recovery", "Cleaned stale lockfile for profile id=" + target.id);
            }

            EnsureProfileDirectoriesAndSeeds(profilePath, target.id);

            string mode = install["mode"].ToString();
            string exeToRun = install["executable"].ToString();
            string launchArgs = string.Empty;
            string workDir = Path.GetDirectoryName(exeToRun);
            bool hideConsole = false;
            string trackedAppPath = exeToRun;

            if (mode == "node-fixture")
            {
                string scriptPath = install["script_path"].ToString();
                trackedAppPath = scriptPath;
                hideConsole = true;
                string envUid = Environment.GetEnvironmentVariable("FIXTURE_ZALO_USER_ID");
                string resolvedFixtureUid = targetUserId ?? (!string.IsNullOrWhiteSpace(envUid) ? envUid.Trim() : DeriveDeterministicFixtureUserId(target.id));
                targetUserId = resolvedFixtureUid;

                StringBuilder argSb = new StringBuilder();
                argSb.Append(QuoteArg(scriptPath));
                argSb.Append(" --user-id ").Append(QuoteArg(resolvedFixtureUid));
                if (crashFlag)
                {
                    argSb.Append(" --crash");
                    if (crashDelayMs > 0) argSb.Append(" --crash-delay-ms ").Append(crashDelayMs.ToString(CultureInfo.InvariantCulture));
                }
                launchArgs = argSb.ToString();
                workDir = Path.GetDirectoryName(scriptPath);
            }

            string quitSigBeforeLaunch = Path.Combine(zaloDataPath, ".quit-signal");
            try { if (File.Exists(quitSigBeforeLaunch)) File.Delete(quitSigBeforeLaunch); } catch { }

            int pid = StartIsolatedProcess(exeToRun, launchArgs, workDir, profilePath, target, hideConsole);
            WriteRuntimeHint(profilePath, target.id, pid, mode, trackedAppPath, exeToRun);

            int pollBudgetMs = (mode == "official-zalo") ? 1500 : 2500;
            Stopwatch waitSw = Stopwatch.StartNew();
            while (waitSw.ElapsedMilliseconds < pollBudgetMs)
            {
                if (File.Exists(quitSigBeforeLaunch) || IsLockfileReady(lockfilePath)) break;
                if (!IsPidStillAlive(pid))
                {
                    if (IsLockfileReady(lockfilePath)) break;
                    RemoveRuntimeHint(profilePath);
                    return Fail("LAUNCH_FAILED", "Spawned process (PID " + pid + ") exited before acquiring profile lockfile.");
                }
                Thread.Sleep(20);
            }

            using (AcquireStoreLockScope())
            {
                ProfileStoreFile fresh = LoadStore();
                ProfileRecord rec = FindProfile(fresh, target.id);
                if (rec != null)
                {
                    rec.last_launched_at = NowRfc3339();
                    if (!string.IsNullOrEmpty(targetUserId)) rec.zalo_user_id = targetUserId;
                    SaveStore(fresh);
                }
            }

            LogEvent("info", "launch", "Launched profile id=" + target.id + " pid=" + pid + " mode=" + mode + " crash_recovered=" + crashRecovered);

            Dictionary<string, object> data = new Dictionary<string, object>();
            data["action"] = "launched";
            data["profile_id"] = target.id;
            data["pid"] = pid;
            data["mode"] = mode;
            data["crash_recovered"] = crashRecovered;
            data["profile_path"] = profilePath;
            return Ok("Launched profile '" + target.name + "'.", data);
            }
        }

        public Dictionary<string, object> FocusProfile(string idOrName, string appPath, bool nodeFixture)
        {
            ProfileStoreFile store = LoadStore();
            ProfileRecord target = FindProfile(store, idOrName);
            if (target == null) return Fail("NOT_FOUND", "Profile not found.");

            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
            Dictionary<string, object> runtime = InspectProfileRuntime(target, snap, false);
            if (!(bool)runtime["running"])
            {
                return Fail("NOT_RUNNING", "Profile '" + target.name + "' is not currently running.");
            }
            Dictionary<string, object> install = DiscoverApp(appPath, nodeFixture);
            return FocusProfileInternal(target, runtime, install);
        }

        private Dictionary<string, object> FocusProfileInternal(ProfileRecord target, Dictionary<string, object> runtime, Dictionary<string, object> install)
        {
            long hwndVal = Convert.ToInt64(runtime["window_handle"], CultureInfo.InvariantCulture);
            if (hwndVal != 0)
            {
                IntPtr hWnd = new IntPtr(hwndVal);
                NativeWin32.ShowWindow(hWnd, NativeWin32.IsIconic(hWnd) ? NativeWin32.SW_RESTORE : NativeWin32.SW_SHOW);
                NativeWin32.SetForegroundWindow(hWnd);
                if (NativeWin32.GetForegroundWindow() != hWnd)
                {
                    Thread.Sleep(35);
                }
                if (NativeWin32.GetForegroundWindow() == hWnd)
                {
                    Dictionary<string, object> data = new Dictionary<string, object>();
                    data["action"] = "focused";
                    data["profile_id"] = target.id;
                    data["pid"] = runtime["main_pid"];
                    data["method"] = "SetForegroundWindow";
                    return Ok("Focused profile '" + target.name + "'.", data);
                }
                // Foreground Lock blocked direct activation; fall through to isolated second-instance handoff!
            }

            string profilePath = GetProfilePath(target);
            string mode = (runtime["mode"] as string) ?? (install.ContainsKey("mode") ? install["mode"].ToString() : "official-zalo");
            if (mode == "node-fixture")
            {
                string focusSig = Path.Combine(profilePath, "AppData", "Roaming", "ZaloData", ".focus-signal");
                File.WriteAllText(focusSig, NowRfc3339(), new UTF8Encoding(false));
                Dictionary<string, object> data = new Dictionary<string, object>();
                data["action"] = "focused";
                data["profile_id"] = target.id;
                data["pid"] = runtime["main_pid"];
                data["method"] = "focus-signal";
                return Ok("Focused fixture profile '" + target.name + "'.", data);
            }

            if (install != null && install.ContainsKey("valid") && (bool)install["valid"])
            {
                string exe = install["executable"].ToString();
                StartIsolatedProcess(exe, string.Empty, Path.GetDirectoryName(exe), profilePath, target, false);
                Dictionary<string, object> data = new Dictionary<string, object>();
                data["action"] = "focused";
                data["profile_id"] = target.id;
                data["pid"] = runtime["main_pid"];
                data["method"] = "second-instance-signal";
                return Ok("Sent focus signal to profile '" + target.name + "'.", data);
            }

            return Fail("FOCUS_UNAVAILABLE", "No window handle available for profile '" + target.name + "'.");
        }

        public Dictionary<string, object> StopProfile(string idOrName, string appPath, bool nodeFixture)
        {
            ProfileStoreFile store = LoadStore();
            ProfileRecord target = FindProfile(store, idOrName);
            if (target == null) return Fail("NOT_FOUND", "Profile not found.");

            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
            Dictionary<string, object> primary = InspectPrimaryZalo(snap);
            HashSet<int> protectedPids = new HashSet<int>();
            List<int> primaryAll = (List<int>)primary["all_pids"];
            for (int i = 0; i < primaryAll.Count; i++) protectedPids.Add(primaryAll[i]);
            List<int> primaryLocking = (List<int>)primary["locking_pids"];
            for (int i = 0; i < primaryLocking.Count; i++) protectedPids.Add(primaryLocking[i]);

            Dictionary<string, object> runtime = InspectProfileRuntime(target, snap, false);

            // Check every possible PID reference against primary Zalo PIDs BEFORE stopping
            int? hintPidObj = runtime["hint_pid"] as int?;
            int? lockPidObj = runtime["lockfile_pid"] as int?;
            int? mainPidObj = runtime["main_pid"] as int?;
            if ((hintPidObj.HasValue && protectedPids.Contains(hintPidObj.Value)) ||
                (lockPidObj.HasValue && protectedPids.Contains(lockPidObj.Value)) ||
                (mainPidObj.HasValue && protectedPids.Contains(mainPidObj.Value)))
            {
                LogEvent("warn", "stop", "Blocked stop attempt matching primary Zalo PID");
                return Fail("PROTECTED_PRIMARY_PROCESS", "Refused stop: target PID matches protected primary Zalo Desktop process.");
            }
            List<int> profileTree = (List<int>)runtime["process_ids"];
            for (int i = 0; i < profileTree.Count; i++)
            {
                if (protectedPids.Contains(profileTree[i]))
                {
                    LogEvent("warn", "stop", "Blocked stop attempt matching primary Zalo descendant tree");
                    return Fail("PROTECTED_PRIMARY_PROCESS", "Refused stop: process tree intersects primary Zalo Desktop.");
                }
            }

            if (!(bool)runtime["running"])
            {
                RemoveRuntimeHint(GetProfilePath(target));
                Dictionary<string, object> idleData = new Dictionary<string, object>();
                idleData["profile_id"] = target.id;
                idleData["was_running"] = false;
                return Ok("Profile '" + target.name + "' is already stopped.", idleData);
            }

            int mainPid = mainPidObj.Value;
            string profilePath = GetProfilePath(target);
            using (AcquireProfileLockScope(profilePath, 10000))
            {
            string zaloDataDir = Path.Combine(profilePath, "AppData", "Roaming", "ZaloData");
            string lockfilePath = Path.Combine(zaloDataDir, "lockfile");
            string quitSigPath = Path.Combine(zaloDataDir, ".quit-signal");
            if (Directory.Exists(zaloDataDir))
            {
                try { File.WriteAllText(quitSigPath, "{\"requested_at\":\"" + NowRfc3339() + "\"}\n", new UTF8Encoding(false)); } catch { }
            }

            string lifecycle = (runtime.ContainsKey("lifecycle_state") && runtime["lifecycle_state"] != null)
                ? runtime["lifecycle_state"].ToString()
                : "running";
            bool isStartingNoWindow = string.Equals(lifecycle, "starting", StringComparison.OrdinalIgnoreCase) &&
                Convert.ToInt64(runtime["window_handle"], CultureInfo.InvariantCulture) == 0L;

            string mode = (runtime["mode"] as string) ?? (nodeFixture ? "node-fixture" : "official-zalo");
            string storedAppPath = runtime["app_path"] as string;
            bool isNodeProc = false;
            try
            {
                Process mp = Process.GetProcessById(mainPid);
                if (mp != null && mp.ProcessName.Equals("node", StringComparison.OrdinalIgnoreCase)) isNodeProc = true;
            }
            catch
            {
            }
            if (isNodeProc || nodeFixture || (!string.IsNullOrEmpty(appPath) && appPath.EndsWith(".mjs", StringComparison.OrdinalIgnoreCase)))
            {
                mode = "node-fixture";
            }

            bool gracefulExited = false;
            int terminatedCount = 0;

            if (mode == "node-fixture")
            {
                string scriptPath = !string.IsNullOrEmpty(appPath) ? Path.GetFullPath(appPath)
                    : (!string.IsNullOrEmpty(storedAppPath) ? storedAppPath : Path.Combine(_repoRoot, "fixtures", "test-desktop-app", "fixture-app.mjs"));
                string nodeExe = FindNodeExe();
                if (!string.IsNullOrEmpty(nodeExe) && File.Exists(scriptPath))
                {
                    StartIsolatedProcess(nodeExe, QuoteArg(scriptPath) + " --z-update-force-quit", Path.GetDirectoryName(scriptPath), profilePath, target, true);
                }

                int waitBudgetMs = isStartingNoWindow ? 150 : 5000;
                Stopwatch sw = Stopwatch.StartNew();
                while (sw.ElapsedMilliseconds < waitBudgetMs)
                {
                    if (!IsPidStillAlive(mainPid))
                    {
                        gracefulExited = true;
                        break;
                    }
                    Thread.Sleep(50);
                }
            }
            else
            {
                Dictionary<string, object> install = DiscoverApp(appPath, false);
                if (install.ContainsKey("valid") && (bool)install["valid"])
                {
                    string exe = install["executable"].ToString();
                    try
                    {
                        StartIsolatedProcess(exe, "--z-update-force-quit", Path.GetDirectoryName(exe), profilePath, target, false);
                    }
                    catch
                    {
                    }
                }

                long hwndVal = Convert.ToInt64(runtime["window_handle"], CultureInfo.InvariantCulture);
                if (hwndVal != 0)
                {
                    NativeWin32.PostMessage(new IntPtr(hwndVal), NativeWin32.WM_CLOSE, IntPtr.Zero, IntPtr.Zero);
                }

                try
                {
                    Process mp = Process.GetProcessById(mainPid);
                    if (mp != null && !mp.HasExited)
                    {
                        mp.CloseMainWindow();
                        if (mp.WaitForExit(isStartingNoWindow ? 150 : 2500)) gracefulExited = true;
                    }
                    else
                    {
                        gracefulExited = true;
                    }
                }
                catch
                {
                    gracefulExited = true;
                }
            }

            if (!gracefulExited)
            {
                ProcessTreeSnapshot freshSnap = CaptureProcessTreeSnapshot();
                List<int> rmLocking = GetLockingPids(lockfilePath);
                int verifiedLease = VerifyLiveLeasePid(profilePath, target.id);
                HashSet<int> verifiedOwned = new HashSet<int>();
                if (verifiedLease > 0) CollectDescendants(freshSnap, verifiedLease, verifiedOwned, true);
                for (int i = 0; i < rmLocking.Count; i++)
                {
                    if (rmLocking[i] > 0) CollectDescendants(freshSnap, rmLocking[i], verifiedOwned, true);
                }

                HashSet<int> toStop = new HashSet<int>();
                if (IsPidStillAlive(mainPid))
                {
                    CollectDescendants(freshSnap, mainPid, toStop, true);
                }
                foreach (int vpid in verifiedOwned)
                {
                    if (IsPidStillAlive(vpid)) toStop.Add(vpid);
                }
                for (int i = 0; i < profileTree.Count; i++)
                {
                    if (IsPidStillAlive(profileTree[i])) toStop.Add(profileTree[i]);
                }

                int selfPid = Process.GetCurrentProcess().Id;
                foreach (int pid in toStop)
                {
                    if (pid <= 0 || pid == selfPid || protectedPids.Contains(pid)) continue;
                    try
                    {
                        Process p = Process.GetProcessById(pid);
                        if (p != null && !p.HasExited)
                        {
                            string pn = p.ProcessName;
                            bool allowed = verifiedOwned.Contains(pid) ||
                                pn.StartsWith("Zalo", StringComparison.OrdinalIgnoreCase) ||
                                pn.StartsWith("zocr", StringComparison.OrdinalIgnoreCase) ||
                                pn.Equals("node", StringComparison.OrdinalIgnoreCase) ||
                                pn.StartsWith("dim-test-fixture", StringComparison.OrdinalIgnoreCase) ||
                                pn.StartsWith("xf-desktop-fixture", StringComparison.OrdinalIgnoreCase);
                            if (allowed)
                            {
                                p.Kill();
                                p.WaitForExit(2000);
                                terminatedCount++;
                            }
                        }
                    }
                    catch
                    {
                    }
                }
            }

            for (int attempt = 0; attempt < 20; attempt++)
            {
                if (!IsFileExclusivelyLocked(lockfilePath)) break;
                Thread.Sleep(100);
            }

            RemoveRuntimeHint(profilePath);
            try { if (File.Exists(quitSigPath)) File.Delete(quitSigPath); } catch { }
            LogEvent("info", "stop", "Stopped profile id=" + target.id + " graceful=" + gracefulExited + " killed=" + terminatedCount);

            Dictionary<string, object> data = new Dictionary<string, object>();
            data["profile_id"] = target.id;
            data["stopped_main_pid"] = mainPid;
            data["graceful_exit"] = gracefulExited;
            data["terminated_process_count"] = terminatedCount;
            return Ok("Stopped profile '" + target.name + "'.", data);
            }
        }

        public Dictionary<string, object> StopAllProfiles(string appPath, bool nodeFixture)
        {
            ProfileStoreFile store = LoadStore();
            List<Dictionary<string, object>> stoppedItems = new List<Dictionary<string, object>>();
            for (int i = 0; i < store.profiles.Count; i++)
            {
                ProfileRecord rec = store.profiles[i];
                Dictionary<string, object> res = StopProfile(rec.id, appPath, nodeFixture);
                if (res != null && res.ContainsKey("ok") && (bool)res["ok"] && res["data"] is Dictionary<string, object>)
                {
                    Dictionary<string, object> d = (Dictionary<string, object>)res["data"];
                    if (d.ContainsKey("stopped_main_pid"))
                    {
                        Dictionary<string, object> item = new Dictionary<string, object>();
                        item["profile_id"] = rec.id;
                        item["terminated_process_count"] = d.ContainsKey("terminated_process_count") ? d["terminated_process_count"] : 1;
                        stoppedItems.Add(item);
                    }
                }
            }
            Dictionary<string, object> outData = new Dictionary<string, object>();
            outData["stopped_profiles_count"] = stoppedItems.Count;
            outData["stopped_profiles"] = stoppedItems;
            return Ok("Stopped " + stoppedItems.Count + " secondary profile(s).", outData);
        }

        private static bool IsPidStillAlive(int pid)
        {
            if (pid <= 0) return false;
            try
            {
                Process p = Process.GetProcessById(pid);
                return p != null && !p.HasExited;
            }
            catch
            {
                return false;
            }
        }

        public Dictionary<string, object> BackupProfile(string idOrName, string customDest)
        {
            return BackupProfile(idOrName, customDest, false);
        }

        public Dictionary<string, object> BackupProfile(string idOrName, string customDest, bool allowExternalDbOmit)
        {
            ProfileStoreFile store = LoadStore();
            ProfileRecord target = FindProfile(store, idOrName);
            if (target == null) return Fail("NOT_FOUND", "Profile not found.");

            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
            Dictionary<string, object> runtime = InspectProfileRuntime(target, snap, false);
            if ((bool)runtime["running"])
            {
                return Fail("PROFILE_RUNNING", "Stop profile '" + target.name + "' before creating an offline backup.");
            }

            string profilePathForExt = GetProfilePath(target);
            List<string> externalPaths = GetExternalPathsOutsideProfile(profilePathForExt);
            if (externalPaths.Count > 0 && !allowExternalDbOmit)
            {
                return Fail("EXTERNAL_DB_OUTSIDE_PROFILE",
                    "Profile '" + target.name + "' stores Zalo database/resources outside its isolated directory (" +
                    string.Join(", ", externalPaths.ToArray()) +
                    "); pass --allow-external-db-omit to back up profile-local files only.");
            }

            string destRoot = _backupsRoot;
            if (!string.IsNullOrEmpty(customDest))
            {
                if (HasTraversalOrInvalidSyntax(customDest))
                {
                    return Fail("PATH_TRAVERSAL", "Backup destination contains invalid path traversal syntax.");
                }
                destRoot = Path.GetFullPath(customDest);
                string primaryZaloData = Path.GetFullPath(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ZaloData")).TrimEnd('\\');
                string trimmedDest = destRoot.TrimEnd('\\');
                if (string.Equals(trimmedDest, primaryZaloData, StringComparison.OrdinalIgnoreCase)
                    || trimmedDest.StartsWith(primaryZaloData + "\\", StringComparison.OrdinalIgnoreCase))
                {
                    return Fail("PROTECTED_PRIMARY_PATH", "Backup destination cannot reside inside primary ZaloData.");
                }
            }

            string profilePath = GetProfilePath(target);
            long payloadEstimate = 0;
            int estimatedFiles = 0;
            MeasureDirectorySafe(profilePath, out payloadEstimate, out estimatedFiles);

            Directory.CreateDirectory(destRoot);
            long freeSpace = new DriveInfo(Path.GetPathRoot(destRoot)).AvailableFreeSpace;
            long minThreshold = (5L * 1024L * 1024L * 1024L) + payloadEstimate;
            if (freeSpace < minThreshold)
            {
                return Fail("INSUFFICIENT_DISK_SPACE", "Available disk space is below the 5 GiB + payload safety threshold.");
            }
            string slug = SlugifyName(target.name);
            string stamp = DateTime.UtcNow.ToString("yyyyMMddTHHmmssfffZ", CultureInfo.InvariantCulture);
            string backupId = Guid.NewGuid().ToString("N").Substring(0, 8);
            string backupName = slug + "_" + stamp;
            if (Directory.Exists(Path.Combine(destRoot, backupName)))
            {
                backupName = backupName + "_" + backupId;
            }
            string partialDir = ResolveSafeChild(destRoot, backupName + ".partial");
            string finalDir = ResolveSafeChild(destRoot, backupName);
            Directory.CreateDirectory(partialDir);

            List<Dictionary<string, object>> manifestFiles = new List<Dictionary<string, object>>();
            long totalBytes = 0;

            try
            {
                CopyProfileTreeForBackup(profilePath, profilePath, partialDir, manifestFiles, ref totalBytes);

                Dictionary<string, object> manifest = new Dictionary<string, object>();
                manifest["schema_version"] = 1;
                manifest["backup_id"] = backupId;
                manifest["created_at"] = NowRfc3339();
                manifest["source_profile_id"] = target.id;
                manifest["source_profile_name"] = target.name;
                manifest["source_directory_name"] = target.directory_name;
                manifest["zalo_user_id"] = DetectZaloUserIdAllLayouts(profilePath) ?? runtime["zalo_user_id"];
                manifest["file_count"] = manifestFiles.Count;
                manifest["total_bytes"] = totalBytes;
                manifest["files"] = manifestFiles;
                if (externalPaths.Count > 0)
                {
                    manifest["external_db_omitted"] = true;
                    manifest["omitted_external_paths"] = externalPaths;
                }

                File.WriteAllText(Path.Combine(partialDir, "backup.json"), PrettyPrintJson(_json.Serialize(manifest)), new UTF8Encoding(false));
                Directory.Move(partialDir, finalDir);
            }
            catch
            {
                try { if (Directory.Exists(partialDir)) Directory.Delete(partialDir, true); } catch { }
                throw;
            }

            LogEvent("info", "backup", "Created offline backup for profile id=" + target.id + " files=" + manifestFiles.Count);

            Dictionary<string, object> resultData = new Dictionary<string, object>();
            resultData["backup_id"] = backupId;
            resultData["backup_dir"] = finalDir;
            resultData["backup_name"] = backupName;
            resultData["external_db_omitted"] = externalPaths.Count > 0;
            resultData["manifest_path"] = Path.Combine(finalDir, "backup.json");
            resultData["file_count"] = manifestFiles.Count;
            resultData["total_bytes"] = totalBytes;
            return Ok("Created offline atomic backup.", resultData);
        }

        private static void CopyProfileTreeForBackup(string rootDir, string currentDir, string destPayloadDir, List<Dictionary<string, object>> manifestFiles, ref long totalBytes)
        {
            if (!Directory.Exists(currentDir) || IsReparsePoint(currentDir)) return;

            string[] files = Directory.GetFiles(currentDir);
            for (int i = 0; i < files.Length; i++)
            {
                string src = files[i];
                string fileName = Path.GetFileName(src);
                if (string.Equals(fileName, "lockfile", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(fileName, "SingletonLock", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(fileName, ".runtime-pid", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(fileName, ".runtime-lease.json", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(fileName, ".dim-profile.lock", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(fileName, ".quit-signal", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(fileName, ".focus-signal", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                string rel = src.Substring(rootDir.Length).TrimStart('\\', '/');
                string relNorm = rel.Replace('\\', '/');
                if (string.Equals(relNorm, "backup.json", StringComparison.OrdinalIgnoreCase)) continue;
                string dst = ResolveSafeChild(destPayloadDir, relNorm);
                Directory.CreateDirectory(Path.GetDirectoryName(dst));
                File.Copy(src, dst, true);

                long len = new FileInfo(dst).Length;
                totalBytes += len;

                Dictionary<string, object> entry = new Dictionary<string, object>();
                entry["relative_path"] = relNorm;
                entry["size_bytes"] = len;
                entry["sha256"] = ComputeSha256(dst);
                manifestFiles.Add(entry);
            }

            string[] subdirs = Directory.GetDirectories(currentDir);
            for (int i = 0; i < subdirs.Length; i++)
            {
                string sub = subdirs[i];
                if (IsReparsePoint(sub)) continue;
                string relSub = sub.Substring(rootDir.Length).TrimStart('\\', '/').Replace('\\', '/');
                if (relSub.Equals("AppData/Local/Temp", StringComparison.OrdinalIgnoreCase)) continue;
                CopyProfileTreeForBackup(rootDir, sub, destPayloadDir, manifestFiles, ref totalBytes);
            }
        }

        public Dictionary<string, object> RestoreBackup(string backupDirInput, string requestedName)
        {
            if (string.IsNullOrEmpty(backupDirInput) || HasTraversalOrInvalidSyntax(backupDirInput))
            {
                return Fail("PATH_TRAVERSAL", "Backup path rejected by traversal guard.");
            }

            string backupDir = Path.IsPathRooted(backupDirInput)
                ? Path.GetFullPath(backupDirInput)
                : ResolveSafeChild(_backupsRoot, backupDirInput);

            string primaryZaloData = Path.GetFullPath(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "ZaloData")).TrimEnd('\\');
            string trimmedBackupDir = backupDir.TrimEnd('\\');
            if (string.Equals(trimmedBackupDir, primaryZaloData, StringComparison.OrdinalIgnoreCase)
                || trimmedBackupDir.StartsWith(primaryZaloData + "\\", StringComparison.OrdinalIgnoreCase))
            {
                return Fail("PROTECTED_PRIMARY_PATH", "Backup directory cannot reside inside primary ZaloData.");
            }

            string manifestPath = Path.Combine(backupDir, "backup.json");
            if (!Directory.Exists(backupDir) || IsReparsePoint(backupDir) || !File.Exists(manifestPath))
            {
                return Fail("BACKUP_ERROR", "Backup directory or backup.json manifest missing.");
            }

            Dictionary<string, object> manifest = _json.Deserialize<Dictionary<string, object>>(File.ReadAllText(manifestPath, Encoding.UTF8));
            int manifestSchema = manifest != null && manifest.ContainsKey("schema_version") && manifest["schema_version"] != null
                ? Convert.ToInt32(manifest["schema_version"], CultureInfo.InvariantCulture) : 0;
            if (manifestSchema != 1)
            {
                return Fail("BACKUP_ERROR", "Unsupported backup manifest schema_version.");
            }
            ArrayList filesList = manifest.ContainsKey("files") ? manifest["files"] as ArrayList : null;
            if (filesList == null)
            {
                return Fail("BACKUP_ERROR", "Manifest missing files array.");
            }

            long declaredCount = manifest.ContainsKey("file_count") && manifest["file_count"] != null
                ? Convert.ToInt64(manifest["file_count"], CultureInfo.InvariantCulture) : -1;
            long declaredTotal = manifest.ContainsKey("total_bytes") && manifest["total_bytes"] != null
                ? Convert.ToInt64(manifest["total_bytes"], CultureInfo.InvariantCulture) : -1;
            if (declaredCount != filesList.Count)
            {
                return Fail("BACKUP_ERROR", "Manifest file_count mismatch.");
            }

            long observedTotal = 0;
            // Verify SHA-256 and size of EVERY file before touching profile store
            for (int i = 0; i < filesList.Count; i++)
            {
                Dictionary<string, object> f = filesList[i] as Dictionary<string, object>;
                if (f == null || !f.ContainsKey("relative_path") || !f.ContainsKey("sha256") || !f.ContainsKey("size_bytes"))
                {
                    return Fail("BACKUP_ERROR", "Corrupt manifest file entry.");
                }
                string relNorm = f["relative_path"].ToString().Replace('\\', '/');
                if (string.Equals(relNorm, "backup.json", StringComparison.OrdinalIgnoreCase))
                {
                    return Fail("BACKUP_ERROR", "Manifest illegally lists backup.json.");
                }
                string srcFile = ResolveSafeChild(backupDir, relNorm);
                if (!File.Exists(srcFile) || IsReparsePoint(srcFile))
                {
                    return Fail("BACKUP_ERROR", "Missing or invalid backed-up file: " + relNorm);
                }
                long expectedSize = Convert.ToInt64(f["size_bytes"], CultureInfo.InvariantCulture);
                long actualSize = new FileInfo(srcFile).Length;
                if (actualSize != expectedSize)
                {
                    return Fail("BACKUP_ERROR", "Size mismatch for file: " + relNorm);
                }
                string actualSha = ComputeSha256(srcFile);
                string expectedSha = f["sha256"].ToString();
                if (!string.Equals(actualSha, expectedSha, StringComparison.OrdinalIgnoreCase))
                {
                    return Fail("BACKUP_ERROR", "SHA-256 mismatch for file: " + relNorm);
                }
                observedTotal += actualSize;
            }

            if (declaredTotal != observedTotal)
            {
                return Fail("BACKUP_ERROR", "Manifest total_bytes mismatch.");
            }

            long freeSpace = new DriveInfo(Path.GetPathRoot(_profilesRoot)).AvailableFreeSpace;
            if (freeSpace < (5L * 1024L * 1024L * 1024L) + observedTotal)
            {
                return Fail("INSUFFICIENT_DISK_SPACE", "Insufficient disk space for restore.");
            }

            string origName = manifest.ContainsKey("source_profile_name") && manifest["source_profile_name"] != null
                ? manifest["source_profile_name"].ToString()
                : "Restored Profile";
            string origUid = manifest.ContainsKey("zalo_user_id") && manifest["zalo_user_id"] != null
                ? manifest["zalo_user_id"].ToString()
                : null;

            string explicitRequestedName = null;
            if (!string.IsNullOrWhiteSpace(requestedName))
            {
                string validName;
                string nameErr;
                if (!TryValidateProfileName(requestedName, out validName, out nameErr))
                {
                    return Fail("INVALID_NAME", nameErr);
                }
                explicitRequestedName = validName;
            }

            using (AcquireStoreLockScope())
            {
                ProfileStoreFile store = LoadStore();
                string finalName = null;
                if (explicitRequestedName != null)
                {
                    if (FindProfileByExactName(store, explicitRequestedName) != null)
                    {
                        return Fail("DUPLICATE_NAME", "Profile '" + explicitRequestedName + "' already exists.");
                    }
                    finalName = explicitRequestedName;
                }
                else
                {
                    string baseCandidate = ClampUnicodeScalars(origName + " (Restored)", 64);
                    if (FindProfileByExactName(store, baseCandidate) == null)
                    {
                        finalName = baseCandidate;
                    }
                    else
                    {
                        for (int suffix = 2; suffix <= 999; suffix++)
                        {
                            string suffixText = " (Restored #" + suffix.ToString(CultureInfo.InvariantCulture) + ")";
                            int maxPrefix = Math.Max(0, 64 - suffixText.Length);
                            string prefix = ClampUnicodeScalars(origName, maxPrefix);
                            string candidate = prefix + suffixText;
                            if (FindProfileByExactName(store, candidate) == null)
                            {
                                finalName = candidate;
                                break;
                            }
                        }
                        if (finalName == null)
                        {
                            return Fail("RESTORE_ERROR", "Could not allocate unique restored profile name.");
                        }
                    }
                }

                string newId = Guid.NewGuid().ToString("N").Substring(0, 8);
                while (FindProfile(store, newId) != null)
                {
                    newId = Guid.NewGuid().ToString("N").Substring(0, 8);
                }
                string dirName = SlugifyName(finalName) + "_" + newId;
                string newProfilePath = ResolveSafeChild(_profilesRoot, dirName);
                string stamp = DateTime.UtcNow.ToString("yyyyMMddTHHmmssfffZ", CultureInfo.InvariantCulture);
                string stagingDir = ResolveSafeChild(_profilesRoot, ".restore-" + stamp + "-" + newId);

                try
                {
                    if (Directory.Exists(stagingDir))
                    {
                        UnlinkJunctionsRecursively(stagingDir);
                        VerifyZeroReparsePointsRemaining(stagingDir);
                        Directory.Delete(stagingDir, true);
                    }
                    Directory.CreateDirectory(stagingDir);
                    for (int i = 0; i < filesList.Count; i++)
                    {
                        Dictionary<string, object> f = (Dictionary<string, object>)filesList[i];
                        string relNorm = f["relative_path"].ToString().Replace('\\', '/');
                        string srcFile = ResolveSafeChild(backupDir, relNorm);
                        string dstFile = ResolveSafeChild(stagingDir, relNorm);
                        Directory.CreateDirectory(Path.GetDirectoryName(dstFile));
                        File.Copy(srcFile, dstFile, true);
                    }

                    EnsureProfileDirectoriesAndSeeds(stagingDir, newId);
                    Directory.Move(stagingDir, newProfilePath);
                }
                catch
                {
                    try
                    {
                        if (Directory.Exists(stagingDir))
                        {
                            UnlinkJunctionsRecursively(stagingDir);
                            VerifyZeroReparsePointsRemaining(stagingDir);
                            Directory.Delete(stagingDir, true);
                        }
                    }
                    catch
                    {
                    }
                    throw;
                }

                ProfileRecord rec = new ProfileRecord();
                rec.id = newId;
                rec.name = finalName;
                rec.directory_name = dirName;
                rec.created_at = NowRfc3339();
                rec.last_launched_at = null;
                rec.zalo_user_id = DetectZaloUserIdAllLayouts(newProfilePath) ?? origUid;
                rec.notes = "Restored from backup";

                store.profiles.Add(rec);
                SaveStore(store);

                LogEvent("info", "restore", "Restored backup into new profile id=" + newId + " files=" + filesList.Count);
                return Ok("Restored backup into new profile '" + finalName + "'.", rec);
            }
        }

        private static string DetectZaloUserIdAllLayouts(string profilePath)
        {
            if (string.IsNullOrEmpty(profilePath)) return null;
            string[] candidates = new string[]
            {
                Path.Combine(profilePath, "AppData", "Roaming", "ZaloData"),
                Path.Combine(profilePath, "Library", "Application Support", "ZaloData"),
                Path.Combine(profilePath, ".config", "ZaloData")
            };
            for (int i = 0; i < candidates.Length; i++)
            {
                string uid = DetectZaloUserId(candidates[i]);
                if (!string.IsNullOrEmpty(uid)) return uid;
            }
            return null;
        }

        private static ProfileRecord FindProfileByExactName(ProfileStoreFile store, string name)
        {
            for (int i = 0; i < store.profiles.Count; i++)
            {
                if (ProfileNamesEqualCaseInsensitive(store.profiles[i].name, name)) return store.profiles[i];
            }
            return null;
        }

        public Dictionary<string, object> ListBackups()
        {
            EnsureStorageInitialized();
            List<Dictionary<string, object>> items = new List<Dictionary<string, object>>();
            if (Directory.Exists(_backupsRoot))
            {
                string[] dirs = Directory.GetDirectories(_backupsRoot);
                for (int i = 0; i < dirs.Length; i++)
                {
                    string d = dirs[i];
                    string name = Path.GetFileName(d);
                    if (name.EndsWith(".partial", StringComparison.OrdinalIgnoreCase)) continue;
                    string manifestPath = Path.Combine(d, "backup.json");
                    if (!File.Exists(manifestPath)) continue;
                    try
                    {
                        Dictionary<string, object> m = _json.Deserialize<Dictionary<string, object>>(File.ReadAllText(manifestPath, Encoding.UTF8));
                        Dictionary<string, object> item = new Dictionary<string, object>();
                        item["backup_name"] = name;
                        item["backup_dir"] = d;
                        item["backup_id"] = m.ContainsKey("backup_id") ? m["backup_id"] : null;
                        item["created_at"] = m.ContainsKey("created_at") ? m["created_at"] : null;
                        item["file_count"] = m.ContainsKey("file_count") ? m["file_count"] : 0;
                        item["total_bytes"] = m.ContainsKey("total_bytes") ? m["total_bytes"] : 0;
                        item["source_profile_id"] = m.ContainsKey("source_profile_id") ? m["source_profile_id"] : null;
                        item["source_profile_name"] = m.ContainsKey("source_profile_name") ? m["source_profile_name"] : null;
                        items.Add(item);
                    }
                    catch
                    {
                    }
                }
            }

            Dictionary<string, object> res = new Dictionary<string, object>();
            res["ok"] = true;
            res["backups"] = items;
            return res;
        }

        public Dictionary<string, object> Diagnostics()
        {
            EnsureStorageInitialized();
            bool isElevated = false;
            try
            {
                WindowsPrincipal pr = new WindowsPrincipal(WindowsIdentity.GetCurrent());
                isElevated = pr.IsInRole(WindowsBuiltInRole.Administrator);
            }
            catch
            {
            }

            object sacState = null;
            try
            {
                using (RegistryKey key = Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Control\CI\Policy", false))
                {
                    if (key != null) sacState = key.GetValue("VerifiedAndReputablePolicyState");
                }
            }
            catch
            {
            }

            long diskFreeBytes = 0;
            try
            {
                string root = Path.GetPathRoot(_dataDir);
                diskFreeBytes = new DriveInfo(root).AvailableFreeSpace;
            }
            catch
            {
            }

            bool aclSafe = CheckProfilesAclSafe();
            List<string> recentLogs = new List<string>();
            if (File.Exists(_logFile))
            {
                try
                {
                    string[] lines = File.ReadAllLines(_logFile, Encoding.UTF8);
                    int start = Math.Max(0, lines.Length - 25);
                    for (int i = start; i < lines.Length; i++)
                    {
                        recentLogs.Add(SanitizeText(lines[i]));
                    }
                }
                catch
                {
                }
            }

            bool pipeRecvPresent = false;
            bool pipeSendPresent = false;
            try
            {
                string[] pipes = Directory.GetFiles(@"\\.\pipe\");
                for (int i = 0; i < pipes.Length; i++)
                {
                    string pn = Path.GetFileName(pipes[i]);
                    if (string.Equals(pn, "PipeZCallRecv", StringComparison.OrdinalIgnoreCase)) pipeRecvPresent = true;
                    else if (string.Equals(pn, "PipeZCallSend", StringComparison.OrdinalIgnoreCase)) pipeSendPresent = true;
                }
            }
            catch
            {
            }

            List<Dictionary<string, object>> matrix = new List<Dictionary<string, object>>();
            matrix.Add(MakePlatformRow("Windows 10/11", "PARTIALLY_SUPPORTED", "Per-process CreateProcessW + USERPROFILE/APPDATA/TEMP isolation + signed PowerShell host", "Core isolation/login/chat/backup SUPPORTED_AND_TESTED; enable Zalo in-app floating notifications (notiWin / znotification.html) for per-account click routing instead of shared AUMID com.vng.zalo Action Center toast; global PipeZCallRecv/PipeZCallSend call channels (passive check: recv=" + pipeRecvPresent.ToString().ToLowerInvariant() + ", send=" + pipeSendPresent.ToString().ToLowerInvariant() + ")."));
            matrix.Add(MakePlatformRow("macOS (Apple Silicon & Intel)", "NOT_TESTED", "open -n -a /Applications/Zalo.app --env CFFIXED_USER_HOME=<profile_home>", "Requires physical Mac verification; /tmp/socketzalorecv2021 call socket conflict."));
            matrix.Add(MakePlatformRow("Ubuntu LTS (Official Zalo)", "UNSUPPORTED", "No official VNG Linux desktop build exists", "Official VNG Linux package does not exist."));
            matrix.Add(MakePlatformRow("Ubuntu LTS (Fixture / Local AppImage)", "EXPERIMENTAL", "XDG_CONFIG_HOME / HOME / optional bwrap", "On Ubuntu 24.04+, configure an AppArmor userns profile in /etc/apparmor.d/ for local AppImages (never disable Chromium sandboxing); never bundle proprietary VNG app.asar."));

            Dictionary<string, object> res = new Dictionary<string, object>();
            res["ok"] = true;
            res["timestamp_utc"] = NowRfc3339();
            res["os_version"] = Environment.OSVersion.VersionString;
            res["is_elevated"] = isElevated;
            res["smart_app_control_state"] = sacState;
            res["folder_redirection"] = CheckFolderRedirection();
            res["disk_free_bytes"] = diskFreeBytes;
            res["disk_free_ok"] = diskFreeBytes >= (5L * 1024L * 1024L * 1024L);
            res["profiles_acl_safe"] = aclSafe;
            res["platform_support_matrix"] = matrix;
            res["sanitized_logs"] = recentLogs;
            return res;
        }

        private static Dictionary<string, object> MakePlatformRow(string platform, string status, string strategy, string limits)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["platform"] = platform;
            d["status"] = status;
            d["strategy"] = strategy;
            d["limits"] = limits;
            return d;
        }

        private bool CheckProfilesAclSafe()
        {
            try
            {
                DirectorySecurity ds = Directory.GetAccessControl(_profilesRoot);
                AuthorizationRuleCollection rules = ds.GetAccessRules(true, true, typeof(NTAccount));
                foreach (FileSystemAccessRule rule in rules)
                {
                    if (rule.AccessControlType != AccessControlType.Allow) continue;
                    string id = rule.IdentityReference.Value;
                    if (id.Equals("Everyone", StringComparison.OrdinalIgnoreCase) ||
                        id.Equals(@"BUILTIN\Users", StringComparison.OrdinalIgnoreCase))
                    {
                        FileSystemRights r = rule.FileSystemRights;
                        if ((r & (FileSystemRights.Write | FileSystemRights.Modify | FileSystemRights.FullControl | FileSystemRights.WriteData)) != 0)
                        {
                            return false;
                        }
                    }
                }
                return true;
            }
            catch
            {
                return false;
            }
        }

        public Dictionary<string, object> VerifyIsolation(string appPath, bool nodeFixture)
        {
            EnsureStorageInitialized();
            ProfileStoreFile store = LoadStore();
            ProcessTreeSnapshot snap = CaptureProcessTreeSnapshot();
            Dictionary<string, object> install = DiscoverApp(appPath, nodeFixture);
            Dictionary<string, object> primary = InspectPrimaryZalo(snap);
            Dictionary<string, object> folderRedir = CheckFolderRedirection();

            List<Dictionary<string, object>> checks = new List<Dictionary<string, object>>();

            bool installOk = install.ContainsKey("valid") && (bool)install["valid"];
            checks.Add(MakeCheck("ApplicationDiscovery", installOk,
                installOk ? ("Mode=" + install["mode"] + ", Auth=" + install["authenticode_status"]) : "Executable missing"));

            string primaryPath = Path.GetFullPath(primary["zalo_data_path"].ToString()).TrimEnd('\\');
            HashSet<int> primaryPids = new HashSet<int>((List<int>)primary["all_pids"]);
            HashSet<string> seenPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            HashSet<int> seenPids = new HashSet<int>();
            HashSet<string> seenRunningUids = new HashSet<string>(StringComparer.Ordinal);

            bool primaryOverlap = false;
            bool dupPaths = false;
            bool pidOverlap = false;
            bool uidOverlap = false;
            bool markerOk = true;

            for (int i = 0; i < store.profiles.Count; i++)
            {
                ProfileRecord rec = store.profiles[i];
                Dictionary<string, object> rt = InspectProfileRuntime(rec, snap, false);
                string zPath = Path.GetFullPath(rt["zalo_data_path"].ToString()).TrimEnd('\\');
                if (string.Equals(zPath, primaryPath, StringComparison.OrdinalIgnoreCase)) primaryOverlap = true;
                if (!seenPaths.Add(zPath)) dupPaths = true;

                string marker = Path.Combine(zPath, ".zalo-profile-id");
                if (File.Exists(marker))
                {
                    string mv = File.ReadAllText(marker, Encoding.UTF8).Trim();
                    if (!string.Equals(mv, rec.id, StringComparison.OrdinalIgnoreCase)) markerOk = false;
                }

                if ((bool)rt["running"] && rt["main_pid"] != null)
                {
                    int pid = Convert.ToInt32(rt["main_pid"], CultureInfo.InvariantCulture);
                    if (primaryPids.Contains(pid) || !seenPids.Add(pid)) pidOverlap = true;
                    string uid = rt["zalo_user_id"] as string;
                    if (!string.IsNullOrEmpty(uid) && !seenRunningUids.Add(uid)) uidOverlap = true;
                }
            }

            checks.Add(MakeCheck("PrimaryZaloDataSeparation", !primaryOverlap,
                !primaryOverlap ? "All profiles isolated outside primary APPDATA\\ZaloData" : "Profile overlaps primary ZaloData"));
            checks.Add(MakeCheck("DistinctProfilePathsAndMarkers", !dupPaths && markerOk,
                string.Format(CultureInfo.InvariantCulture, "Checked {0} profile(s), markers_valid={1}", store.profiles.Count, markerOk)));
            checks.Add(MakeCheck("DistinctProcessOwnership", !pidOverlap,
                string.Format(CultureInfo.InvariantCulture, "Active isolated PIDs={0}, Primary PIDs={1}", seenPids.Count, primaryPids.Count)));
            checks.Add(MakeCheck("DistinctZaloUserIds", !uidOverlap,
                !uidOverlap ? "No duplicate logged-in zalo_user_id among running instances" : "Duplicate zalo_user_id detected"));
            bool aclSafe = CheckProfilesAclSafe();
            checks.Add(MakeCheck("RestrictiveNtfsAcls", aclSafe,
                aclSafe ? "No Everyone or BUILTIN\\Users write access on Profiles directory" : "Profiles directory permits Everyone or BUILTIN\\Users write access"));
            bool isRedirected = (bool)folderRedir["redirected"];
            string redirStatus = isRedirected ? "BLOCKED" : "PASS";
            checks.Add(MakeCheckWithStatus("FolderRedirectionCheck", redirStatus,
                isRedirected ? "Blocked: AppData Folder Redirection detected" : "AppData resolves locally under USERPROFILE"));

            bool allPassed = true;
            for (int i = 0; i < checks.Count; i++)
            {
                if (!(bool)checks[i]["passed"])
                {
                    allPassed = false;
                    break;
                }
            }

            Dictionary<string, object> res = new Dictionary<string, object>();
            res["ok"] = allPassed;
            res["all_passed"] = allPassed;
            res["timestamp_utc"] = NowRfc3339();
            res["checks"] = checks;
            return res;
        }

        private static Dictionary<string, object> MakeCheck(string name, bool passed, string detail)
        {
            return MakeCheckWithStatus(name, passed ? "PASS" : "FAIL", detail);
        }

        private static Dictionary<string, object> MakeCheckWithStatus(string name, string status, string detail)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["check_name"] = name;
            d["status"] = status;
            d["passed"] = string.Equals(status, "PASS", StringComparison.Ordinal);
            d["detail"] = SanitizeText(detail);
            return d;
        }

        private static Dictionary<string, object> Ok(string message, object data)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["ok"] = true;
            d["error_code"] = null;
            d["message"] = SanitizeText(message);
            d["data"] = data;
            return d;
        }

        private static Dictionary<string, object> Fail(string code, string message)
        {
            Dictionary<string, object> d = new Dictionary<string, object>();
            d["ok"] = false;
            d["error_code"] = code;
            d["message"] = SanitizeText(message);
            d["data"] = null;
            return d;
        }

        public string SerializeResult(object obj)
        {
            return PrettyPrintJson(_json.Serialize(obj));
        }

        public int DispatchCli(string[] args)
        {
            string appPath = null;
            bool nodeFixture = false;
            bool withSize = false;
            bool confirm = false;
            bool allowExternalDbOmit = false;
            bool crash = false;
            int crashDelayMs = 0;
            string dest = null;
            string newNameOpt = null;
            string notesOpt = "";
            string fixtureUserIdOpt = null;

            bool afterDoubleDash = false;
            List<string> positional = new List<string>();
            for (int i = 0; i < args.Length; i++)
            {
                string a = args[i];
                if (afterDoubleDash)
                {
                    positional.Add(a);
                    continue;
                }
                if (a == "--")
                {
                    afterDoubleDash = true;
                    continue;
                }
                if (a == "--cli" || a == "--json") continue;
                if ((a == "--data-dir" || a == "--storage-root") && i + 1 < args.Length) { i++; continue; }
                if (a == "--app-path" && i + 1 < args.Length) { appPath = args[++i]; continue; }
                if (a == "--node-fixture") { nodeFixture = true; continue; }
                if (a == "--with-size") { withSize = true; continue; }
                if (a == "--confirm") { confirm = true; continue; }
                if (a == "--allow-external-db-omit" || a == "--allow-incomplete-external-db") { allowExternalDbOmit = true; continue; }
                if (a == "--crash") { crash = true; continue; }
                if (a == "--crash-delay-ms" && i + 1 < args.Length)
                {
                    int.TryParse(args[++i], out crashDelayMs);
                    continue;
                }
                if (a == "--dest" && i + 1 < args.Length) { dest = args[++i]; continue; }
                if (a == "--name" && i + 1 < args.Length) { newNameOpt = args[++i]; continue; }
                if (a == "--notes" && i + 1 < args.Length) { notesOpt = args[++i]; continue; }
                if (a == "--user-id" && i + 1 < args.Length) { fixtureUserIdOpt = args[++i]; continue; }
                positional.Add(a);
            }

            string cmd = positional.Count > 0 ? positional[0].ToLowerInvariant() : "status";
            Dictionary<string, object> output;
            try
            {
                if (cmd == "status") output = Status(withSize, appPath, nodeFixture);
                else if (cmd == "discover") output = DiscoverApp(appPath, nodeFixture);
                else if (cmd == "create")
                {
                    if (positional.Count < 2) output = Fail("INVALID_NAME", "Usage: create <name> [--notes <text>]");
                    else output = CreateProfile(positional[1], positional.Count >= 3 ? positional[2] : notesOpt);
                }
                else if (cmd == "rename")
                {
                    if (positional.Count < 2) output = Fail("NOT_FOUND", "Usage: rename <id> <newName>");
                    else if (positional.Count < 3) output = Fail("INVALID_NAME", "Usage: rename <id> <newName>");
                    else output = RenameProfile(positional[1], positional[2]);
                }
                else if (cmd == "delete")
                {
                    if (positional.Count < 2) output = Fail("NOT_FOUND", "Usage: delete <id> --confirm");
                    else output = DeleteProfile(positional[1], confirm);
                }
                else if (cmd == "launch")
                {
                    if (positional.Count < 2) output = Fail("NOT_FOUND", "Usage: launch <id> [--app-path <exeOrScript>] [--node-fixture]");
                    else output = LaunchProfile(positional[1], appPath, nodeFixture, crash, crashDelayMs, fixtureUserIdOpt);
                }
                else if (cmd == "focus")
                {
                    if (positional.Count < 2) output = Fail("NOT_FOUND", "Usage: focus <id>");
                    else output = FocusProfile(positional[1], appPath, nodeFixture);
                }
                else if (cmd == "stop")
                {
                    if (positional.Count < 2) output = Fail("NOT_FOUND", "Usage: stop <id>");
                    else output = StopProfile(positional[1], appPath, nodeFixture);
                }
                else if (cmd == "stop-all") output = StopAllProfiles(appPath, nodeFixture);
                else if (cmd == "backup")
                {
                    if (positional.Count < 2) output = Fail("NOT_FOUND", "Usage: backup <id> [--dest <dir>] [--allow-external-db-omit]");
                    else output = BackupProfile(positional[1], dest, allowExternalDbOmit);
                }
                else if (cmd == "restore")
                {
                    if (positional.Count < 2) output = Fail("BACKUP_ERROR", "Usage: restore <backupDir> [--name <newName>]");
                    else output = RestoreBackup(positional[1], newNameOpt);
                }
                else if (cmd == "backups") output = ListBackups();
                else if (cmd == "diagnostics") output = Diagnostics();
                else if (cmd == "verify-isolation") output = VerifyIsolation(appPath, nodeFixture);
                else output = Fail("UNKNOWN_COMMAND", "Unknown command: " + cmd);
            }
            catch (Exception ex)
            {
                string msg = ex.Message ?? "Unhandled error";
                string code = msg.StartsWith("PATH_TRAVERSAL", StringComparison.Ordinal) ? "PATH_TRAVERSAL"
                    : msg.StartsWith("PROTECTED_PRIMARY_PATH", StringComparison.Ordinal) ? "PROTECTED_PRIMARY_PATH"
                    : msg.StartsWith("JSON_ERROR", StringComparison.Ordinal) ? "JSON_ERROR"
                    : msg.StartsWith("SCHEMA_ERROR", StringComparison.Ordinal) ? "SCHEMA_ERROR"
                    : msg.StartsWith("IO_ERROR", StringComparison.Ordinal) ? "IO_ERROR"
                    : "UNHANDLED_EXCEPTION";
                output = Fail(code, msg);
            }

            Console.WriteLine(SerializeResult(output));
            bool okFlag = output.ContainsKey("ok") && output["ok"] is bool && (bool)output["ok"];
            return okFlag ? 0 : 1;
        }
    }
}
'@

if (-not ("DesktopInstanceManager.Host.HostEngine" -as [type])) {
    Add-Type -TypeDefinition $csharpSource -ReferencedAssemblies @(
        "System.dll",
        "System.Core.dll",
        "System.Web.Extensions.dll"
    )
}

$argList = @()
if ($CliArgs) { $argList = [string[]]$CliArgs }

$dataDirArg = $null
for ($i = 0; $i -lt $argList.Length; $i++) {
    if ($argList[$i] -eq "--") { break }
    if (($argList[$i] -eq "--data-dir" -or $argList[$i] -eq "--storage-root") -and ($i + 1 -lt $argList.Length)) {
        $dataDirArg = $argList[$i + 1]
        break
    }
}

try {
    $engine = New-Object DesktopInstanceManager.Host.HostEngine($repoRoot, $dataDirArg)
    $exitCode = $engine.DispatchCli($argList)
    exit $exitCode
}
catch {
    $errMsg = $_.Exception.Message
    if ($_.Exception.InnerException) {
        $errMsg = $_.Exception.InnerException.Message
    }
    $code = if ($errMsg -like "PATH_TRAVERSAL*") { "PATH_TRAVERSAL" } elseif ($errMsg -like "PROTECTED_PRIMARY_PATH*") { "PROTECTED_PRIMARY_PATH" } elseif ($errMsg -like "JSON_ERROR*") { "JSON_ERROR" } elseif ($errMsg -like "SCHEMA_ERROR*") { "SCHEMA_ERROR" } elseif ($errMsg -like "IO_ERROR*") { "IO_ERROR" } else { "INIT_FAILED" }
    $errObj = [ordered]@{
        ok = $false
        error_code = $code
        message = [DesktopInstanceManager.Host.HostEngine]::SanitizeText($errMsg)
        data = $null
    }
    Write-Output ($errObj | ConvertTo-Json -Compress)
    exit 1
}
