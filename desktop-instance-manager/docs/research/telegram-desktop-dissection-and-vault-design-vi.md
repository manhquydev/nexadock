# Phân tích Telegram Desktop Linux 7.4.4 và thiết kế Telegram Vault

## Phạm vi và cách đọc bằng chứng

- **[MEASURED]** là kết quả trực tiếp từ tarball được cung cấp, trích xuất read-only tại `/tmp/rea-td/Telegram/Telegram`; SHA-256 ELF: `53f9bb082d536f7ca0f96c1489f622eb93591ccaad650972b1eb8eb7ed82be72`.
- **[OSS-KNOWLEDGE]** là đối chiếu với mã nguồn mở `telegramdesktop/tdesktop` tại commit `c4bd064b` hoặc tài liệu Telegram chính thức. Chưa có bằng chứng xác nhận commit này là source commit dùng để build ELF của tarball; không coi nội dung source đó là phép đo trực tiếp trên binary.
- REA: `mcp__rea_open_binary` chạy ở orchestrator sau đó **khởi động thành công** và xác thực đúng identity của ELF (SHA-256 `53f9bb…be72`, `kind: executable`, `format: elf`, `arch: x86_64`) — xem mục 6. Provider phân tích sâu (Hopper/Ghidra) của REA không khả dụng trên máy này ở thời điểm phân tích (chi tiết và remediation ở mục 6), nên phần kiểm tra nội dung ELF dùng `strings`, `readelf`, `nm` và `file`; không chạy binary, không inject/hack client.

## Tóm tắt quyết định

1. **[MEASURED]** Binary có dấu vết nội bộ `MTP::...`, các request `upload_saveFilePart`, `upload_saveBigFilePart`, `upload_getFile` và module `Storage::Uploader`; tarball không kèm `.so` riêng, ELF không phụ thuộc `tdjson`/`libtdjson`. Bằng chứng phù hợp với MTProto implementation tích hợp của Telegram Desktop, không phải ứng dụng gọi TDLib/tdjson.
2. **[MEASURED]** Có các literal `tdata`, `tdata/config`, `/tdata/` và `media_cache`; không thấy literal `key_datas` hoặc `local_key`. **[OSS-KNOWLEDGE]** snapshot source đối chiếu tổ chức account storage quanh `tdata/`, map files và `user_<dataName>/`, nhưng không chứng minh layout chính xác của package này và không củng cố giả định `user_data/<id>`.
3. **[OSS-KNOWLEDGE]** Telegram Desktop tách cache nội bộ (`cache`, `media_cache`, có giới hạn size/time trong cấu hình) khỏi tải xuống ghi ra `toFile`/đường dẫn do người dùng chọn. Vì vậy Vault phải lưu bản nguồn trong thư mục Zalo, không dựa vào cache Telegram để giữ file lâu dài.
4. **[OSS-KNOWLEDGE]** Tài liệu Telegram công bố giới hạn upload 2 GB/tệp với user thường, 4 GB/tệp với Premium. Binary không cung cấp literal giới hạn có thể xác nhận chắc chắn bằng static string scan.
5. **Thiết kế NexaDock:** engine có session Telegram riêng; gửi file vào Saved Messages; queue SQLite at-least-once với kiểm tra idempotency/reconcile sau crash; upload theo part và giới hạn theo tier. Không đọc, sửa, tái sử dụng hay chia sẻ `tdata` của GUI.

## 1. TDLib/tdjson hay MTProto riêng?

**Kết luận: [MEASURED] package này chứa implementation MTProto của Telegram Desktop; không có bằng chứng binary sử dụng TDLib/tdjson.** Không khẳng định từ việc thiếu một tên symbol đơn lẻ: binary bị strip, nên kết luận dựa trên dấu vết tích cực cùng dependency/package inventory.

