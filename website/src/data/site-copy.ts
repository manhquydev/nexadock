export type Lang = 'vi' | 'en';

export const SITE_COPY = {
  vi: {
    title: 'NexaDock | Nhiều tài khoản Zalo Desktop trong một không gian tách biệt',
    description:
      'NexaDock là ứng dụng mã nguồn mở chạy nhiều tài khoản Zalo Desktop độc lập trên cùng một máy tính mà không chạm vào Zalo chính.',
    docsTitle: 'Tài liệu & Hướng dẫn cài đặt | NexaDock',
    docsDescription:
      'Hướng dẫn cài đặt NexaDock, kiểm tra SHA-256, nâng cấp từ KHOANG không mất dữ liệu và xử lý Smart App Control / Gatekeeper.',
    nav: {
      experience: 'Trải nghiệm',
      safety: 'An toàn dữ liệu',
      download: 'Tải xuống',
      docs: 'Tài liệu',
      cta: 'Tải bản thử nghiệm',
      menu: 'Menu'
    },
    hero: {
      title: 'Nhiều tài khoản. Một không gian làm việc.',
      lead: 'Mở nhiều tài khoản Zalo Desktop song song trên cùng một máy tính với thư mục dữ liệu, tiến trình và sao lưu tách biệt, giữ nguyên Zalo chính ở chế độ chỉ đọc.',
      primaryCta: 'Tải NexaDock',
      secondaryCta: 'Xem mã nguồn trên GitHub',
      meta: 'Mã nguồn mở MIT / Apache-2.0, bản thử nghiệm chưa ký số, không thu thập mật khẩu hay tin nhắn.'
    },
    showcase: {
      title: 'Xem trước luồng chuyển tài khoản',
      note: 'Mô phỏng giao diện NexaDock với dữ liệu mẫu (không kết nối mạng hay Zalo thật).',
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
          path: '%LOCALAPPDATA%\\MultiZaloDesktop\\Profiles\\duc-cong-viec_176ea8dd',
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
          path: '%LOCALAPPDATA%\\MultiZaloDesktop\\Profiles\\ca-nhan_2492ef1d',
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
          path: '%LOCALAPPDATA%\\MultiZaloDesktop\\Profiles\\ban-hang_52fb0a78',
          detail:
            'Kiểm tra trùng zalo_user_id và khóa thư mục cơ sở dữ liệu ngoài vùng lưu trữ trước khi khởi chạy để tránh hỏng dữ liệu.'
        }
      ]
    },
    shots: {
      title: 'Giao diện thực tế của ứng dụng desktop',
      desc: 'Ảnh chụp trực tiếp từ bản dựng NexaDock trên giao diện sáng, tối, màn hình quản lý tài khoản và chẩn đoán tách biệt dữ liệu.'
    },
    isolation: {
      title: 'Cách NexaDock tách biệt dữ liệu',
      desc: 'Thay vì sửa tệp thực thi của Zalo hay giả lập giao thức, NexaDock cô lập môi trường chạy ở cấp hệ điều hành.',
      items: [
        {
          name: 'Bảo vệ Zalo chính ở chế độ chỉ đọc',
          body: 'Đường dẫn %APPDATA%\\ZaloData và tiến trình Zalo chính được đánh dấu chỉ đọc. Mọi yêu cầu dừng, xóa hoặc ghi đè đều bị chặn với mã PROTECTED_PRIMARY_PROCESS và PROTECTED_PRIMARY_PATH.'
        },
        {
          name: 'Tách thư mục, bộ nhớ đệm và khóa chạy',
          body: 'Mỗi tài khoản phụ nằm trong thư mục riêng dưới Profiles/<slug>_<id>/. Trước khi khởi chạy, ứng dụng xác thực chữ ký Zalo chính thức (WinVerifyTrust trên Windows hoặc chữ ký bundle trên macOS).'
        },
        {
          name: 'Sao lưu nguyên tử và khôi phục không ghi đè',
          body: 'Sao lưu tạo gói .zip kèm mã băm SHA-256 sau khi kiểm tra tối thiểu 5 GiB trống. Khi khôi phục luôn tạo thành một không gian tài khoản mới, không ghi đè dữ liệu hiện có.'
        }
      ]
    },
    compat: {
      title: 'Hỗ trợ nền tảng và mức độ kiểm chứng',
      desc: 'Bảng dưới đây nêu rõ nền tảng đã kiểm chứng qua bộ test tự động, nền tảng thử nghiệm và phần chưa nghiệm thu với tài khoản thật.',
      headers: ['Nền tảng', 'Zalo Desktop gốc', 'Trạng thái NexaDock', 'Ghi chú trung thực'],
      rows: [
        {
          os: 'Windows 10 / 11 (x64)',
          zalo: 'Có (Zalo.exe)',
          status: 'Hoạt động (L1/L2 đã kiểm thử)',
          kind: 'ok',
          note: 'Đã đóng gói .exe và .msi. Chờ kiểm thử quét QR 2 tài khoản thật (L3) ở máy người dùng; bản dựng chưa ký số Authenticode.'
        },
        {
          os: 'Windows 11 Smart App Control (SAC=1)',
          zalo: 'Có (Zalo.exe)',
          status: 'Hỗ trợ qua Signed-Host Bridge',
          kind: 'ok',
          note: 'Không cần tắt Smart App Control: chạy giao diện qua Node.js và PowerShell đã ký số của hệ thống.'
        },
        {
          os: 'macOS Apple Silicon (arm64)',
          zalo: 'Có (/Applications/Zalo.app)',
          status: 'Đã đóng gói .dmg (Chờ test Zalo thật)',
          kind: 'warn',
          note: 'Đạt kiểm thử fixture và build native; chưa kiểm thử đăng nhập tài khoản thật trên Zalo.app và chưa Notarized. Bản macOS Intel chưa phát hành ở bản beta này.'
        },
        {
          os: 'Ubuntu 24.04 LTS (x64)',
          zalo: 'Không có bản Linux từ VNG',
          status: 'Thử nghiệm (Manager / Fixture)',
          kind: 'neutral',
          note: 'Chỉ dùng để kiểm thử NexaDock Manager, fixture hoặc AppImage cục bộ tự cung cấp. Không có Zalo Desktop chính thức cho Linux.'
        }
      ]
    },
    download: {
      title: 'Tải NexaDock',
      prereleaseBadge: 'Bản thử nghiệm Pre-release (UNSIGNED_DEVELOPMENT_ARTIFACT)',
      upgradeNote:
        'Nâng cấp từ KHOANG hoặc Multi-Zalo Desktop: NexaDock giữ nguyên mã định danh com.multizalodesktop.manager, thư mục %LOCALAPPDATA%\\MultiZaloDesktop và tệp khoang-preferences.json nên toàn bộ tài khoản đã lưu được giữ nguyên.',
      verifyTitle: 'Kiểm tra toàn vẹn SHA-256 trước khi cài đặt',
      verifyBody: 'Bản thử nghiệm chưa có chữ ký số EV/Authenticode hay Apple Notarization. Luôn đối chiếu mã băm với tệp SHA256SUMS.txt đi kèm bản phát hành.',
      missingIntel: 'Bản phát hành thử nghiệm này chưa bao gồm bộ cài macOS Intel (x86_64).'
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
          q: 'Dữ liệu cũ từ KHOANG có bị mất khi chuyển sang NexaDock không?',
          a: 'Không. Toàn bộ đường dẫn lưu trữ dữ liệu, cấu hình và hồ sơ cũ đều được giữ nguyên để nâng cấp an toàn.'
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
      'Install NexaDock, verify SHA-256 checksums, upgrade from KHOANG with zero data loss, and handle Smart App Control or Gatekeeper.',
    nav: {
      experience: 'Experience',
      safety: 'Data Safety',
      download: 'Download',
      docs: 'Docs',
      cta: 'Download Pre-release',
      menu: 'Menu'
    },
    hero: {
      title: 'Your accounts. Their own isolated space.',
      lead: 'Run multiple Zalo Desktop sessions side-by-side on one machine with separate profile directories, processes, and backups while keeping your primary Zalo strictly read-only.',
      primaryCta: 'Download NexaDock',
      secondaryCta: 'View Source on GitHub',
      meta: 'Open Source MIT / Apache-2.0, unsigned pre-release, zero credentials collected.'
    },
    showcase: {
      title: 'Interactive workspace switcher preview',
      note: 'Simulated NexaDock UI with synthetic demo accounts (no live network or Zalo connection).',
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
          path: '%LOCALAPPDATA%\\MultiZaloDesktop\\Profiles\\duc-cong-viec_176ea8dd',
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
          path: '%LOCALAPPDATA%\\MultiZaloDesktop\\Profiles\\ca-nhan_2492ef1d',
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
          path: '%LOCALAPPDATA%\\MultiZaloDesktop\\Profiles\\ban-hang_52fb0a78',
          detail:
            'Enforces duplicate zalo_user_id detection and external database path guards before launch to prevent cross-profile corruption.'
        }
      ]
    },
    shots: {
      title: 'Actual desktop application interface',
      desc: 'Direct captures from NexaDock across light mode, dark mode, account management stage, and isolation diagnostics.'
    },
    isolation: {
      title: 'How NexaDock isolates account data',
      desc: 'Instead of patching Zalo binaries or emulating unofficial protocols, NexaDock isolates runtime directories and processes at the OS level.',
      items: [
        {
          name: 'Primary Zalo stays strictly read-only',
          body: 'Your existing %APPDATA%\\ZaloData directory and primary process are never stopped, overwritten, or deleted. Requests targeting them fail closed with PROTECTED_PRIMARY_PROCESS or PROTECTED_PRIMARY_PATH.'
        },
        {
          name: 'Separate profile directories, caches, and locks',
          body: 'Each secondary account lives under Profiles/<slug>_<id>/. Before launch, NexaDock verifies the official client signature (WinVerifyTrust on Windows or bundle signature on macOS).'
        },
        {
          name: 'Atomic SHA-256 backups and non-destructive restore',
          body: 'Backups require at least 5 GiB free space and write SHA-256 verified archives. Restoring always creates a brand-new profile instead of overwriting existing data.'
        }
      ]
    },
    compat: {
      title: 'Platform support and verification status',
      desc: 'Honest breakdown of automated verification, experimental targets, and platforms awaiting live end-user QR acceptance.',
      headers: ['Platform', 'Official Zalo Client', 'NexaDock Status', 'Verification Notes'],
      rows: [
        {
          os: 'Windows 10 / 11 (x64)',
          zalo: 'Yes (Zalo.exe)',
          status: 'Working (L1/L2 Verified)',
          kind: 'ok',
          note: 'NSIS .exe and .msi installers published. Interactive L3 2-account QR login awaits end-user verification; binaries are unsigned.'
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
          status: 'Packaged .dmg (Live Zalo Not Tested)',
          kind: 'warn',
          note: 'Passes fixture and native CI builds; live multi-account Zalo.app login is not yet tested and .dmg is unnotarized. Intel macOS is omitted from this beta.'
        },
        {
          os: 'Ubuntu 24.04 LTS (x64)',
          zalo: 'No official Linux client',
          status: 'Experimental (Manager / Fixture)',
          kind: 'neutral',
          note: 'For testing NexaDock Manager, synthetic fixtures, or user-supplied AppImages only. VNG does not ship Zalo Desktop for Linux.'
        }
      ]
    },
    download: {
      title: 'Download NexaDock',
      prereleaseBadge: 'Development Pre-release (UNSIGNED_DEVELOPMENT_ARTIFACT)',
      upgradeNote:
        'Upgrading from KHOANG or Multi-Zalo Desktop: NexaDock keeps com.multizalodesktop.manager, %LOCALAPPDATA%\\MultiZaloDesktop, and khoang-preferences.json unchanged so existing accounts and sessions stay intact.',
      verifyTitle: 'Verify SHA-256 checksums before running',
      verifyBody: 'Pre-release installers do not yet carry commercial Authenticode or Apple Notarization signatures. Always verify checksums against SHA256SUMS.txt.',
      missingIntel: 'This pre-release does not include a macOS Intel (x86_64) installer.'
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
          q: 'Will upgrading from KHOANG erase my existing accounts?',
          a: 'No. Storage paths, bundle identifiers, preferences, and existing isolated profiles remain unchanged for seamless upgrades.'
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
