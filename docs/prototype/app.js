/* KHOANG Interactive Prototype Controller
 * Strict DOM APIs only (ZERO .innerHTML). Supports URL query params & live state switching.
 */

const ACCOUNT_COLORS = [
  "#0f6b66",
  "#1e56a0",
  "#6d3b9e",
  "#a92b5b",
  "#a14a0a",
  "#1c6e3b",
  "#0e6488",
  "#4b468e",
];

const I18N = {
  "vi-VN": {
    skipToMain: "Bỏ qua đến nội dung chính",
    navAccounts: "Tài khoản",
    navBackups: "Sao lưu",
    navSettings: "Cài đặt",
    navHelp: "Trợ giúp",
    ready: "Sẵn sàng",
    disconnected: "Mất kết nối",
    openSummary: (n) => `${n} đang mở`,
    countSummary: (n) => `${n} tài khoản`,
    homeTitle: "Không gian tài khoản",
    homeSubtitle: "Mỗi tài khoản. Một không gian riêng.",
    stopAll: "Đóng tất cả",
    refresh: "Làm mới",
    addAccount: "Thêm tài khoản",
    searchLabel: "Tìm tài khoản",
    searchPlaceholder: "Tìm theo tên hoặc ghi chú…",
    viewGrid: "Lưới",
    viewList: "Danh sách",
    emptyTitle: "Chưa có khoang tài khoản nào",
    emptyHint:
      "Tạo khoang riêng đầu tiên để đăng nhập tài khoản Zalo thứ hai song song mà không ảnh hưởng đến Zalo chính của bạn.",
    primaryZaloTitle: "Zalo chính trên máy",
    primaryProtectedBadge: "Được bảo vệ · Chỉ đọc",
    primaryZaloDesc:
      "KHOANG không bao giờ chỉnh sửa, sao lưu hoặc đóng tiến trình Zalo chính của bạn.",
    primaryRunning: "Đang chạy",
    primaryStopped: "Chưa mở",
    primaryUnknown: "Không rõ trạng thái",
    backupsTitle: "Bản sao lưu",
    backupsSubtitle:
      "Khôi phục luôn tạo thành khoang tài khoản mới, không ghi đè dữ liệu hiện có.",
    backupsEmptyTitle: "Chưa có bản sao lưu nào",
    backupsEmptyHint:
      "Bạn có thể sao lưu bất kỳ khoang tài khoản nào đang dừng từ menu tuỳ chọn trên thẻ tài khoản.",
    colAccount: "Tài khoản",
    colBackupTime: "Thời gian",
    colSize: "Dung lượng",
    colAction: "Thao tác",
    restoreBtn: "Khôi phục",
    settingsTitle: "Cài đặt",
    settingsSubtitle: "Tuỳ chỉnh giao diện, ngôn ngữ và kiểm tra hệ thống.",
    languageLegend: "Ngôn ngữ hiển thị",
    themeLegend: "Giao diện (Chế độ màu)",
    themeSystem: (resolved) =>
      `Theo hệ thống (Hiện tại: ${resolved === "dark" ? "Tối" : "Sáng"})`,
    themeLight: "Sáng (Warm Stone)",
    themeDark: "Tối (Warm Obsidian)",
    storageTitle: "Dữ liệu & Chẩn đoán",
    toggleSizesLabel: "Tính dung lượng từng khoang tài khoản khi làm mới",
    openDiagnostics: "Mở Chẩn đoán hệ thống",
    aboutTitle: "Thông tin KHOANG",
    aboutApp: "Ứng dụng",
    aboutLicense: "Giấy phép",
    backToAccounts: "← Quay lại Tài khoản",
    diagnosticsTitle: "Chẩn đoán & Cô lập",
    diagnosticsSubtitle:
      "Kiểm tra mức độ tách biệt dữ liệu giữa các khoang tài khoản.",
    runIsolation: "Kiểm tra cô lập",
    readinessTitle: "Sẵn sàng hệ thống",
    isolationResultsTitle: "Kết quả kiểm tra cô lập",
    sanitizedLogsTitle: "Nhật ký chẩn đoán (đã ẩn thông tin nhạy cảm)",
    colCheck: "Hạng mục",
    colStatus: "Trạng thái",
    colDetail: "Chi tiết",
    helpTitle: "Trợ giúp & An toàn",
    helpSubtitle: "Hướng dẫn sử dụng khoang tài khoản an toàn.",
    helpQ1: "1. Zalo chính trên máy có bị ảnh hưởng không?",
    helpA1:
      "Không. KHOANG giữ Zalo chính ở chế độ chỉ đọc được bảo vệ và tạo thư mục riêng biệt cho từng tài khoản mới.",
    helpQ2: '2. Khi thấy trạng thái "Cần khôi phục" thì làm gì?',
    helpA2:
      'Điều này xảy ra khi phiên trước đóng đột ngột. Nhấn "Khôi phục & Mở" để dọn dẹp khoá cũ và khởi động lại an toàn.',
    createTitle: "Tạo khoang tài khoản mới",
    accountNameLabel: "Tên tài khoản",
    notesLabel: "Ghi chú (tuỳ chọn)",
    colorLabel: "Màu nhận diện",
    cancel: "Huỷ",
    createSubmit: "Tạo khoang",
    createNameRequired: "Vui lòng nhập tên cho khoang tài khoản.",
    createStep2Title: (name) => `Đã tạo "${name}"`,
    createStep2Hint:
      "Khoang dữ liệu riêng đã sẵn sàng. Bạn có thể mở Zalo ngay để quét mã đăng nhập.",
    later: "Để sau",
    launchNow: "Mở Zalo ngay",
    renameTitle: "Đổi tên & màu nhận diện",
    save: "Lưu thay đổi",
    deleteTitle: "Xoá khoang tài khoản?",
    deleteWarning: (name) =>
      `Khoang "${name}" và toàn bộ dữ liệu đăng nhập riêng của nó sẽ bị xoá vĩnh viễn. Zalo chính trên máy hoàn toàn không bị ảnh hưởng.`,
    deleteConfirm: "Xoá khoang",
    restoreTitle: "Khôi phục thành khoang mới",
    restoreSubtitle: (name, ts) => `Từ bản sao lưu "${name}" (${ts})`,
    newAccountNameLabel: "Tên khoang mới",
    restoreSubmit: "Khôi phục",
    detailsTitle: "Chi tiết khoang tài khoản",
    close: "Đóng",
    advancedInfo: "Thông tin nâng cao",
    technicalDetails: "Chi tiết kỹ thuật",
    dismiss: "Đóng",
    zaloMissing:
      "Chưa tìm thấy Zalo Desktop trên máy. Vui lòng cài đặt Zalo trước khi mở khoang tài khoản.",
    stateRunning: "Đang chạy",
    stateStopped: "Đã dừng",
    stateStarting: "Đang mở…",
    stateStopping: "Đang đóng…",
    stateRecovery: "Cần khôi phục",
    stateError: "Lỗi",
    stateUnknown: "Không rõ",
    ctaLaunch: "Mở Zalo",
    ctaFocus: "Đưa lên trước",
    ctaRecover: "Khôi phục & Mở",
    ctaPendingOpen: "Đang mở…",
    ctaPendingStop: "Đang đóng…",
    recoveryHint: "Phiên trước đóng chưa sạch. Mở lại để tự động dọn khoá.",
    menuOpenLabel: (name) => `Tuỳ chọn cho ${name}`,
    menuDetails: "Xem chi tiết",
    menuRename: "Đổi tên & màu",
    menuBackup: "Sao lưu khoang",
    menuStop: "Đóng Zalo này",
    menuDelete: "Xoá khoang",
    closeZaloFirst: "Đóng Zalo trước",
    detailStatus: "Trạng thái",
    detailSize: "Dung lượng",
    detailLastOpened: "Mở gần nhất",
    detailNotes: "Ghi chú",
    detailFolder: "Thư mục khoang",
    detailId: "Mã khoang",
    toastCreated: (name) => `Đã tạo khoang "${name}".`,
    toastBackup: (name) => `Đã tạo bản sao lưu cho "${name}".`,
    toastRestored: (name) => `Đã khôi phục thành khoang mới "${name}".`,
    hostErrorMsg:
      "Không thể kết nối tới tiến trình nền của KHOANG. Vui lòng thử làm mới hoặc kiểm tra chẩn đoán.",
  },
  "en-US": {
    skipToMain: "Skip to main content",
    navAccounts: "Accounts",
    navBackups: "Backups",
    navSettings: "Settings",
    navHelp: "Help",
    ready: "Ready",
    disconnected: "Disconnected",
    openSummary: (n) => `${n} running`,
    countSummary: (n) => `${n} accounts`,
    homeTitle: "Account Spaces",
    homeSubtitle: "Your accounts. Their own space.",
    stopAll: "Stop all",
    refresh: "Refresh",
    addAccount: "Add account",
    searchLabel: "Search accounts",
    searchPlaceholder: "Search by name or notes…",
    viewGrid: "Grid",
    viewList: "List",
    emptyTitle: "No account spaces yet",
    emptyHint:
      "Create your first isolated space to run a second Zalo account side-by-side without touching your primary Zalo.",
    primaryZaloTitle: "Primary Zalo on this PC",
    primaryProtectedBadge: "Protected · Read-only",
    primaryZaloDesc:
      "KHOANG never modifies, backs up, or closes your primary Zalo installation.",
    primaryRunning: "Running",
    primaryStopped: "Not running",
    primaryUnknown: "Status unknown",
    backupsTitle: "Backups",
    backupsSubtitle:
      "Restoring always creates a new account space without overwriting existing data.",
    backupsEmptyTitle: "No backups yet",
    backupsEmptyHint:
      "You can back up any stopped account space from the options menu on its card.",
    colAccount: "Account",
    colBackupTime: "Created",
    colSize: "Size",
    colAction: "Action",
    restoreBtn: "Restore",
    settingsTitle: "Settings",
    settingsSubtitle: "Customize theme, language, and system diagnostics.",
    languageLegend: "Display language",
    themeLegend: "Appearance (Color mode)",
    themeSystem: (resolved) =>
      `Follow system (Currently: ${resolved === "dark" ? "Dark" : "Light"})`,
    themeLight: "Light (Warm Stone)",
    themeDark: "Dark (Warm Obsidian)",
    storageTitle: "Storage & Diagnostics",
    toggleSizesLabel: "Calculate account space sizes on refresh",
    openDiagnostics: "Open System Diagnostics",
    aboutTitle: "About KHOANG",
    aboutApp: "Application",
    aboutLicense: "License",
    backToAccounts: "← Back to Accounts",
    diagnosticsTitle: "Diagnostics & Isolation",
    diagnosticsSubtitle:
      "Verify data separation across your account spaces.",
    runIsolation: "Verify isolation",
    readinessTitle: "System readiness",
    isolationResultsTitle: "Isolation check results",
    sanitizedLogsTitle: "Sanitized diagnostics log",
    colCheck: "Check",
    colStatus: "Status",
    colDetail: "Detail",
    helpTitle: "Help & Safety",
    helpSubtitle: "How KHOANG keeps each account space safe.",
    helpQ1: "1. Will my primary Zalo installation be affected?",
    helpA1:
      "No. KHOANG keeps your primary Zalo in protected read-only mode and creates a separate folder for each new account.",
    helpQ2: '2. What should I do when I see "Recovery needed"?',
    helpA2:
      'This happens if a previous session closed unexpectedly. Click "Recover & Open" to clean up stale locks safely.',
    createTitle: "Create new account space",
    accountNameLabel: "Account name",
    notesLabel: "Notes (optional)",
    colorLabel: "Identity color",
    cancel: "Cancel",
    createSubmit: "Create space",
    createNameRequired: "Please enter a name for this account space.",
    createStep2Title: (name) => `Created "${name}"`,
    createStep2Hint:
      "Your isolated data space is ready. You can open Zalo now to scan the login QR code.",
    later: "Later",
    launchNow: "Open Zalo now",
    renameTitle: "Rename & identity color",
    save: "Save changes",
    deleteTitle: "Delete account space?",
    deleteWarning: (name) =>
      `Space "${name}" and its isolated login data will be permanently deleted. Your primary Zalo is untouched.`,
    deleteConfirm: "Delete space",
    restoreTitle: "Restore as new space",
    restoreSubtitle: (name, ts) => `From backup "${name}" (${ts})`,
    newAccountNameLabel: "New space name",
    restoreSubmit: "Restore",
    detailsTitle: "Account space details",
    close: "Close",
    advancedInfo: "Advanced details",
    technicalDetails: "Technical details",
    dismiss: "Dismiss",
    zaloMissing:
      "Zalo Desktop was not found on this PC. Please install Zalo before launching an account space.",
    stateRunning: "Running",
    stateStopped: "Stopped",
    stateStarting: "Starting…",
    stateStopping: "Stopping…",
    stateRecovery: "Recovery needed",
    stateError: "Error",
    stateUnknown: "Unknown",
    ctaLaunch: "Open Zalo",
    ctaFocus: "Bring to front",
    ctaRecover: "Recover & Open",
    ctaPendingOpen: "Starting…",
    ctaPendingStop: "Stopping…",
    recoveryHint: "Previous session ended uncleanly. Reopen to clean locks.",
    menuOpenLabel: (name) => `Options for ${name}`,
    menuDetails: "View details",
    menuRename: "Rename & color",
    menuBackup: "Back up space",
    menuStop: "Close this Zalo",
    menuDelete: "Delete space",
    closeZaloFirst: "Close Zalo first",
    detailStatus: "Status",
    detailSize: "Storage size",
    detailLastOpened: "Last opened",
    detailNotes: "Notes",
    detailFolder: "Space folder",
    detailId: "Space ID",
    toastCreated: (name) => `Created space "${name}".`,
    toastBackup: (name) => `Backup created for "${name}".`,
    toastRestored: (name) => `Restored as new space "${name}".`,
    hostErrorMsg:
      "Unable to reach the KHOANG background service. Try refreshing or open Diagnostics.",
  },
};

