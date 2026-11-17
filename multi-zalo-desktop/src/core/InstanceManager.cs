using System;
using System.Collections.Generic;
using MultiZaloDesktop.Platform;

namespace MultiZaloDesktop.Core
{
    public sealed class InstanceManager
    {
        private readonly ProfileManager _profileManager;
        private readonly IPlatformAdapter _platformAdapter;
        private readonly string _customInstallRoot;

        public ProfileManager Profiles { get { return _profileManager; } }
        public IPlatformAdapter Adapter { get { return _platformAdapter; } }

        public InstanceManager(ProfileManager profileManager, IPlatformAdapter platformAdapter, string customInstallRoot)
        {
            _profileManager = profileManager;
            _platformAdapter = platformAdapter;
            _customInstallRoot = customInstallRoot;
        }

        public StatusSnapshot CaptureStatus(bool includeDiskSizes)
        {
            StatusSnapshot snapshot = new StatusSnapshot();
            snapshot.TimestampUtc = DateTime.UtcNow.ToString("o");
            snapshot.Platform = _platformAdapter.PlatformName;
            snapshot.StorageRoot = _profileManager.StorageRoot;
            snapshot.Installation = _platformAdapter.DiscoverOfficialInstallation(_customInstallRoot);
            snapshot.PrimaryZalo = _platformAdapter.InspectPrimaryZaloStatus();

            List<ProfileRecord> records = _profileManager.ListProfiles();
            WindowsPlatformAdapter winAdapter = _platformAdapter as WindowsPlatformAdapter;
            WindowsPlatformAdapter.ProcessTreeSnapshot treeSnap = winAdapter != null
                ? winAdapter.CaptureProcessTreeSnapshot()
                : null;

            for (int i = 0; i < records.Count; i++)
            {
                ProfileRecord rec = records[i];
                string path = _profileManager.GetProfilePath(rec);
                ProfileRuntimeState state = (winAdapter != null && treeSnap != null)
                    ? winAdapter.InspectProfileRuntimeWithSnapshot(rec, path, treeSnap)
                    : _platformAdapter.InspectProfileRuntime(rec, path);

                if (includeDiskSizes)
                {
                    state.DataSizeBytes = _platformAdapter.CalculateDirectorySize(path);
                }

                snapshot.Profiles.Add(state);
            }

            return snapshot;
        }

        public OperationResult CreateProfile(string name, string notes)
        {
            OperationResult created = _profileManager.CreateProfile(name, notes);
            if (created.Success)
            {
                ProfileRecord record = created.Data as ProfileRecord;
                if (record != null)
                {
                    string path = _profileManager.GetProfilePath(record);
                    _platformAdapter.EnsureProfileDirectories(path);
                }
            }
            return created;
        }

        public OperationResult RenameProfile(string idOrName, string newName)
        {
            return _profileManager.RenameProfile(idOrName, newName);
        }

        public OperationResult LaunchOrFocusProfile(string idOrName)
        {
            ProfileRecord record = _profileManager.FindProfile(idOrName);
            if (record == null)
            {
                return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
            }

            ZaloInstallationInfo install = _platformAdapter.DiscoverOfficialInstallation(_customInstallRoot);
            if (!install.IsValid)
            {
                return OperationResult.Fail("INSTALL_NOT_FOUND", install.Error);
            }

            string path = _profileManager.GetProfilePath(record);
            ProfileRuntimeState state = _platformAdapter.InspectProfileRuntime(record, path);
            if (state.IsRunning)
            {
                return _platformAdapter.FocusOrRestoreProfile(install, record, path, state);
            }

            OperationResult res = _platformAdapter.LaunchProfile(install, record, path);
            if (res.Success)
            {
                _profileManager.UpdateLastLaunched(record.Id);
            }
            return res;
        }

        public OperationResult FocusProfile(string idOrName)
        {
            ProfileRecord record = _profileManager.FindProfile(idOrName);
            if (record == null)
            {
                return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
            }

            string path = _profileManager.GetProfilePath(record);
            ProfileRuntimeState state = _platformAdapter.InspectProfileRuntime(record, path);
            if (!state.IsRunning)
            {
                return OperationResult.Fail("NOT_RUNNING", "Profile '" + record.Name + "' hien khong chay.");
            }

            ZaloInstallationInfo install = _platformAdapter.DiscoverOfficialInstallation(_customInstallRoot);
            return _platformAdapter.FocusOrRestoreProfile(install, record, path, state);
        }

        public OperationResult StopProfile(string idOrName)
        {
            ProfileRecord record = _profileManager.FindProfile(idOrName);
            if (record == null)
            {
                return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
            }

            string path = _profileManager.GetProfilePath(record);
            ProfileRuntimeState state = _platformAdapter.InspectProfileRuntime(record, path);
            if (!state.IsRunning)
            {
                return OperationResult.Ok("Profile '" + record.Name + "' da dung truoc do.", state);
            }

            PrimaryZaloStatus primary = _platformAdapter.InspectPrimaryZaloStatus();
            return _platformAdapter.StopProfile(state, primary.LockingPids);
        }

        public OperationResult DeleteProfile(string idOrName)
        {
            ProfileRecord record = _profileManager.FindProfile(idOrName);
            if (record == null)
            {
                return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
            }

            string path = _profileManager.GetProfilePath(record);
            ProfileRuntimeState state = _platformAdapter.InspectProfileRuntime(record, path);
            if (state.IsRunning)
            {
                return OperationResult.Fail("PROFILE_RUNNING", "Vui long dong phien Zalo cua profile '" + record.Name + "' truoc khi xoa.");
            }

            return _profileManager.DeleteProfileRecordAndData(record.Id);
        }

        public OperationResult OpenProfileFolder(string idOrName)
        {
            ProfileRecord record = _profileManager.FindProfile(idOrName);
            if (record == null)
            {
                return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
            }
            string path = _profileManager.GetProfilePath(record);
            _platformAdapter.EnsureProfileDirectories(path);
            _platformAdapter.OpenDirectoryInFileManager(path);
            return OperationResult.Ok("Da mo thu muc du lieu.", path);
        }
    }
}
