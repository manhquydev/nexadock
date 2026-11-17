using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows;
using MultiZaloDesktop.Core;
using MultiZaloDesktop.Platform;

namespace MultiZaloDesktop.UI
{
    public static class Program
    {
        private const int ATTACH_PARENT_PROCESS = -1;
        private const int SW_HIDE = 0;

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AttachConsole(int dwProcessId);

        [DllImport("kernel32.dll")]
        private static extern IntPtr GetConsoleWindow();

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        [STAThread]
        public static int Main(string[] args)
        {
            string storageRoot = null;
            string installRoot = null;
            bool cliMode = false;
            List<string> cliArgs = new List<string>();

            for (int i = 0; i < args.Length; i++)
            {
                string arg = args[i];
                if (string.Equals(arg, "--storage-root", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length)
                {
                    storageRoot = args[++i];
                }
                else if (string.Equals(arg, "--install-root", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length)
                {
                    installRoot = args[++i];
                }
                else if (string.Equals(arg, "--cli", StringComparison.OrdinalIgnoreCase))
                {
                    cliMode = true;
                }
                else
                {
                    cliArgs.Add(arg);
                }
            }

            IPlatformAdapter adapter = SelectPlatformAdapter();
            ProfileManager profileMgr = new ProfileManager(storageRoot);
            profileMgr.EnsureStorageInitialized();
            InstanceManager instanceMgr = new InstanceManager(profileMgr, adapter, installRoot);

            if (cliMode)
            {
                if (!Console.IsOutputRedirected)
                {
                    AttachConsole(ATTACH_PARENT_PROCESS);
                }
                return RunCli(instanceMgr, cliArgs);
            }

            IntPtr consoleWnd = GetConsoleWindow();
            if (consoleWnd != IntPtr.Zero)
            {
                ShowWindow(consoleWnd, SW_HIDE);
            }

            if (Application.Current == null)
            {
                Application app = new Application();
                MainWindow win = new MainWindow(instanceMgr);
                app.Run(win);
            }
            else
            {
                MainWindow win = new MainWindow(instanceMgr);
                win.ShowDialog();
            }
            return 0;
        }

        private static IPlatformAdapter SelectPlatformAdapter()
        {
            if (Environment.OSVersion.Platform == PlatformID.Win32NT)
            {
                return new WindowsPlatformAdapter();
            }
            if (Environment.OSVersion.Platform == PlatformID.MacOSX)
            {
                return new MacOSPlatformAdapter();
            }
            return new LinuxPlatformAdapter();
        }

        private static int RunCli(InstanceManager mgr, List<string> args)
        {
            string cmd = args.Count > 0 ? args[0].ToLowerInvariant() : "status";
            try
            {
                if (cmd == "status")
                {
                    bool includeSize = args.Contains("--with-size");
                    Console.WriteLine(JsonHelper.Serialize(mgr.CaptureStatus(includeSize), true));
                    return 0;
                }
                if (cmd == "list")
                {
                    Console.WriteLine(JsonHelper.Serialize(mgr.Profiles.ListProfiles(), true));
                    return 0;
                }
                if (cmd == "create")
                {
                    if (args.Count < 2)
                    {
                        return PrintFail("MISSING_ARG", "Usage: --cli create <name> [notes]");
                    }
                    string notes = args.Count >= 3 ? args[2] : string.Empty;
                    OperationResult res = mgr.CreateProfile(args[1], notes);
                    Console.WriteLine(JsonHelper.Serialize(res, true));
                    return res.Success ? 0 : 1;
                }
                if (cmd == "rename")
                {
                    if (args.Count < 3)
                    {
                        return PrintFail("MISSING_ARG", "Usage: --cli rename <idOrName> <newName>");
                    }
                    OperationResult res = mgr.RenameProfile(args[1], args[2]);
                    Console.WriteLine(JsonHelper.Serialize(res, true));
                    return res.Success ? 0 : 1;
                }
                if (cmd == "launch")
                {
                    if (args.Count < 2)
                    {
                        return PrintFail("MISSING_ARG", "Usage: --cli launch <idOrName>");
                    }
                    OperationResult res = mgr.LaunchOrFocusProfile(args[1]);
                    Console.WriteLine(JsonHelper.Serialize(res, true));
                    return res.Success ? 0 : 1;
                }
                if (cmd == "focus")
                {
                    if (args.Count < 2)
                    {
                        return PrintFail("MISSING_ARG", "Usage: --cli focus <idOrName>");
                    }
                    OperationResult res = mgr.FocusProfile(args[1]);
                    Console.WriteLine(JsonHelper.Serialize(res, true));
                    return res.Success ? 0 : 1;
                }
                if (cmd == "stop")
                {
                    if (args.Count < 2)
                    {
                        return PrintFail("MISSING_ARG", "Usage: --cli stop <idOrName>");
                    }
                    OperationResult res = mgr.StopProfile(args[1]);
                    Console.WriteLine(JsonHelper.Serialize(res, true));
                    return res.Success ? 0 : 1;
                }
                if (cmd == "delete")
                {
                    if (args.Count < 2)
                    {
                        return PrintFail("MISSING_ARG", "Usage: --cli delete <idOrName> --confirm");
                    }
                    if (!args.Contains("--confirm"))
                    {
                        return PrintFail("CONFIRM_REQUIRED", "Pass --confirm to delete a profile and its isolated directory.");
                    }
                    OperationResult res = mgr.DeleteProfile(args[1]);
                    Console.WriteLine(JsonHelper.Serialize(res, true));
                    return res.Success ? 0 : 1;
                }
                if (cmd == "verify-isolation")
                {
                    IsolationReport rep = DiagnosticsService.VerifyIsolation(mgr);
                    Console.WriteLine(JsonHelper.Serialize(rep, true));
                    return rep.AllPassed ? 0 : 1;
                }

                return PrintFail("UNKNOWN_COMMAND", "Unknown command: " + cmd);
            }
            catch (Exception ex)
            {
                return PrintFail("UNHANDLED_EXCEPTION", ex.Message);
            }
        }

        private static int PrintFail(string code, string message)
        {
            Console.WriteLine(JsonHelper.Serialize(OperationResult.Fail(code, message), true));
            return 1;
        }
    }
}