const SAMPLE_PROFILES_3 = [
  {
    id: "acc-work",
    name: "Công việc · Studio",
    notes: "Khách hàng & dự án thiết kế",
    state: "running",
    colorIndex: 0,
    sizeMb: 412,
    lastOpened: "Hôm nay, 09:14",
    folder: "Profiles/cong-viec_acc-work",
  },
  {
    id: "acc-shop",
    name: "Bán hàng Online",
    notes: "Chăm sóc đơn hàng Shopee & TikTok",
    state: "recovery_required",
    colorIndex: 4,
    sizeMb: 285,
    lastOpened: "Hôm qua, 21:40",
    folder: "Profiles/ban-hang_acc-shop",
  },
  {
    id: "acc-family",
    name: "Gia đình",
    notes: "",
    state: "stopped",
    colorIndex: 1,
    sizeMb: 194,
    lastOpened: "16/11/2026",
    folder: "Profiles/gia-dinh_acc-family",
  },
];

function build20Profiles() {
  const names = [
    "Công việc · Studio",
    "Bán hàng Q1",
    "CSKH Miền Nam",
    "CSKH Miền Bắc",
    "Kho vận & Giao nhận",
    "Đối tác Agency",
    "Tài khoản Dự phòng",
    "Nhóm Dự án Alpha",
    "Tuyển dụng HR",
    "Kế toán · Hoá đơn",
    "Marketing Zalo OA",
    "Cộng đồng VIP",
    "Gia đình",
    "Hỗ trợ Kỹ thuật",
    "Chi nhánh Đà Nẵng",
    "Chi nhánh Cần Thơ",
    "Nhà cung cấp",
    "Sỉ & Đại lý",
    "Bảo hành",
    "Thử nghiệm",
  ];
  const states = ["running", "stopped", "stopped", "recovery_required", "starting", "error"];
  return names.map((name, i) => ({
    id: `acc-${i + 1}`,
    name,
    notes: i % 3 === 0 ? "Khoang tách biệt cho nhóm vận hành" : "",
    state: states[i % states.length],
    colorIndex: i % ACCOUNT_COLORS.length,
    sizeMb: 120 + i * 24,
    lastOpened: i % 2 === 0 ? "Hôm nay, 09:10" : "17/11/2026",
    folder: `Profiles/space-${i + 1}_acc-${i + 1}`,
  }));
}

