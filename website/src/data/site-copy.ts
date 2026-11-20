export type Lang = 'vi' | 'en';

export const SITE_COPY = {
  vi: {
    title: 'NexaDock | Nhiều tài khoản Zalo Desktop trong không gian tách biệt',
    description:
      'NexaDock là ứng dụng mã nguồn mở chạy nhiều tài khoản Zalo Desktop độc lập trên cùng một máy tính mà không chạm vào Zalo chính.',
    docsTitle: 'Tài liệu & Hướng dẫn cài đặt | NexaDock',
    docsDescription:
      'Hướng dẫn cài đặt NexaDock, kiểm tra SHA-256, xử lý Smart App Control / Gatekeeper, cơ chế cô lập thư mục và chẩn đoán sự cố.',
    nav: {
      experience: 'Giao diện',
      safety: 'Cô lập dữ liệu',
      platforms: 'Nền tảng',
      download: 'Tải xuống',
      docs: 'Tài liệu',
      cta: 'Tải bản thử nghiệm',
      menu: 'Menu'
    },
    hero: {
      title: 'Nhiều tài khoản. Một không gian tách biệt.',
      lead: 'Chạy nhiều phiên Zalo Desktop song song trên cùng một máy tính với thư mục dữ liệu, tiến trình và sao lưu riêng biệt, giữ nguyên Zalo chính ở chế độ chỉ đọc.',
      primaryCta: 'Tải NexaDock',
      secondaryCta: 'Xem mã nguồn trên GitHub',
      meta: 'Mã nguồn mở MIT / Apache-2.0 — bản thử nghiệm chưa ký số — không thu thập mật khẩu hay tin nhắn.',
      screenshotAlt: 'Giao diện ứng dụng NexaDock hiển thị 3 tài khoản Zalo tách biệt và trạng thái bảo vệ Zalo chính',
      screenshotCaption: 'Ảnh chụp trực tiếp từ ứng dụng desktop NexaDock (chế độ sáng, tiếng Việt)'
    },
    showcase: {
      title: 'Mô phỏng luồng chuyển đổi & cô lập tài khoản',
      desc: 'Chọn từng không gian bên trái để xem cách NexaDock tách biến môi trường và bảo vệ Zalo chính bằng dữ liệu mẫu.',
      note: 'Dữ liệu minh họa cục bộ (không kết nối mạng hay Zalo thật).',
      accounts: [
        {
          id: 'primary',
          initials: 'Z',
          name: 'Zalo đang dùng trên máy',
          role: 'Được bảo vệ, chỉ đọc',
          state: 'Đang chạy độc lập',
          stateKind: 'neutral',
          path: '%APPDATA%\\ZaloData',
          detail:
            'NexaDock chỉ quan sát trạng thái tiến trình chính để tránh xung đột. Mọi lệnh dừng, ghi đè hoặc xóa nhắm vào Zalo gốc đều bị chặn ở tầng Rust.'
        },
        {
          id: 'work',
          initials: 'ĐN',
          name: 'Đức: Công việc & Đối tác',
          role: 'Không gian tách biệt',
          state: 'Cửa sổ đang mở',
          stateKind: 'ok',
          path: '%LOCALAPPDATA%\\NexaDock\\Profiles\\duc-cong-viec_176ea8dd',
          detail:
            'Chạy bằng bộ biến môi trường APPDATA, LOCALAPPDATA và TEMP riêng. Đưa cửa sổ lên trên hoặc đóng cây tiến trình an toàn mà không chạm tài khoản khác.'
        },
        {
          id: 'personal',
          initials: 'CN',
          name: 'Cá nhân',
          role: 'Không gian tách biệt',
          state: 'Đã đóng',
          stateKind: 'neutral',
          path: '%LOCALAPPDATA%\\NexaDock\\Profiles\\ca-nhan_2492ef1d',
          detail:
            'Sẵn sàng sao lưu nguyên tử sang tệp .zip có kiểm tra SHA-256 và ngưỡng dung lượng trống tối thiểu 5 GiB trước khi thực hiện.'
        },
        {
          id: 'sales',
          initials: 'BH',
          name: 'Bán hàng & CSKH',
          role: 'Không gian tách biệt',
          state: 'Cửa sổ đang mở',
          stateKind: 'ok',
          path: '%LOCALAPPDATA%\\NexaDock\\Profiles\\ban-hang_52fb0a78',
          detail:
            'Kiểm tra trùng zalo_user_id và khóa thư mục cơ sở dữ liệu ngoài vùng lưu trữ trước khi khởi chạy để tránh hỏng dữ liệu.'
        }
      ]
    },
    shots: {
      title: 'Giao diện thực tế trên desktop',
      desc: 'Ảnh chụp từ bản dựng NexaDock trên chế độ tối, chế độ Desktop Stage, quản lý sao lưu và kiểm tra chẩn đoán.',
      items: [
        {
          file: 'nexadock-vi-vn-dark.png',
          alt: 'Giao diện NexaDock chế độ tối với danh sách tài khoản tách biệt',
          caption: 'Chế độ tối (Dark mode) — Quản lý nhiều tài khoản song song'
        },
        {
          file: 'nexadock-desktop-stage-vi-light.png',
          alt: 'Màn hình Desktop Stage quản lý cửa sổ và tiến trình tài khoản',
          caption: 'Desktop Stage — Theo dõi trạng thái cửa sổ và thao tác từng phiên'
        },
        {
          file: 'nexadock-backups-vi-light.png',
          alt: 'Màn hình sao lưu và khôi phục nguyên tử có kiểm tra SHA-256',
          caption: 'Sao lưu & Khôi phục — Kiểm tra dung lượng trống 5 GiB và mã băm SHA-256'
        },
        {
          file: 'nexadock-diagnostics-vi-light.png',
          alt: 'Màn hình chẩn đoán và xác minh rào chắn bảo vệ Zalo chính',
          caption: 'Chẩn đoán (Diagnostics) — Kiểm tra chữ ký Zalo và rào chắn chỉ đọc'
        }
      ]
    },
    isolation: {
      title: 'Kiến trúc cô lập & bảo vệ Zalo chính',
      desc: 'Thay vì sửa tệp thực thi của Zalo hay giả lập giao thức, NexaDock cô lập môi trường chạy ở cấp hệ điều hành.',
      items: [
        {
          name: 'Zalo chính luôn ở chế độ chỉ đọc',
          body: 'Đường dẫn %APPDATA%\\ZaloData và tiến trình Zalo chính không bao giờ bị dừng, ghi đè hoặc xóa. Mọi yêu cầu tác động đều bị chặn với mã PROTECTED_PRIMARY_PROCESS hoặc PROTECTED_PRIMARY_PATH.'
        },
        {
          name: 'Tách thư mục, bộ nhớ đệm và xác thực chữ ký',
          body: 'Mỗi tài khoản phụ chạy trong thư mục riêng dưới Profiles/<slug>_<id>/ với APPDATA, LOCALAPPDATA và TEMP độc lập. Trước khi khởi chạy, NexaDock xác thực chữ ký Zalo chính thức (WinVerifyTrust trên Windows hoặc chữ ký bundle trên macOS).'
        },
        {
          name: 'Sao lưu nguyên tử và khôi phục không ghi đè',
          body: 'Sao lưu tạo gói .zip kèm mã băm SHA-256 sau khi kiểm tra tối thiểu 5 GiB trống. Khi khôi phục luôn tạo một không gian tài khoản mới, không ghi đè dữ liệu hiện có.'
        }
      ]
    },
    compat: {
      title: 'Ma trận hỗ trợ nền tảng (trạng thái thực tế)',
      desc: 'Trình bày trung thực mức độ kiểm thử tự động, tình trạng ký số và các nền tảng đang ở giai đoạn thử nghiệm.',
      headers: ['Nền tảng & Kiến trúc', 'Zalo Desktop gốc', 'Trạng thái NexaDock', 'Ghi chú kiểm chứng & Ký số'],
      rows: [
        {
          os: 'Windows 10 / 11 (x64)',
          zalo: 'Có (Zalo.exe)',
          status: 'Beta (Chưa ký số — L1/L2 đạt)',
          kind: 'ok',
          note: 'Đã đóng gói .exe và .msi. Đạt kiểm thử tự động L1/L2; chờ nghiệm thu quét QR 2 tài khoản thật (L3) và chưa có chữ ký số Authenticode.'
        },
        {
          os: 'Windows 11 Smart App Control (SAC=1)',
          zalo: 'Có (Zalo.exe)',
          status: 'Hỗ trợ qua Signed-Host Bridge',
          kind: 'ok',
          note: 'Không cần tắt Smart App Control: chạy giao diện qua Node.js và PowerShell đã ký số của hệ thống từ mã nguồn.'
        },
        {
          os: 'macOS Apple Silicon (arm64)',
          zalo: 'Có (/Applications/Zalo.app)',
          status: 'Chưa ký số / Chưa Notarized',
          kind: 'warn',
          note: 'Đã đóng gói .dmg và đạt build/fixture CI; chưa ký Developer ID/Notarized (chờ cấu hình Apple secrets) và chưa test đăng nhập Zalo.app thật. Chưa hỗ trợ macOS Intel.'
        },
        {
          os: 'Linux (Ubuntu 24.04 x64)',
          zalo: 'Không có bản Linux từ VNG',
          status: 'Thử nghiệm (Manager / Fixture)',
          kind: 'neutral',
          note: 'Đóng gói .AppImage và .deb để kiểm thử NexaDock Manager hoặc fixture. VNG không phát hành Zalo Desktop chính thức cho Linux.'
        }
      ]
    },
    download: {
      title: 'Tải NexaDock',
      prereleaseBadge: 'Pre-release chưa ký số',
      verifyTitle: 'Kiểm tra toàn vẹn SHA-256 trước khi chạy',
      verifyBody: 'Bản thử nghiệm chưa có chữ ký số thương mại (Authenticode / Apple Notarization). Luôn đối chiếu mã băm với tệp SHA256SUMS.txt đi kèm bản phát hành.',
      missingIntel: 'Bản phát hành thử nghiệm này chưa bao gồm bộ cài macOS Intel (x86_64).',
      linuxNote: 'Ghi chú: VNG không phát hành Zalo Desktop cho Linux. Các gói này chỉ phục vụ thử nghiệm Manager hoặc môi trường fixture.'
    },
    faq: {
      title: 'Câu hỏi thường gặp',
      items: [
        {
          q: 'NexaDock có phải sản phẩm chính thức của VNG hay Zalo không?',
          a: 'Không. NexaDock là dự án mã nguồn mở độc lập (MIT OR Apache-2.0), không liên kết hay được chứng thực bởi VNG Corporation. Người dùng tự cài đặt Zalo Desktop chính thức từ VNG.'
        },
        {
          q: 'NexaDock có đọc tin nhắn, mật khẩu hoặc mã QR của tôi không?',
          a: 'Không. Bạn đăng nhập trực tiếp trong cửa sổ Zalo Desktop chính thức. NexaDock chỉ quản lý thư mục lưu trữ tách biệt và vòng đời tiến trình trên máy cục bộ.'
        },
        {
          q: 'Có thể gọi thoại/video ZaloCall ở 2 tài khoản cùng một lúc không?',
          a: 'Chưa. Bản Zalo Desktop của VNG dùng chung tên kênh hệ thống cho ZaloCall. Bạn có thể gọi tuần tự từng tài khoản bình thường, nhưng không gọi đồng thời trên hai cửa sổ.'
        },
        {
          q: 'Tại sao Windows SmartScreen hoặc macOS Gatekeeper cảnh báo khi mở bộ cài?',
          a: 'Vì bản pre-release chưa được ký số thương mại (Authenticode / Apple Developer ID). Hãy kiểm tra SHA-256 khớp với SHA256SUMS.txt, hoặc dùng Signed-Host Bridge từ mã nguồn nếu máy bật Smart App Control.'
        }
      ]
    },
    footer: {
      disclaimer:
        'NexaDock là phần mềm mã nguồn mở độc lập, không liên kết với VNG Corporation hay Zalo. Nhãn hiệu “Zalo” thuộc sở hữu của VNG Corporation.',
      projectHeading: 'Dự án',
      resourcesHeading: 'Tài nguyên'
    }
  },
  en: {
    title: 'NexaDock | Multiple Zalo Desktop accounts in isolated workspaces',
    description:
      'NexaDock is an open-source desktop utility to run multiple isolated Zalo Desktop accounts side-by-side without modifying your primary Zalo installation.',
    docsTitle: 'Documentation & Installation Guide | NexaDock',
    docsDescription:
      'Install NexaDock, verify SHA-256 checksums, configure Smart App Control or Gatekeeper, understand profile isolation, and run diagnostics.',
    nav: {
      experience: 'Interface',
      safety: 'Isolation',
      platforms: 'Platforms',
      download: 'Download',
      docs: 'Docs',
      cta: 'Download Pre-release',
      menu: 'Menu'
    },
    hero: {
      title: 'Multiple accounts. One isolated workspace.',
      lead: 'Run multiple Zalo Desktop sessions side-by-side on one machine with separate profile directories, processes, and backups while keeping your primary Zalo strictly read-only.',
      primaryCta: 'Download NexaDock',
      secondaryCta: 'View Source on GitHub',
      meta: 'Open Source MIT / Apache-2.0 — unsigned pre-release — zero credentials collected.',
      screenshotAlt: 'NexaDock desktop app showing 3 isolated Zalo accounts and read-only primary Zalo protection',
      screenshotCaption: 'Direct capture from NexaDock desktop app (Light mode, English)'
    },
    showcase: {
      title: 'Interactive workspace & isolation preview',
      desc: 'Select a workspace on the left to inspect how NexaDock isolates environment paths and protects your primary Zalo using synthetic sample data.',
      note: 'Local sample data (no live network or Zalo connection).',
      accounts: [
        {
          id: 'primary',
          initials: 'Z',
          name: 'Primary Zalo on this machine',
          role: 'Protected, read-only',
          state: 'Running independently',
          stateKind: 'neutral',
          path: '%APPDATA%\\ZaloData',
          detail:
            'NexaDock only observes your primary Zalo state to prevent conflicts. Any stop, overwrite, or delete targeting the primary client is blocked in Rust.'
        },
        {
          id: 'work',
          initials: 'ĐN',
          name: 'Duc: Work & Partners',
          role: 'Isolated space',
          state: 'Window open',
          stateKind: 'ok',
          path: '%LOCALAPPDATA%\\NexaDock\\Profiles\\duc-cong-viec_176ea8dd',
          detail:
            'Runs with dedicated APPDATA, LOCALAPPDATA, and TEMP directories. Bring its window to front or close its process tree cleanly without affecting siblings.'
        },
        {
          id: 'personal',
          initials: 'CN',
          name: 'Personal',
          role: 'Isolated space',
          state: 'Closed',
          stateKind: 'neutral',
          path: '%LOCALAPPDATA%\\NexaDock\\Profiles\\ca-nhan_2492ef1d',
          detail:
            'Ready for atomic .zip backup with SHA-256 verification and a mandatory 5 GiB free-space safety check before writing.'
        },
        {
          id: 'sales',
          initials: 'BH',
          name: 'Sales & Support',
          role: 'Isolated space',
          state: 'Window open',
          stateKind: 'ok',
          path: '%LOCALAPPDATA%\\NexaDock\\Profiles\\ban-hang_52fb0a78',
          detail:
            'Enforces duplicate zalo_user_id detection and external database path guards before launch to prevent cross-profile corruption.'
        }
      ]
    },
    shots: {
      title: 'Actual desktop application views',
      desc: 'Direct captures from NexaDock across dark mode, Desktop Stage, atomic backups, and isolation diagnostics.',
      items: [
        {
          file: 'nexadock-vi-vn-dark.png',
          alt: 'NexaDock dark mode workspace with isolated account list',
          caption: 'Dark mode — Manage multiple isolated accounts side-by-side'
        },
        {
          file: 'nexadock-desktop-stage-vi-light.png',
          alt: 'NexaDock Desktop Stage view for window focus and process control',
          caption: 'Desktop Stage — Inspect window state and control individual sessions'
        },
        {
          file: 'nexadock-backups-vi-light.png',
          alt: 'NexaDock atomic backup and restore screen with SHA-256 verification',
          caption: 'Backups & Restore — Enforces 5 GiB free-space check and SHA-256 integrity'
        },
        {
          file: 'nexadock-diagnostics-vi-light.png',
          alt: 'NexaDock Diagnostics screen verifying client signature and read-only guards',
          caption: 'Diagnostics — Verify official client signature and read-only protection'
        }
      ]
    },
    isolation: {
      title: 'Isolation & primary-Zalo protection',
      desc: 'Instead of patching Zalo binaries or emulating unofficial protocols, NexaDock isolates runtime directories and processes at the OS level.',
      items: [
        {
          name: 'Primary Zalo stays strictly read-only',
          body: 'Your existing %APPDATA%\\ZaloData directory and primary process are never stopped, overwritten, or deleted. Requests targeting them fail closed with PROTECTED_PRIMARY_PROCESS or PROTECTED_PRIMARY_PATH.'
        },
        {
          name: 'Separate profile directories, caches, and signature checks',
          body: 'Each secondary account lives under Profiles/<slug>_<id>/ with dedicated APPDATA, LOCALAPPDATA, and TEMP paths. Before launch, NexaDock verifies the official client signature (WinVerifyTrust on Windows or bundle signature on macOS).'
        },
        {
          name: 'Atomic SHA-256 backups and non-destructive restore',
          body: 'Backups require at least 5 GiB free space and write SHA-256 verified archives. Restoring always creates a brand-new profile instead of overwriting existing data.'
        }
      ]
    },
    compat: {
      title: 'Platform support matrix (honest status)',
      desc: 'Transparent breakdown of automated verification, code-signing status, and experimental targets.',
      headers: ['Platform & Arch', 'Official Zalo Client', 'NexaDock Status', 'Verification & Signing Notes'],
      rows: [
        {
          os: 'Windows 10 / 11 (x64)',
          zalo: 'Yes (Zalo.exe)',
          status: 'Unsigned Beta (L1/L2 Verified)',
          kind: 'ok',
          note: 'NSIS .exe and .msi installers published. Passes L1/L2 automated checks; live L3 2-account QR login awaits end-user verification and binaries are unsigned.'
        },
        {
          os: 'Windows 11 Smart App Control (SAC=1)',
          zalo: 'Yes (Zalo.exe)',
          status: 'Supported via Signed-Host Bridge',
          kind: 'ok',
          note: 'Do not disable Smart App Control: launch the UI through system-signed Node.js and PowerShell from source.'
        },
        {
          os: 'macOS Apple Silicon (arm64)',
          zalo: 'Yes (/Applications/Zalo.app)',
          status: 'Unsigned until Apple secrets configured',
          kind: 'warn',
          note: 'Packaged .dmg passes fixture and native CI builds; unsigned/unnotarized until Apple Developer secrets are configured, and live Zalo.app login is not yet tested. Intel macOS is omitted.'
        },
        {
          os: 'Linux (Ubuntu 24.04 x64)',
          zalo: 'No official Linux client',
          status: 'Experimental (Manager / Fixture)',
          kind: 'neutral',
          note: 'Publishes .AppImage and .deb for testing NexaDock Manager or synthetic fixtures. VNG does not ship Zalo Desktop for Linux.'
        }
      ]
    },
    download: {
      title: 'Download NexaDock',
      prereleaseBadge: 'Unsigned Development Pre-release',
      verifyTitle: 'Verify SHA-256 checksums before running',
      verifyBody: 'Pre-release installers do not yet carry commercial Authenticode or Apple Notarization signatures. Always verify checksums against SHA256SUMS.txt.',
      missingIntel: 'This pre-release does not include a macOS Intel (x86_64) installer.',
      linuxNote: 'Note: VNG does not ship Zalo Desktop for Linux. These packages are experimental for Manager and fixture testing.'
    },
    faq: {
      title: 'Frequently asked questions',
      items: [
        {
          q: 'Is NexaDock affiliated with VNG or Zalo?',
          a: 'No. NexaDock is an independent open-source project (MIT OR Apache-2.0) and is not affiliated with, sponsored by, or endorsed by VNG Corporation. Users install official Zalo Desktop directly from VNG.'
        },
        {
          q: 'Does NexaDock read passwords, OTP codes, or messages?',
          a: 'No. You sign in directly inside the official Zalo Desktop window. NexaDock only manages isolated local storage directories and process lifecycles.'
        },
        {
          q: 'Can two accounts run ZaloCall voice/video calls simultaneously?',
          a: 'No. The vendor Zalo Desktop client binds global IPC channels for ZaloCall. Sequential calls work normally, while simultaneous calls across instances are unsupported by the client.'
        },
        {
          q: 'Why do Windows SmartScreen or macOS Gatekeeper show a warning?',
          a: 'Because these pre-release artifacts are unsigned development builds. Verify the file hash against SHA256SUMS.txt, or use the Signed-Host Bridge from source on Smart App Control machines.'
        }
      ]
    },
    footer: {
      disclaimer:
        'NexaDock is an independent open-source utility and is not affiliated with VNG Corporation or Zalo. “Zalo” is a trademark of VNG Corporation.',
      projectHeading: 'Project',
      resourcesHeading: 'Resources'
    }
  }
} as const;
