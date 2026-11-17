using System;
using System.Collections.Generic;
using System.Text;
using System.Web.Script.Serialization;

namespace MultiZaloDesktop.Core
{
    public sealed class ProfileRecord
    {
        public string Id { get; set; }
        public string Name { get; set; }
        public string DirectoryName { get; set; }
        public string CreatedAt { get; set; }
        public string LastLaunchedAt { get; set; }
        public string Notes { get; set; }
    }

    public sealed class ProfileStore
    {
        public int StorageVersion { get; set; }
        public List<ProfileRecord> Profiles { get; set; }

        public ProfileStore()
        {
            StorageVersion = 1;
            Profiles = new List<ProfileRecord>();
        }
    }

    public sealed class ZaloInstallationInfo
    {
        public bool IsValid { get; set; }
        public string InstallRoot { get; set; }
        public string StubExePath { get; set; }
        public string VersionDirName { get; set; }
        public string Version { get; set; }
        public string ElectronExePath { get; set; }
        public string AppAsarPath { get; set; }
        public string AuthenticodeStatus { get; set; }
        public string SignerSubject { get; set; }
        public string Error { get; set; }
    }

    public sealed class ProfileRuntimeState
    {
        public string ProfileId { get; set; }
        public string ProfileName { get; set; }
        public string ProfilePath { get; set; }
        public string ZaloDataPath { get; set; }
        public string LockfilePath { get; set; }
        public bool IsRunning { get; set; }
        public int MainPid { get; set; }
        public int ProcessCount { get; set; }
        public long TotalWorkingSetBytes { get; set; }
        public long TotalPrivateBytes { get; set; }
        public long WindowHandleValue { get; set; }
        public string WindowTitle { get; set; }
        public long DataSizeBytes { get; set; }
        public string StatusLabel { get; set; }
        public string LastLaunchedAt { get; set; }
        public List<int> ProcessIds { get; set; }

        public ProfileRuntimeState()
        {
            ProcessIds = new List<int>();
        }
    }

    public sealed class PrimaryZaloStatus
    {
        public string ZaloDataPath { get; set; }
        public string LockfilePath { get; set; }
        public bool IsRunning { get; set; }
        public List<int> LockingPids { get; set; }
        public int ProcessCount { get; set; }
        public long TotalWorkingSetBytes { get; set; }
        public string LastConfigWriteUtc { get; set; }

        public PrimaryZaloStatus()
        {
            LockingPids = new List<int>();
        }
    }

    public sealed class StatusSnapshot
    {
        public string TimestampUtc { get; set; }
        public string Platform { get; set; }
        public string StorageRoot { get; set; }
        public ZaloInstallationInfo Installation { get; set; }
        public PrimaryZaloStatus PrimaryZalo { get; set; }
        public List<ProfileRuntimeState> Profiles { get; set; }

        public StatusSnapshot()
        {
            Profiles = new List<ProfileRuntimeState>();
        }
    }

    public sealed class OperationResult
    {
        public bool Success { get; set; }
        public string Message { get; set; }
        public string ErrorCode { get; set; }
        public object Data { get; set; }

        public static OperationResult Ok(string message, object data)
        {
            OperationResult r = new OperationResult();
            r.Success = true;
            r.Message = message;
            r.Data = data;
            return r;
        }

        public static OperationResult Fail(string errorCode, string message)
        {
            OperationResult r = new OperationResult();
            r.Success = false;
            r.ErrorCode = errorCode;
            r.Message = message;
            return r;
        }
    }

    public static class JsonHelper
    {
        private static JavaScriptSerializer CreateSerializer()
        {
            JavaScriptSerializer serializer = new JavaScriptSerializer();
            serializer.MaxJsonLength = 16777216;
            return serializer;
        }

        public static string Serialize(object obj, bool pretty)
        {
            string raw = CreateSerializer().Serialize(obj);
            if (!pretty)
            {
                return raw;
            }
            return FormatJson(raw);
        }

        public static T Deserialize<T>(string json)
        {
            return CreateSerializer().Deserialize<T>(json);
        }

        public static string FormatJson(string json)
        {
            if (string.IsNullOrEmpty(json))
            {
                return json;
            }

            StringBuilder sb = new StringBuilder();
            bool inQuotes = false;
            bool escaped = false;
            int indent = 0;

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
                    sb.Append(ch);
                    inQuotes = !inQuotes;
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
                        indent++;
                        sb.Append(Environment.NewLine);
                        sb.Append(' ', indent * 2);
                    }
                }
                else if (ch == '}' || ch == ']')
                {
                    if (i > 0 && json[i - 1] != '{' && json[i - 1] != '[')
                    {
                        indent = Math.Max(0, indent - 1);
                        sb.Append(Environment.NewLine);
                        sb.Append(' ', indent * 2);
                    }
                    sb.Append(ch);
                }
                else if (ch == ',')
                {
                    sb.Append(ch);
                    sb.Append(Environment.NewLine);
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

            return sb.ToString();
        }
    }
}