const SAMPLE_BACKUPS = [
  {
    backupDir: "Backups/cong-viec_20261118",
    profileName: "Công việc · Studio",
    timestamp: "18/11/2026 22:15",
    sizeMb: 408,
    colorIndex: 0,
  },
  {
    backupDir: "Backups/ban-hang_20261115",
    profileName: "Bán hàng Online",
    timestamp: "15/11/2026 18:04",
    sizeMb: 279,
    colorIndex: 4,
  },
];

const state = {
  locale: "vi-VN",
  theme: "system",
  view: "profiles",
  viewMode: "grid",
  dataset: "3",
  profiles: structuredClone(SAMPLE_PROFILES_3),
  backups: structuredClone(SAMPLE_BACKUPS),
  primaryZalo: { is_running: true },
  zaloInstalled: true,
  hostConnected: true,
  loading: false,
  bannerError: null,
  searchQuery: "",
  withSizes: true,
  activeMenuId: null,
  dialogTargetId: null,
  dialogBackup: null,
  lastCreatedId: null,
  lastInvoker: null,
};

function t(key, ...args) {
  const dict = I18N[state.locale] || I18N["vi-VN"];
  const val = dict[key];
  return typeof val === "function" ? val(...args) : val || key;
}

function isProfileActiveOrUnclean(prof) {
  return ["running", "starting", "stopping", "recovery_required"].includes(prof.state);
}

function createSvgIcon(paths, size = 16) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  for (const d of paths) {
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", d);
    svg.appendChild(p);
  }
  return svg;
}

function stateBadgeElement(profState) {
  const badge = document.createElement("span");
  badge.className = "status-badge";
  badge.dataset.state = profState;

  let icon;
  let label;
  switch (profState) {
    case "running":
      icon = createSvgIcon(["M20 6 9 17l-5-5"], 12);
      label = t("stateRunning");
      break;
    case "starting":
      icon = createSvgIcon(["M12 2v4", "M12 18v4", "M4.93 4.93l2.83 2.83"], 12);
      label = t("stateStarting");
      break;
    case "stopping":
      icon = createSvgIcon(["M12 2v4", "M12 18v4"], 12);
      label = t("stateStopping");
      break;
    case "recovery_required":
      icon = createSvgIcon(["M12 9v4", "M12 17h.01", "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z"], 12);
      label = t("stateRecovery");
      break;
    case "error":
      icon = createSvgIcon(["M18 6 6 18", "M6 6l12 12"], 12);
      label = t("stateError");
      break;
    default:
      icon = createSvgIcon(["M12 12m-5 0a5 5 0 1 0 10 0a5 5 0 1 0 -10 0"], 12);
      label = t("stateStopped");
  }
  badge.append(icon, document.createTextNode(label));
  return badge;
}

