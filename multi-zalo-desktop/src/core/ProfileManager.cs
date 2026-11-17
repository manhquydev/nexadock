using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace MultiZaloDesktop.Core
{
    public sealed class ProfileManager
    {
        private readonly string _storageRoot;
        private readonly string _profilesRoot;
        private readonly string _metadataFile;
        private readonly object _syncLock = new object();

        public string StorageRoot { get { return _storageRoot; } }
        public string ProfilesRoot { get { return _profilesRoot; } }
        public string MetadataFile { get { return _metadataFile; } }

        public ProfileManager(string storageRoot)
        {
            if (string.IsNullOrEmpty(storageRoot))
            {
                string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                storageRoot = Path.Combine(localAppData, "MultiZaloDesktop");
            }
            _storageRoot = Path.GetFullPath(storageRoot);
            _profilesRoot = Path.Combine(_storageRoot, "Profiles");
            _metadataFile = Path.Combine(_storageRoot, "profiles.json");
        }

        public void EnsureStorageInitialized()
        {
            Directory.CreateDirectory(_storageRoot);
            Directory.CreateDirectory(_profilesRoot);
            if (!File.Exists(_metadataFile))
            {
                SaveStoreInternal(new ProfileStore());
            }
        }

        public List<ProfileRecord> ListProfiles()
        {
            lock (_syncLock)
            {
                ProfileStore store = LoadStoreInternal();
                return new List<ProfileRecord>(store.Profiles);
            }
        }

        public ProfileRecord FindProfile(string idOrName)
        {
            if (string.IsNullOrEmpty(idOrName))
            {
                return null;
            }
            string needle = idOrName.Trim();
            lock (_syncLock)
            {
                ProfileStore store = LoadStoreInternal();
                for (int i = 0; i < store.Profiles.Count; i++)
                {
                    if (string.Equals(store.Profiles[i].Id, needle, StringComparison.OrdinalIgnoreCase))
                    {
                        return store.Profiles[i];
                    }
                }
                for (int i = 0; i < store.Profiles.Count; i++)
                {
                    if (string.Equals(store.Profiles[i].Name, needle, StringComparison.OrdinalIgnoreCase))
                    {
                        return store.Profiles[i];
                    }
                }
                return null;
            }
        }

        public string GetProfilePath(ProfileRecord record)
        {
            if (record == null || string.IsNullOrEmpty(record.DirectoryName))
            {
                throw new ArgumentException("Invalid profile record.");
            }

            string candidate = Path.GetFullPath(Path.Combine(_profilesRoot, record.DirectoryName));
            string normalizedRoot = _profilesRoot.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
            if (!candidate.StartsWith(normalizedRoot, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("Profile directory escapes configured ProfilesRoot.");
            }
            return candidate;
        }

        public OperationResult CreateProfile(string rawName, string notes)
        {
            string name = (rawName ?? string.Empty).Trim();
            if (name.Length == 0)
            {
                return OperationResult.Fail("INVALID_NAME", "Ten profile khong duoc de trong.");
            }
            if (name.Length > 64)
            {
                return OperationResult.Fail("INVALID_NAME", "Ten profile toi da 64 ky tu.");
            }

            lock (_syncLock)
            {
                EnsureStorageInitialized();
                ProfileStore store = LoadStoreInternal();
                for (int i = 0; i < store.Profiles.Count; i++)
                {
                    if (string.Equals(store.Profiles[i].Name, name, StringComparison.OrdinalIgnoreCase))
                    {
                        return OperationResult.Fail("DUPLICATE_NAME", "Profile '" + name + "' da ton tai.");
                    }
                }

                string id = Guid.NewGuid().ToString("N").Substring(0, 8);
                string dirName = SanitizeSlug(name) + "_" + id;

                ProfileRecord record = new ProfileRecord();
                record.Id = id;
                record.Name = name;
                record.DirectoryName = dirName;
                record.CreatedAt = DateTime.UtcNow.ToString("o");
                record.LastLaunchedAt = string.Empty;
                record.Notes = (notes ?? string.Empty).Trim();

                store.Profiles.Add(record);
                SaveStoreInternal(store);

                string fullDir = GetProfilePath(record);
                Directory.CreateDirectory(fullDir);

                return OperationResult.Ok("Da tao profile '" + name + "'.", record);
            }
        }

        public OperationResult RenameProfile(string idOrName, string newRawName)
        {
            string newName = (newRawName ?? string.Empty).Trim();
            if (newName.Length == 0)
            {
                return OperationResult.Fail("INVALID_NAME", "Ten moi khong duoc de trong.");
            }
            if (newName.Length > 64)
            {
                return OperationResult.Fail("INVALID_NAME", "Ten profile toi da 64 ky tu.");
            }

            lock (_syncLock)
            {
                ProfileStore store = LoadStoreInternal();
                ProfileRecord target = FindInStore(store, idOrName);
                if (target == null)
                {
                    return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
                }

                for (int i = 0; i < store.Profiles.Count; i++)
                {
                    ProfileRecord p = store.Profiles[i];
                    if (p.Id != target.Id && string.Equals(p.Name, newName, StringComparison.OrdinalIgnoreCase))
                    {
                        return OperationResult.Fail("DUPLICATE_NAME", "Da ton tai profile khac voi ten '" + newName + "'.");
                    }
                }

                target.Name = newName;
                SaveStoreInternal(store);
                return OperationResult.Ok("Da doi ten profile thanh '" + newName + "'.", target);
            }
        }

        public OperationResult UpdateLastLaunched(string profileId)
        {
            lock (_syncLock)
            {
                ProfileStore store = LoadStoreInternal();
                ProfileRecord target = FindInStore(store, profileId);
                if (target == null)
                {
                    return OperationResult.Fail("NOT_FOUND", "Profile not found.");
                }
                target.LastLaunchedAt = DateTime.UtcNow.ToString("o");
                SaveStoreInternal(store);
                return OperationResult.Ok("Updated.", target);
            }
        }

        public OperationResult DeleteProfileRecordAndData(string idOrName)
        {
            lock (_syncLock)
            {
                ProfileStore store = LoadStoreInternal();
                ProfileRecord target = FindInStore(store, idOrName);
                if (target == null)
                {
                    return OperationResult.Fail("NOT_FOUND", "Khong tim thay profile: " + idOrName);
                }

                string fullDir = GetProfilePath(target);
                store.Profiles.Remove(target);
                SaveStoreInternal(store);

                if (Directory.Exists(fullDir))
                {
                    Directory.Delete(fullDir, true);
                }

                return OperationResult.Ok("Da xoa profile '" + target.Name + "' va du lieu co lap.", target);
            }
        }

        private ProfileStore LoadStoreInternal()
        {
            EnsureStorageInitialized();
            string content = File.ReadAllText(_metadataFile, Encoding.UTF8);
            if (string.IsNullOrWhiteSpace(content))
            {
                return new ProfileStore();
            }
            ProfileStore store = JsonHelper.Deserialize<ProfileStore>(content);
            if (store == null)
            {
                store = new ProfileStore();
            }
            if (store.Profiles == null)
            {
                store.Profiles = new List<ProfileRecord>();
            }
            return store;
        }

        private void SaveStoreInternal(ProfileStore store)
        {
            string tempPath = _metadataFile + ".tmp";
            string json = JsonHelper.Serialize(store, true);
            File.WriteAllText(tempPath, json, new UTF8Encoding(false));
            if (File.Exists(_metadataFile))
            {
                File.Replace(tempPath, _metadataFile, null, true);
            }
            else
            {
                File.Move(tempPath, _metadataFile);
            }
        }

        private static ProfileRecord FindInStore(ProfileStore store, string idOrName)
        {
            if (string.IsNullOrEmpty(idOrName))
            {
                return null;
            }
            string needle = idOrName.Trim();
            for (int i = 0; i < store.Profiles.Count; i++)
            {
                if (string.Equals(store.Profiles[i].Id, needle, StringComparison.OrdinalIgnoreCase))
                {
                    return store.Profiles[i];
                }
            }
            for (int i = 0; i < store.Profiles.Count; i++)
            {
                if (string.Equals(store.Profiles[i].Name, needle, StringComparison.OrdinalIgnoreCase))
                {
                    return store.Profiles[i];
                }
            }
            return null;
        }

        private static string SanitizeSlug(string name)
        {
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < name.Length && sb.Length < 24; i++)
            {
                char ch = name[i];
                if ((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9'))
                {
                    sb.Append(char.ToLowerInvariant(ch));
                }
                else if (ch == ' ' || ch == '-' || ch == '_')
                {
                    if (sb.Length > 0 && sb[sb.Length - 1] != '-')
                    {
                        sb.Append('-');
                    }
                }
            }
            string slug = sb.ToString().Trim('-');
            if (string.IsNullOrEmpty(slug))
            {
                return "profile";
            }
            return slug;
        }
    }
}