- **[MEASURED]** `tar -tf /home/manhquy/Downloads/td-setup-linux-x64-7.4.4.tar.xz` trả đúng ba entry: `Telegram/`, `Telegram/Updater`, `Telegram/Telegram`. `find /tmp/rea-td/Telegram -maxdepth 1 -type f -printf '%f\n'` trả `Telegram` và `Updater`: không có thư viện `.so` đi kèm.
- **[MEASURED]** `readelf -d /tmp/rea-td/Telegram/Telegram | grep NEEDED` liệt kê dependency giao diện/runtime như `libpangocairo-1.0.so.0`, `libcairo.so.2`, `libgio-2.0.so.0`, `libglib-2.0.so.0`, `libc.so.6`; không có `libtdjson`/TDLib.
- **[MEASURED]** `strings -a /tmp/rea-td/Telegram/Telegram | grep -F upload_saveFilePart` có `...MTPupload_saveFilePart...` và `{ upload_saveFilePart`; với `upload_saveBigFilePart` có `...MTPupload_saveBigFilePart...` và `{ upload_saveBigFilePart`; với `upload_getFile` có `...MTPupload_getFile...` và `{ upload_getFile`. `strings ... | grep -F N7Storage8Uploader` có tên lớp `Storage::Uploader` trong symbol mangled.
- **[MEASURED]** `for s in tdjson libtdjson TDLib; do strings -a ... | grep -F -c "$s"; done` cho kết quả lần lượt `0`, `0`, `0`. Đây là tìm literal trong ELF, không phải chứng minh tuyệt đối về mọi hình thức tích hợp.
- **[MEASURED]** `file /tmp/rea-td/Telegram/Telegram` xác định ELF64 x86-64, PIE, stripped. `nm -D -C ... | grep -Ei 'tdjson|tdlib|td::|mtproto|lib_mtproto|telegram::'` không cho kết quả export phù hợp; symbols nội bộ còn lại được nhìn thấy dưới dạng chuỗi mangled trong `strings`.

**[OSS-KNOWLEDGE]** Mã nguồn upstream đặt các request upload/download trong namespace `MTP`, module `Storage::Uploader` và `DownloadMtprotoTask` (`storage/file_upload.cpp`, `storage/download_manager_mtproto.cpp`). Đây củng cố cách đọc symbols trong binary, không xác nhận exact source commit của tarball.

## 2. Session storage, local key và account data

**Kết luận:** chỉ một phần layout có thể đo trực tiếp qua strings. Các tên `key_datas`, `local_key` và `user_data/<id>` không được xác nhận literal trong ELF; không nên dựa vào các tên này để thiết kế tích hợp.

- **[MEASURED]** `strings -a /tmp/rea-td/Telegram/Telegram | grep -F tdata` có `tdata/experimental_options.json`, `tdata/editing-theme.tdesktop-palette`, `tdata/config`, `tdata/prefix`, `tdata/update-manifest`, và `/tdata/`.
- **[MEASURED]** `strings -a ... | grep -F media_cache` trả `media_cache`; `strings -a ... | grep -F key_datas` và `... | grep -F local_key` không có output. `strings ... | grep -F user_data` chỉ trả các tên API như `cairo_scaled_font_get_user_data`, `gdk_window_get_children_with_user_data` và `hb_object_get_user_data`; không có path `user_data/<id>`.
- **[MEASURED]** Binary có chuỗi phiên bản `7.4.4` và embedded source paths như `/usr/src/tdesktop/Telegram/SourceFiles/data/data_saved_messages.cpp`; path build không chỉ ra commit source.
- **[OSS-KNOWLEDGE]** `storage_account.cpp` tại upstream commit `c4bd064b` định nghĩa `BaseGlobalPath()` là `cWorkingDir() + "tdata/"`; tạo `_basePath` theo hash `dataName`, `_tempPath` dạng `tdata/temp_<dataName>/`, và database path qua `tdata/user_<dataName>/`. `collectGoodNames()` liệt kê `map0`, `map1`, `maps`, `configs` cùng các key file khác. Đây không phải literal `user_data/<id>`; tên map files không đủ để kết luận schema chung `map<key,value>`.
- **[OSS-KNOWLEDGE]** Trong cùng snapshot source, `CreateLocalKey(passcode, salt)` tính SHA-512 prehash rồi `PKCS5_PBKDF2_HMAC(... EVP_sha512(), ...)`. `PrepareEncrypted` gọi `MTP::aesEncryptLocal`; `mtproto_auth_key.h` định nghĩa helper đó gọi `aesIgeEncryptRaw`, còn `mtproto_auth_key.cpp` hiện thực raw helper bằng OpenSSL `AES_ige_encrypt` (decrypt có đường tương ứng). Như vậy upstream source đối chiếu cho thấy local-encryption path dùng AES-IGE, song **không chứng minh `key_datas`/`local_key`, bytes, map schema, hay build source của binary 7.4.4**.