function applyTheme(theme) {
  state.theme = theme;
  try {
    if (theme === "system") localStorage.removeItem("multi-zalo.theme");
    else localStorage.setItem("multi-zalo.theme", theme);
  } catch {}
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (meta) {
    meta.content = theme === "system" ? "light dark" : theme;
  }
  const osDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const sysLabel = document.getElementById("theme-system-label");
  if (sysLabel) sysLabel.textContent = t("themeSystem", osDark ? "dark" : "light");

  for (const radio of document.querySelectorAll('input[name="ui-theme"]')) {
    radio.checked = radio.value === theme;
  }
}

function applyLocale(locale) {
  state.locale = locale === "en-US" ? "en-US" : "vi-VN";
  document.documentElement.lang = state.locale === "en-US" ? "en" : "vi";
  try {
    localStorage.setItem("multi-zalo.locale", state.locale);
  } catch {}

  for (const el of document.querySelectorAll("[data-i18n]")) {
    const key = el.dataset.i18n;
    const val = I18N[state.locale][key];
    if (typeof val === "string") el.textContent = val;
  }
  const searchInput = document.getElementById("profile-search");
  if (searchInput) searchInput.placeholder = t("searchPlaceholder");

  for (const radio of document.querySelectorAll('input[name="ui-locale"]')) {
    radio.checked = radio.value === state.locale;
  }
  applyTheme(state.theme);
  renderAll();
}

function announcePolite(msg) {
  const el = document.getElementById("live-polite");
  if (!el) return;
  el.textContent = "";
  requestAnimationFrame(() => {
    el.textContent = msg;
  });
}

function showToast(msg) {
  if (document.querySelector("dialog[open]")) return;
  const region = document.getElementById("toast-region");
  if (!region) return;
  const toast = document.createElement("div");
  toast.className = "toast-item";
  toast.setAttribute("popover", "manual");
  const span = document.createElement("span");
  span.textContent = msg;
  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "btn btn-ghost";
  closeBtn.textContent = t("dismiss");
  closeBtn.addEventListener("click", () => {
    try {
      toast.hidePopover?.();
    } catch {}
    toast.remove();
  });
  toast.append(span, closeBtn);
  region.appendChild(toast);
  try {
    if (typeof toast.showPopover === "function") toast.showPopover();
    else toast.classList.add("is-open");
  } catch {
    toast.classList.add("is-open");
  }
  announcePolite(msg);
  setTimeout(() => {
    if (toast.isConnected) toast.remove();
  }, 4500);
}

function switchView(viewName) {
  state.view = viewName;
  const views = ["profiles", "backups", "settings", "diagnostics", "help"];
  for (const v of views) {
    const sec = document.getElementById(`view-${v}`);
    if (sec) sec.hidden = v !== viewName;
  }
  for (const btn of document.querySelectorAll(".top-nav-btn[data-view]")) {
    const active = btn.dataset.view === viewName;
    btn.classList.toggle("is-active", active);
    if (active) btn.setAttribute("aria-current", "page");
    else btn.removeAttribute("aria-current");
  }
  renderAll();
}

function closeActiveMenu(restoreFocus = false) {
  if (!state.activeMenuId) return;
  const menu = document.getElementById(`menu-${state.activeMenuId}`);
  const trigger = document.getElementById(`menu-btn-${state.activeMenuId}`);
  if (menu) {
    try {
      if (typeof menu.hidePopover === "function" && menu.matches(":popover-open")) {
        menu.hidePopover();
      }
    } catch {}
    menu.classList.remove("is-open");
  }
  if (trigger) {
    trigger.setAttribute("aria-expanded", "false");
    if (restoreFocus) trigger.focus();
  }
  state.activeMenuId = null;
}

function buildMenuItem(label, onClick, { danger = false, disabled = false, hint = "" } = {}) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = danger ? "menu-item menu-item-danger" : "menu-item";
  btn.setAttribute("role", "menuitem");
  if (disabled) btn.setAttribute("aria-disabled", "true");

  const row = document.createElement("span");
  row.className = "menu-item-row";
  row.textContent = label;
  btn.appendChild(row);

  if (hint) {
    const hintEl = document.createElement("span");
    hintEl.className = "menu-item-hint";
    hintEl.textContent = hint;
    btn.appendChild(hintEl);
  }

  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    if (disabled) return;
    closeActiveMenu(false);
    onClick();
  });
  return btn;
}