Nguồn upstream: [`storage_account.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/storage/storage_account.cpp), [`storage_file_utilities.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/storage/details/storage_file_utilities.cpp), [`mtproto_auth_key.h`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/mtproto/mtproto_auth_key.h), [`mtproto_auth_key.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/mtproto/mtproto_auth_key.cpp).

**[OSS-KNOWLEDGE] Hệ quả:** tdata là vùng dữ liệu nội bộ của Telegram Desktop, chứa account/session state được lưu dưới dạng mã hóa trong source đối chiếu; không phải API hoặc thư mục staging. Không giải mã, copy, symlink, chỉnh sửa, hay dùng chung với service sync.

## 3. Download, cache và file lưu bền

- **[MEASURED]** ELF có `media_cache`, `Storage::DownloadMtprotoTask`, `upload_getFile`, cùng nhiều `MTPupload_getFile` symbol. Đây là dấu vết media/file transfer, nhưng không cho biết eviction policy hay đích file của một lần download cụ thể.
- **[OSS-KNOWLEDGE]** `storage_account.cpp` tách hai đường dẫn cache: `cachePath()` trả `_databasePath + "cache"`, `cacheBigFilePath()` trả `_databasePath + "media_cache"`; `cacheSettings()` và `cacheBigFileSettings()` mang `totalSizeLimit` và `totalTimeLimit`. Đây là cache nội bộ do app quản lý theo giới hạn, không phải kho lưu trữ bền của Vault.
- **[OSS-KNOWLEDGE]** `file_download_mtproto.cpp` khởi tạo loader với các offset và `Storage::kDownloadPartSize`, `feedPart()` ghi từng part rồi `finalizeResult()` khi đủ; location-backed media có `cacheKey()` và `fileLocationKey()`. `download_manager_mtproto.cpp` lập hàng đợi ưu tiên và phân phối request part theo DC/session. Điều này giải thích phân mảnh/resume nội bộ; không nên kết luận rằng Telegram Desktop là công cụ đồng bộ file Zalo.
- **[OSS-KNOWLEDGE]** `FileLoader` nhận `toFile`, `LoadToCacheSetting`; khi ghi file sẽ mở `_filename`, ghi bytes và đóng file. Một file được yêu cầu `LoadToFileOnly` khác với một media cache có `cacheKey`. Tải về qua UI được lưu bền ở đường dẫn được chọn/đã cấu hình; file cache có thể được tái tạo và bị dọn theo các giới hạn cache. Source snapshot không đo được policy/default cụ thể của package 7.4.4.
- **Thiết kế NexaDock:** đọc file gốc từ profile `Profiles/<slug>/...Zalo Received Files`; giữ nguyên file nguồn, không đưa vào `tdata/*/cache`, `media_cache`, `/tmp` hoặc thư mục download Telegram làm “bản lưu vĩnh viễn”. Đích lâu dài là file cloud được gửi thành document vào Saved Messages; local DB chỉ lưu metadata/state, không biến cache Telegram thành nguồn sự thật.

Nguồn upstream: [`storage_account.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/storage/storage_account.cpp), [`file_download.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/storage/file_download.cpp), [`file_download_mtproto.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/storage/file_download_mtproto.cpp), [`download_manager_mtproto.cpp`](https://github.com/telegramdesktop/tdesktop/blob/c4bd064b/Telegram/SourceFiles/storage/download_manager_mtproto.cpp).

## 4. Giới hạn upload 2 GB / 4 GB

- **[MEASURED]** Binary có schema/request `upload_saveFilePart` và `upload_saveBigFilePart`. Quét regex các chuỗi có `2 ?GB`, `4 ?GB`, `2000000000`, `4000000000`, `2147483648` có match, nhưng output là token/chuỗi không mang nghĩa quota (`2GB7$5`, `54gB`, `200000000000000`, assert chứa dãy số dài, và thông báo TIFF với `-2147483648`); vì vậy không xác nhận được giới hạn theo tier từ ELF. Protocol thường/big cũng không chứng minh quota tài khoản.
- **[OSS-KNOWLEDGE — tài liệu chính thức]** Telegram công bố “Any user can upload ... each up to 2 GB” và Premium “send 4 GB files” trong [700 Million Users and Telegram Premium](https://telegram.org/blog/700-million-and-premium#4-gb-uploads). Đây là tài liệu sản phẩm chính thức, không phải kết quả đo từ ELF. Giới hạn cần được coi là cấu hình theo tier và kiểm tra/diễn giải lỗi server, không hardcode như hằng số vĩnh viễn.
- **[OSS-KNOWLEDGE]** Snapshot source upstream `file_upload.cpp` dùng `upload_saveFilePart`/`upload_saveBigFilePart`, chọn part size 32–512 KiB và ràng buộc `kDocumentMaxPartsCountDefault = 4000`; đó là chi tiết uploader của snapshot mã nguồn, không phải bằng chứng exact limit của binary/tài khoản.

## 5. Implications cho NexaDock Telegram Vault

Các mục dưới đây là khuyến nghị thiết kế, không phải hành vi đo được của Telegram Desktop:

1. **Session riêng.** Chọn một Telegram client headless có hỗ trợ upload MTProto (hoặc TDLib headless) và tạo authorization/session storage riêng cho NexaDock. Không mở/đọc/sao chép `tdata` của GUI, không dựa vào GUI đang chạy. Điều này tránh coupling với định dạng nội bộ, passcode/local key, phiên bản và thao tác sign-out/upgrade; đồng thời tách vòng đời service khỏi desktop client. Binary 7.4.4 cho thấy client dùng MTProto nội bộ nhưng không phơi một tdjson API công khai.
2. **Saved Messages làm đích.** Gửi file như document vào Saved Messages của tài khoản đã authorize. Đây là đích cloud phù hợp cho kho cá nhân; không giả định Telegram Desktop cache hay folder download giữ bản lâu dài. Sau phản hồi thành công, lưu server identifiers để reconcile, nhưng vẫn giữ nguồn Zalo theo retention policy của NexaDock.
3. **SQLite queue at-least-once.** Ghi job trước khi upload với identity ổn định (profile, canonical path, size, mtime và tốt hơn là content hash), state/attempt/progress và server result. Nếu tiến trình chết sau server chấp nhận nhưng trước khi DB commit, lần chạy lại có thể gửi trùng: at-least-once không đồng nghĩa exactly-once. Reconcile bằng metadata/message history hoặc dấu vết upload được ghi nhận trước khi retry; không đánh dấu hoàn tất chỉ dựa vào việc đã gửi bytes.
4. **Chunked upload.** Stream file từ đĩa theo part, giới hạn số request song song và lưu checkpoint/progress; tránh nạp toàn file vào RAM. Dùng API client xử lý đúng `saveFilePart`/`saveBigFilePart`, part count, retry/backoff, session/DC routing và tier cap. Không tự tái cài giao thức dựa trên một vài strings từ ELF.
5. **Không vượt giới hạn.** Trước upload, kiểm tra size so với tier/giới hạn client hiện hành; xử lý server rejection như outcome có thể retry sau khi cấu hình/tier đổi, nhưng không retry vòng lặp vô hạn. 2/4 GB là giới hạn sản phẩm công bố tại nguồn chính thức, còn điều kiện API thực tế phải được client/API phản hồi xác nhận.

## Nhật ký lệnh/bằng chứng chính

Các lệnh thực hiện trên bản giải nén (tarball gốc chỉ đọc):

```sh
tar -tf /home/manhquy/Downloads/td-setup-linux-x64-7.4.4.tar.xz
# Telegram/
# Telegram/Updater
# Telegram/Telegram

file /tmp/rea-td/Telegram/Telegram
# ELF 64-bit LSB pie executable, x86-64, dynamically linked, ... stripped

readelf -d /tmp/rea-td/Telegram/Telegram | grep NEEDED
# ... libpangocairo-1.0.so.0, libcairo.so.2, libgio-2.0.so.0, ... libc.so.6

strings -a /tmp/rea-td/Telegram/Telegram | grep -F upload_saveBigFilePart
# ...MTPupload_saveBigFilePart...
# { upload_saveBigFilePart

strings -a /tmp/rea-td/Telegram/Telegram | grep -F media_cache
# media_cache

sha256sum /tmp/rea-td/Telegram/Telegram
# 53f9bb082d536f7ca0f96c1489f622eb93591ccaad650972b1eb8eb7ed82be72  /tmp/rea-td/Telegram/Telegram
```

Đối chiếu source [OSS-KNOWLEDGE] được lấy trực tiếp từ các URL upstream đã liên kết ở từng mục; grep các đoạn source trả về `BaseGlobalPath`/`user_` ở `storage_account.cpp` dòng 132–145, cache paths ở dòng 1889–1935, PBKDF2/AES helper ở `storage_file_utilities.cpp` dòng 445–455 và 748, uploader part sizing ở `file_upload.cpp` dòng 41–61 và 152–166, download part queue ở `download_manager_mtproto.cpp` dòng 25–26 và 478+.

Output source upstream dùng để đối chiếu (đều là **[OSS-KNOWLEDGE]**, không khẳng định cùng source commit với binary):

```sh
curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/storage/storage_account.cpp \
  | grep -nE 'BaseGlobalPath|ComputeDatabasePath|user_|map0|map1|maps|cachePath\\(|cacheBigFilePath\\(|media_cache|totalTimeLimit'
# 132:[[nodiscard]] QString BaseGlobalPath() {
# 136:[[nodiscard]] QString ComputeDatabasePath(const QString &dataName) {
# 138:        + "user_" + dataName
# 281:        "map0",
# 282:        "map1",
# 283:        "maps",
# 1889:QString Account::cachePath() const {
# 1892:        return _databasePath + "cache";
# 1925:QString Account::cacheBigFilePath() const {
# 1928:        return _databasePath + "media_cache";

curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/storage/details/storage_file_utilities.cpp \
  | grep -nE 'CreateLocalKey|PBKDF2_HMAC|aesEncryptLocal|aesDecryptLocal'
# 445:MTP::AuthKeyPtr CreateLocalKey(
# 455:    PKCS5_PBKDF2_HMAC(
# 748:    MTP::aesEncryptLocal(...)
# 877:    aesDecryptLocal(...)

curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/storage/file_upload.cpp \
  | grep -nE 'kDocumentMaxPartsCountDefault|kDocumentUploadPartSize|docPartsCount'
# 43:constexpr auto kDocumentMaxPartsCountDefault = 4000;
# 46:constexpr auto kDocumentUploadPartSize0 = 32 * 1024;
# 49:constexpr auto kDocumentUploadPartSize1 = 64 * 1024;
# 52:constexpr auto kDocumentUploadPartSize2 = 128 * 1024;
# 55:constexpr auto kDocumentUploadPartSize3 = 256 * 1024;
# 58:constexpr auto kDocumentUploadPartSize4 = 512 * 1024;
# 166:return (docPartsCount <= kDocumentMaxPartsCountDefault);

```sh
curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/mtproto/mtproto_auth_key.cpp \
  | grep -nE 'aesIge(Encrypt|Decrypt)Raw|AES_ige_encrypt'
# 152:void aesIgeEncryptRaw(...)
# 159:    AES_ige_encrypt(..., AES_ENCRYPT);
# 162:void aesIgeDecryptRaw(...)
# 169:    AES_ige_encrypt(..., AES_DECRYPT);

curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/storage/file_download_mtproto.cpp \
  | grep -nE 'takeNextRequestOffset|_nextRequestOffset \\+=|feedPart\\(|finalizeResult\\(|cacheKey\\(|fileLocationKey\\('
# 137:int64 mtpFileLoader::takeNextRequestOffset() {
# 141:    _nextRequestOffset += Storage::kDownloadPartSize;
# 145:bool mtpFileLoader::feedPart(int64 offset, const QByteArray &bytes) {
# 157:        if (!finalizeResult()) {
# 204:Storage::Cache::Key mtpFileLoader::cacheKey() const {
# 216:std::optional<MediaKey> mtpFileLoader::fileLocationKey() const {
```

Tài liệu chính thức về giới hạn (nguồn ngoài, **[OSS-KNOWLEDGE]**):

```sh
curl -fsSL https://telegram.org/blog/700-million-and-premium \
  | grep -oE '.{0,100}4 GB files.{0,80}' | head -n 3
# ...With Telegram Premium, subscribers will now be able to send <strong>4 GB files</strong>...

curl -fsSL https://telegram.org/blog/700-million-and-premium \
  | grep -oE 'Any user can upload large files and media, each up to 2 GB[^<]*' | head -n 3
# Any user can upload large files and media, each up to 2 GB in size and enjoy unlimited storage in the Telegram Cloud for free.
```

```sh
curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/storage/file_download.cpp \
  | grep -nE 'FileLoader::FileLoader|_filename\\(toFile\\)|_toCache\\(toCache\\)|LoadToFileOnly|_file\\.open|_file\\.write|_file\\.close' | head -n 18
# 105:FileLoader::FileLoader(
# 118:, _filename(toFile)
# 120:, _toCache(toCache)
# 141:        if (!_fileIsOpen) _fileIsOpen = _file.open(QIODevice::WriteOnly);
# 147:        if (_file.write(_data) != qint64(_data.size())) {
# 155:        _file.close();
# 265:        || (_toCache != LoadToFileOnly)
```

```sh
curl -fsSL https://raw.githubusercontent.com/telegramdesktop/tdesktop/c4bd064b/Telegram/SourceFiles/mtproto/mtproto_auth_key.h \
  | grep -nE 'aesIgeEncryptRaw|aesEncryptLocal|aesIgeDecryptRaw|aesDecryptLocal'
# 68:void aesIgeEncryptRaw(...)
# 69:void aesIgeDecryptRaw(...)
# 85:inline void aesEncryptLocal(...)
# 89:    return aesIgeEncryptRaw(...);
# 106:inline void aesDecryptLocal(...)
# 110:    return aesIgeDecryptRaw(...);
```

Nếu REA deep provider khả dụng, các phân tích tiếp theo ưu tiên: `binary_overview` (segments/procedures inventory), `search_procedures` cho `MTP`/`Storage::Uploader` để xem call graph upload, và `trace_feature` trên `upload_saveBigFilePart`.

## 6. REA: kết quả chạy thực và giới hạn provider

- **[MEASURED]** `mcp__rea_open_binary --path /tmp/rea-td/Telegram/Telegram` (chạy ở orchestrator) trả thành công: `{"sha256":"53f9bb082d536f7ca0f96c1489f622eb93591ccaad650972b1eb8eb7ed82be72","kind":"executable","format":"elf","architecture":"x86_64"}` — xác thực độc lập cùng content hash như `sha256sum` ở mục 1.
- **[MEASURED]** Các lệnh phân tích sâu sau đó (`mcp__rea_binary_overview`) trả `provider_unavailable` (`unsupported_hopper_build`, lỗi ở bước launch provider, không phải lỗi target). `rea doctor` xác nhận: Hopper launcher `/opt/hopper/bin/Hopper` không khớp bản authenticated (`hopper-version: config_drift`), Ghidra `GHIDRA_INSTALL_DIR` chưa set, IDA/jadx chưa đăng ký.
- **Remediation (chưa chạy — cần quyền hệ thống, ngoài phạm vi run tự trị):** `rea setup -y --install-hopper` (cài qua `pkexec apt-get`, tải bản Hopper demo chính thức có PGP verify) hoặc set `GHIDRA_INSTALL_DIR` tới bản Ghidra `>= 12.2`. Khi một trong hai sẵn sàng, có thể chạy lại phân tích sâu (procedures/pseudocode) trên cùng ELF đã được REA xác thực để bổ sung bằng chứng; các kết luận chính của báo cáo này không phụ thuộc bước đó.