function renderProfiles() {
  const listEl = document.getElementById("profiles-list");
  const emptyEl = document.getElementById("profiles-empty");
  const loadingEl = document.getElementById("profiles-loading");
  const toolbarEl = document.getElementById("accounts-toolbar");
  const openCreateBtn = document.getElementById("btn-open-create");
  const stopAllBtn = document.getElementById("btn-stop-all");
  const countEl = document.getElementById("accounts-count-summary");
  const runningSummaryEl = document.getElementById("topbar-open-summary");

  const runningCount = state.profiles.filter((p) => p.state === "running").length;
  runningSummaryEl.textContent = t("openSummary", runningCount);
  stopAllBtn.hidden = runningCount < 2;

  loadingEl.hidden = !state.loading;
  if (state.loading) {
    emptyEl.hidden = true;
    listEl.hidden = true;
    toolbarEl.hidden = true;
    return;
  }

  const isEmpty = state.profiles.length === 0;
  emptyEl.hidden = !isEmpty;
  toolbarEl.hidden = isEmpty;
  openCreateBtn.hidden = isEmpty; // Avoid duplicate CTA in empty state

  const q = state.searchQuery.trim().toLowerCase();
  const filtered = q
    ? state.profiles.filter(
        (p) =>
          p.name.toLowerCase().includes(q) ||
          (p.notes && p.notes.toLowerCase().includes(q))
      )
    : state.profiles;

  countEl.textContent = t("countSummary", filtered.length);
  listEl.hidden = isEmpty;
  listEl.dataset.viewMode = state.viewMode;

  const cards = filtered.map((prof) => {
    const wrap = document.createElement("div");
    wrap.className = "account-card-wrap";

    const card = document.createElement("article");
    card.className = "account-card";
    card.dataset.profileId = prof.id;
    card.dataset.state = prof.state;

    const header = document.createElement("div");
    header.className = "account-card-header";

    const avatar = document.createElement("div");
    avatar.className = "account-avatar";
    avatar.style.setProperty(
      "--avatar-color",
      ACCOUNT_COLORS[prof.colorIndex % ACCOUNT_COLORS.length]
    );
    avatar.textContent = (prof.name.trim()[0] || "K").toUpperCase();

    const ident = document.createElement("div");
    ident.className = "account-identity";
    const h3 = document.createElement("h3");
    h3.className = "account-name";
    h3.textContent = prof.name;
    ident.append(h3, stateBadgeElement(prof.state));

    header.append(avatar, ident);
    card.appendChild(header);

    if (prof.notes && prof.notes.trim()) {
      const notes = document.createElement("p");
      notes.className = "account-notes";
      notes.textContent = prof.notes;
      card.appendChild(notes);
    }

    if (prof.state === "recovery_required") {
      const rec = document.createElement("p");
      rec.className = "account-recovery-hint";
      rec.textContent = t("recoveryHint");
      card.appendChild(rec);
    }

    const footer = document.createElement("div");
    footer.className = "account-footer";

    const primaryBtn = document.createElement("button");
    primaryBtn.type = "button";
    primaryBtn.className = "btn btn-primary";
    if (!state.zaloInstalled || prof.state === "starting" || prof.state === "stopping") {
      primaryBtn.disabled = true;
    }
    if (prof.state === "running") primaryBtn.textContent = t("ctaFocus");
    else if (prof.state === "recovery_required") primaryBtn.textContent = t("ctaRecover");
    else if (prof.state === "starting") primaryBtn.textContent = t("ctaPendingOpen");
    else if (prof.state === "stopping") primaryBtn.textContent = t("ctaPendingStop");
    else primaryBtn.textContent = t("ctaLaunch");

    primaryBtn.addEventListener("click", () => {
      if (prof.state === "running") {
        showToast(`${prof.name}`);
      } else {
        prof.state = "running";
        renderProfiles();
      }
    });

    const menuWrap = document.createElement("div");
    menuWrap.className = "menu-anchor";

    const menuBtn = document.createElement("button");
    menuBtn.type = "button";
    menuBtn.id = `menu-btn-${prof.id}`;
    menuBtn.className = "btn btn-icon";
    menuBtn.setAttribute("aria-haspopup", "menu");
    menuBtn.setAttribute("aria-expanded", "false");
    menuBtn.setAttribute("aria-controls", `menu-${prof.id}`);
    menuBtn.setAttribute("aria-label", t("menuOpenLabel", prof.name));
    menuBtn.appendChild(
      createSvgIcon(["M12 12h.01", "M12 5h.01", "M12 19h.01"], 16)
    );

    const menu = document.createElement("div");
    menu.id = `menu-${prof.id}`;
    menu.className = "overflow-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("popover", "auto");

    const blocked = isProfileActiveOrUnclean(prof);
    menu.append(
      buildMenuItem(t("menuDetails"), () => openDetailsDialog(prof, menuBtn)),
      buildMenuItem(t("menuRename"), () => openRenameDialog(prof, menuBtn)),
      buildMenuItem(
        t("menuBackup"),
        () => {
          state.backups.unshift({
            backupDir: `Backups/${prof.id}_${Date.now()}`,
            profileName: prof.name,
            timestamp: "19/11/2026 09:38",
            sizeMb: prof.sizeMb,
            colorIndex: prof.colorIndex,
          });
          showToast(t("toastBackup", prof.name));
          renderBackups();
        },
        { disabled: blocked, hint: blocked ? t("closeZaloFirst") : "" }
      )
    );

    if (prof.state === "running") {
      menu.appendChild(
        buildMenuItem(t("menuStop"), () => {
          prof.state = "stopped";
          renderProfiles();
        })
      );
    }

    const sep = document.createElement("div");
    sep.className = "menu-separator";
    sep.setAttribute("role", "separator");
    menu.appendChild(sep);

    menu.appendChild(
      buildMenuItem(
        t("menuDelete"),
        () => openDeleteDialog(prof, menuBtn),
        { danger: true, disabled: blocked, hint: blocked ? t("closeZaloFirst") : "" }
      )
    );

    menuBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const wasOpen = state.activeMenuId === prof.id;
      closeActiveMenu(false);
      if (!wasOpen) {
        state.activeMenuId = prof.id;
        menuBtn.setAttribute("aria-expanded", "true");
        try {
          if (typeof menu.showPopover === "function") menu.showPopover();
          else menu.classList.add("is-open");
        } catch {
          menu.classList.add("is-open");
        }
        const firstItem = menu.querySelector('[role="menuitem"]');
        if (firstItem) firstItem.focus();
      }
    });

    menu.addEventListener("keydown", (e) => {
      const items = [...menu.querySelectorAll('[role="menuitem"]')];
      const idx = items.indexOf(document.activeElement);
      if (e.key === "Escape") {
        e.preventDefault();
        closeActiveMenu(true);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        items[(idx + 1) % items.length]?.focus();
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        items[(idx - 1 + items.length) % items.length]?.focus();
      } else if (e.key === "Home") {
        e.preventDefault();
        items[0]?.focus();
      } else if (e.key === "End") {
        e.preventDefault();
        items[items.length - 1]?.focus();
      } else if (e.key === "Tab") {
        closeActiveMenu(false);
      }
    });

    menuWrap.append(menuBtn, menu);
    footer.append(primaryBtn, menuWrap);
    card.appendChild(footer);
    wrap.appendChild(card);
    return wrap;
  });

  listEl.replaceChildren(...cards);

  // Primary Zalo read-only state
  const pzRow = document.getElementById("primary-zalo-state-row");
  const pzBadge = document.createElement("span");
  pzBadge.className = "status-badge";
  if (!state.hostConnected || state.primaryZalo.is_running === null) {
    pzBadge.dataset.state = "unknown";
    pzBadge.classList.add("is-unknown");
    pzBadge.append(
      createSvgIcon(["M12 12m-5 0a5 5 0 1 0 10 0a5 5 0 1 0 -10 0"], 12),
      document.createTextNode(t("primaryUnknown"))
    );
  } else if (state.primaryZalo.is_running) {
    pzBadge.dataset.state = "running";
    pzBadge.append(
      createSvgIcon(["M20 6 9 17l-5-5"], 12),
      document.createTextNode(t("primaryRunning"))
    );
  } else {
    pzBadge.dataset.state = "stopped";
    pzBadge.append(
      createSvgIcon(["M12 12m-5 0a5 5 0 1 0 10 0a5 5 0 1 0 -10 0"], 12),
      document.createTextNode(t("primaryStopped"))
    );
  }
  pzRow.replaceChildren(pzBadge);
}

function renderBackups() {
  const tbody = document.getElementById("backups-tbody");
  const wrap = document.getElementById("backups-table-wrap");
  const empty = document.getElementById("backups-empty");
  if (state.backups.length === 0) {
    wrap.hidden = true;
    empty.hidden = false;
    return;
  }
  wrap.hidden = false;
  empty.hidden = true;
  const rows = state.backups.map((b) => {
    const tr = document.createElement("tr");
    const tdName = document.createElement("td");
    tdName.textContent = b.profileName;
    const tdTime = document.createElement("td");
    tdTime.textContent = b.timestamp;
    const tdSize = document.createElement("td");
    tdSize.textContent = `${b.sizeMb} MB`;
    const tdAct = document.createElement("td");
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "btn";
    btn.textContent = t("restoreBtn");
    btn.addEventListener("click", () => openRestoreDialog(b, btn));
    tdAct.appendChild(btn);
    tr.append(tdName, tdTime, tdSize, tdAct);
    return tr;
  });
  tbody.replaceChildren(...rows);
}

function renderDiagnostics() {
  const readiness = [
    ["Zalo Desktop", state.zaloInstalled ? "PASS" : "BLOCKED", state.zaloInstalled ? "Detected" : "Not found"],
    ["Primary Zalo Guard", "PASS", "Read-only isolation active"],
  ];
  const isolation = [
    ["Profile Directory Isolation", "PASS", "Each space uses an independent data path"],
    ["Cross-Account Lock Check", "NOT_TESTED", "Run live check to verify running instances"],
  ];

  const fillTable = (id, rows) => {
    const tb = document.getElementById(id);
    if (!tb) return;
    const trs = rows.map(([name, status, detail]) => {
      const tr = document.createElement("tr");
      const td1 = document.createElement("td");
      td1.textContent = name;
      const td2 = document.createElement("td");
      const badge = document.createElement("span");
      badge.className = "status-badge";
      badge.dataset.state = status === "PASS" ? "running" : status === "BLOCKED" ? "error" : "recovery_required";
      const iconPaths =
        status === "PASS"
          ? ["M20 6 9 17l-5-5"]
          : status === "BLOCKED"
            ? ["M18 6 6 18", "M6 6l12 12"]
            : ["M12 9v4", "M12 17h.01", "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z"];
      badge.append(createSvgIcon(iconPaths, 12), document.createTextNode(status));
      td2.appendChild(badge);
      const td3 = document.createElement("td");
      td3.textContent = detail;
      tr.append(td1, td2, td3);
      return tr;
    });
    tb.replaceChildren(...trs);
  };

  fillTable("readiness-tbody", readiness);
  fillTable("diagnostics-tbody", isolation);

  const logEl = document.getElementById("sanitized-log-output");
  if (logEl) {
    logEl.textContent = [
      "[2026-11-19 09:30:01] INFO  KHOANG desktop shell initialized",
      "[2026-11-19 09:30:02] INFO  Primary Zalo guard verified (read-only)",
      `[2026-11-19 09:30:02] INFO  Loaded ${state.profiles.length} account spaces`,
    ].join("\n");
  }
}

function renderTopbarAndBanners() {
  const connPill = document.getElementById("connection-pill");
  const connLabel = document.getElementById("connection-label");
  connPill.dataset.state = state.hostConnected ? "connected" : "disconnected";
  connLabel.textContent = state.hostConnected ? t("ready") : t("disconnected");

  const banner = document.getElementById("status-banner");
  const bannerText = document.getElementById("status-banner-text");
  const bannerRaw = document.getElementById("status-banner-raw");
  if (state.bannerError) {
    banner.hidden = false;
    bannerText.textContent = state.bannerError.userMessage;
    bannerRaw.textContent = `${state.bannerError.rawCode}: ${state.bannerError.rawDetail}`;
  } else {
    banner.hidden = true;
  }

  const zaloAlert = document.getElementById("zalo-install-alert");
  zaloAlert.hidden = state.zaloInstalled;

  document.getElementById("btn-view-grid").setAttribute("aria-pressed", String(state.viewMode === "grid"));
  document.getElementById("btn-view-list").setAttribute("aria-pressed", String(state.viewMode === "list"));
}

function renderAll() {
  renderTopbarAndBanners();
  renderProfiles();
  renderBackups();
  renderDiagnostics();
}

function renderColorRadios(containerId, inputName, selectedIndex = 0) {
  const container = document.getElementById(containerId);
  if (!container) return;
  const nodes = ACCOUNT_COLORS.map((hex, idx) => {
    const label = document.createElement("label");
    label.className = "color-swatch-label";
    label.setAttribute("aria-label", `Màu ${idx + 1}`);
    const input = document.createElement("input");
    input.type = "radio";
    input.name = inputName;
    input.value = String(idx);
    input.checked = idx === selectedIndex;
    const dot = document.createElement("span");
    dot.className = "color-swatch-dot";
    dot.style.setProperty("--swatch-color", hex);
    label.append(input, dot);
    return label;
  });
  container.replaceChildren(...nodes);
}

function openCreateDialog(invoker) {
  state.lastInvoker = invoker || document.activeElement;
  const dlg = document.getElementById("create-profile-dialog");
  document.getElementById("create-step-1").hidden = false;
  document.getElementById("create-step-2").hidden = true;
  const nameInput = document.getElementById("create-profile-name");
  const notesInput = document.getElementById("create-profile-notes");
  const errEl = document.getElementById("create-step1-error");
  nameInput.value = "";
  notesInput.value = "";
  nameInput.removeAttribute("aria-invalid");
  nameInput.removeAttribute("aria-describedby");
  errEl.hidden = true;
  renderColorRadios("create-color-options", "create-account-color", 0);
  dlg.showModal();
}

function openRenameDialog(prof, invoker) {
  state.lastInvoker = invoker || document.activeElement;
  state.dialogTargetId = prof.id;
  const dlg = document.getElementById("rename-profile-dialog");
  document.getElementById("rename-profile-name").value = prof.name;
  renderColorRadios("rename-color-options", "rename-account-color", prof.colorIndex);
  dlg.showModal();
}

function openDeleteDialog(prof, invoker) {
  state.lastInvoker = invoker || document.activeElement;
  state.dialogTargetId = prof.id;
  const dlg = document.getElementById("delete-profile-dialog");
  document.getElementById("delete-profile-warning").textContent = t("deleteWarning", prof.name);
  dlg.showModal();
}

function openRestoreDialog(backup, invoker) {
  state.lastInvoker = invoker || document.activeElement;
  state.dialogBackup = backup;
  const dlg = document.getElementById("restore-backup-dialog");
  document.getElementById("restore-backup-subtitle").textContent = t(
    "restoreSubtitle",
    backup.profileName,
    backup.timestamp
  );
  document.getElementById("restore-profile-name").value = `${backup.profileName} (Khôi phục)`;
  dlg.showModal();
}

function openDetailsDialog(prof, invoker) {
  state.lastInvoker = invoker || document.activeElement;
  const dlg = document.getElementById("profile-details-dialog");
  const mainList = document.getElementById("profile-details-list");
  const advList = document.getElementById("profile-details-advanced-list");

  const addPair = (dl, k, v) => {
    const dt = document.createElement("dt");
    dt.textContent = k;
    const dd = document.createElement("dd");
    dd.textContent = v;
    dl.append(dt, dd);
  };
  mainList.replaceChildren();
  const stateLabelMap = {
    running: t("stateRunning"),
    stopped: t("stateStopped"),
    starting: t("stateStarting"),
    stopping: t("stateStopping"),
    recovery_required: t("stateRecovery"),
    error: t("stateError"),
  };
  addPair(mainList, t("accountNameLabel"), prof.name);
  addPair(mainList, t("detailStatus"), stateLabelMap[prof.state] || t("stateUnknown"));
  addPair(mainList, t("detailSize"), state.withSizes ? `${prof.sizeMb} MB` : "—");
  addPair(mainList, t("detailLastOpened"), prof.lastOpened);
  addPair(mainList, t("detailNotes"), prof.notes || "—");

  advList.replaceChildren();
  addPair(advList, t("detailId"), prof.id);
  addPair(advList, t("detailFolder"), prof.folder);
  dlg.showModal();
}

function applyDataset(ds) {
  state.dataset = ds;
  state.loading = false;
  state.bannerError = null;
  state.zaloInstalled = true;
  state.hostConnected = true;

  if (ds === "empty") {
    state.profiles = [];
  } else if (ds === "20") {
    state.profiles = build20Profiles();
  } else if (ds === "loading") {
    state.loading = true;
  } else if (ds === "error") {
    state.hostConnected = false;
    state.primaryZalo.is_running = null;
    state.bannerError = {
      userMessage: t("hostErrorMsg"),
      rawCode: "HOST_TIMEOUT",
      rawDetail: "GET /api/status timed out after 5000ms",
    };
  } else if (ds === "zalo-missing") {
    state.zaloInstalled = false;
    state.profiles = structuredClone(SAMPLE_PROFILES_3);
  } else {
    state.profiles = structuredClone(SAMPLE_PROFILES_3);
    state.primaryZalo.is_running = true;
  }
  const sel = document.getElementById("proto-dataset");
  if (sel) sel.value = ds;
  renderAll();
}

function setupEventListeners() {
  document.addEventListener("click", () => closeActiveMenu(false));

  document.getElementById("btn-brand-home").addEventListener("click", () => switchView("profiles"));
  for (const btn of document.querySelectorAll(".top-nav-btn[data-view]")) {
    btn.addEventListener("click", () => switchView(btn.dataset.view));
  }
  for (const btn of document.querySelectorAll(".back-link[data-back-to]")) {
    btn.addEventListener("click", () => switchView(btn.dataset.backTo));
  }
  document.getElementById("btn-goto-diagnostics").addEventListener("click", () => switchView("diagnostics"));
  document.getElementById("btn-help-diagnostics").addEventListener("click", () => switchView("diagnostics"));

  document.getElementById("profile-search").addEventListener("input", (e) => {
    state.searchQuery = e.target.value;
    renderProfiles();
  });

  document.getElementById("btn-view-grid").addEventListener("click", () => {
    state.viewMode = "grid";
    renderProfiles();
    renderTopbarAndBanners();
  });
  document.getElementById("btn-view-list").addEventListener("click", () => {
    state.viewMode = "list";
    renderProfiles();
    renderTopbarAndBanners();
  });

  document.getElementById("btn-stop-all").addEventListener("click", () => {
    for (const p of state.profiles) {
      if (p.state === "running") p.state = "stopped";
    }
    renderProfiles();
  });

  document.getElementById("btn-refresh").addEventListener("click", () => renderAll());
  document.getElementById("btn-refresh-backups").addEventListener("click", () => renderBackups());
  document.getElementById("btn-refresh-diagnostics").addEventListener("click", () => renderDiagnostics());
  document.getElementById("btn-run-isolation").addEventListener("click", () => renderDiagnostics());
  document.getElementById("btn-dismiss-banner").addEventListener("click", () => {
    state.bannerError = null;
    renderTopbarAndBanners();
  });

  document.getElementById("btn-open-create").addEventListener("click", (e) => openCreateDialog(e.currentTarget));
  document.getElementById("btn-empty-create").addEventListener("click", (e) => openCreateDialog(e.currentTarget));

  document.getElementById("btn-cancel-create").addEventListener("click", () => {
    document.getElementById("create-profile-dialog").close("cancel");
  });

  document.getElementById("create-profile-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const nameInput = document.getElementById("create-profile-name");
    const notesInput = document.getElementById("create-profile-notes");
    const errEl = document.getElementById("create-step1-error");
    const name = nameInput.value.trim();
    if (!name) {
      nameInput.setAttribute("aria-invalid", "true");
      nameInput.setAttribute("aria-describedby", "create-step1-error");
      errEl.textContent = t("createNameRequired");
      errEl.hidden = false;
      nameInput.focus();
      return;
    }
    const colorRadio = document.querySelector('input[name="create-account-color"]:checked');
    const colorIndex = colorRadio ? Number(colorRadio.value) : 0;
    const newProf = {
      id: `acc-${Date.now()}`,
      name,
      notes: notesInput.value.trim(),
      state: "stopped",
      colorIndex,
      sizeMb: 12,
      lastOpened: "—",
      folder: `Profiles/new_${Date.now()}`,
    };
    state.profiles.unshift(newProf);
    state.lastCreatedId = newProf.id;

    document.getElementById("create-step-1").hidden = true;
    document.getElementById("create-step-2").hidden = false;
    const step2Avatar = document.getElementById("create-step2-avatar");
    step2Avatar.textContent = name[0].toUpperCase();
    step2Avatar.style.setProperty("--avatar-color", ACCOUNT_COLORS[colorIndex]);
    document.getElementById("create-step2-title").textContent = t("createStep2Title", name);
    renderProfiles();
    document.getElementById("btn-create-launch").focus();
  });

  document.getElementById("btn-create-later").addEventListener("click", () => {
    document.getElementById("create-profile-dialog").close("later");
  });
  document.getElementById("btn-create-launch").addEventListener("click", () => {
    const prof = state.profiles.find((p) => p.id === state.lastCreatedId);
    if (prof) prof.state = "running";
    document.getElementById("create-profile-dialog").close("launch");
    renderProfiles();
  });

  // Dialog close handlers: reset returnValue & fallback light dismiss
  for (const dlg of document.querySelectorAll("dialog")) {
    dlg.addEventListener("close", () => {
      const rv = dlg.returnValue;
      dlg.returnValue = "";

      if (dlg.id === "rename-profile-dialog" && rv === "rename") {
        const prof = state.profiles.find((p) => p.id === state.dialogTargetId);
        const newName = document.getElementById("rename-profile-name").value.trim();
        const colorRadio = document.querySelector('input[name="rename-account-color"]:checked');
        if (prof && newName) {
          prof.name = newName;
          if (colorRadio) prof.colorIndex = Number(colorRadio.value);
          renderProfiles();
        }
      } else if (dlg.id === "delete-profile-dialog" && rv === "delete") {
        state.profiles = state.profiles.filter((p) => p.id !== state.dialogTargetId);
        renderProfiles();
        document.getElementById("main-content").focus();
        return;
      } else if (dlg.id === "restore-backup-dialog" && rv === "restore" && state.dialogBackup) {
        const newName = document.getElementById("restore-profile-name").value.trim();
        if (newName) {
          state.profiles.unshift({
            id: `acc-${Date.now()}`,
            name: newName,
            notes: "",
            state: "stopped",
            colorIndex: state.dialogBackup.colorIndex,
            sizeMb: state.dialogBackup.sizeMb,
            lastOpened: "—",
            folder: `Profiles/restored_${Date.now()}`,
          });
          switchView("profiles");
          showToast(t("toastRestored", newName));
        }
      }

      if (state.lastInvoker && state.lastInvoker.isConnected) {
        state.lastInvoker.focus();
      }
      state.lastInvoker = null;
    });

    if (!("closedBy" in HTMLDialogElement.prototype) && dlg.getAttribute("closedby") === "any") {
      const outside = (e) => {
        const r = dlg.getBoundingClientRect();
        return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
      };
      let pressedOutside = false;
      dlg.addEventListener("pointerdown", (e) => {
        pressedOutside = outside(e);
      });
      dlg.addEventListener("pointerup", (e) => {
        if (pressedOutside && outside(e)) dlg.close("");
      });
    }
  }

  for (const radio of document.querySelectorAll('input[name="ui-locale"]')) {
    radio.addEventListener("change", (e) => applyLocale(e.target.value));
  }
  for (const radio of document.querySelectorAll('input[name="ui-theme"]')) {
    radio.addEventListener("change", (e) => applyTheme(e.target.value));
  }
  document.getElementById("toggle-sizes").addEventListener("change", (e) => {
    state.withSizes = e.target.checked;
  });

  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (state.theme === "system") applyTheme("system");
  });

  // Prototype bar controls
  document.getElementById("proto-dataset").addEventListener("change", (e) => applyDataset(e.target.value));
  document.getElementById("proto-open-create").addEventListener("click", (e) => openCreateDialog(e.currentTarget));
  document.getElementById("proto-open-delete").addEventListener("click", (e) => {
    const target = state.profiles.find((p) => !isProfileActiveOrUnclean(p)) || state.profiles[0];
    if (target) openDeleteDialog(target, e.currentTarget);
  });
  document.getElementById("proto-open-details").addEventListener("click", (e) => {
    if (state.profiles[0]) openDetailsDialog(state.profiles[0], e.currentTarget);
  });
  document.getElementById("proto-hide").addEventListener("click", () => {
    document.getElementById("proto-bar").hidden = true;
  });
}

function initFromQueryParams() {
  const params = new URLSearchParams(window.location.search);
  const protoBar = document.getElementById("proto-bar");
  if (params.get("proto") === "0" && protoBar) protoBar.hidden = true;

  const lang = params.get("lang");
  const theme = params.get("theme");
  const mode = params.get("mode");
  const ds = params.get("state");
  const view = params.get("view");
  const dialog = params.get("dialog");

  if (mode === "list" || mode === "grid") state.viewMode = mode;
  if (ds) applyDataset(ds);
  if (theme === "light" || theme === "dark" || theme === "system") applyTheme(theme);
  else applyTheme("system");

  applyLocale(lang || "vi-VN");
  if (view) switchView(view);

  if (dialog === "create") openCreateDialog();
  else if (dialog === "rename" && state.profiles[0]) openRenameDialog(state.profiles[0]);
  else if (dialog === "delete" && state.profiles[0]) openDeleteDialog(state.profiles[0]);
  else if (dialog === "details" && state.profiles[0]) openDetailsDialog(state.profiles[0]);
  else if (dialog === "restore" && state.backups[0]) openRestoreDialog(state.backups[0]);
}

setupEventListeners();
initFromQueryParams();
