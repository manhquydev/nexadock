import type { Lang } from './site-copy';

export type GuideOsId = 'macos' | 'windows' | 'ubuntu';

export type GuideVisualId =
  | 'mac-chip'
  | 'mac-dmg-drag'
  | 'mac-applications'
  | 'mac-gatekeeper'
  | 'mac-zalo-qr'
  | 'win-installer'
  | 'win-smartscreen'
  | 'win-sac'
  | 'win-zalo-qr'
  | 'linux-pkg'
  | 'linux-note';

export interface GuideStep {
  id: string;
  title: string;
  shortTitle: string;
  summary: string;
  checklist: string[];
  tip?: string;
  optionalCmdLabel?: string;
  optionalCmd?: string;
  links?: { label: string; href: string }[];
  visual: GuideVisualId;
}

export interface GuideOs {
  id: GuideOsId;
  label: string;
  badge: string;
  badgeKind: 'ok' | 'warn' | 'neutral';
  steps: GuideStep[];
}

export interface InstallGuideCopy {
  figTag: string;
  heading: string;
  subtitle: string;
  simBadge: string;
  osTabAria: string;
  stepListAria: string;
  prevLabel: string;
  nextLabel: string;
  stepPrefix: string;
  copyLabel: string;
  copiedLabel: string;
  platforms: GuideOs[];
  visuals: {
    macChipTitle: string;
    macChipApple: string;
    macChipOk: string;
    macChipIntel: string;
    macChipNo: string;
    macDmgDragHint: string;
    macAppsHint: string;
    macSpotlightHint: string;
    macDialogTitle: string;
    macDialogBody: string;
    macDialogBtn: string;
    macSettingsTitle: string;
    macSettingsRow: string;
    macSettingsBtn: string;
    winSmartTitle: string;
    winSmartBody: string;
    winSmartMore: string;
    winSmartRun: string;
    primaryLabel: string;
    primaryState: string;
    secondaryLabel: string;
    secondaryAction: string;
  };
}

export const INSTALL_GUIDE_COPY: Record<Lang, InstallGuideCopy> = {
  vi: {
    figTag: 'FIG. 05',
    heading: 'Hướng dẫn cài đặt từng bước (có minh họa trực quan)',
    subtitle:
      'Chọn hệ điều hành bạn đang dùng và bấm qua từng bước để xem đúng màn hình sẽ hiện ra — không cần kiến thức kỹ thuật.',
    simBadge: 'HƯỚNG DẪN TƯƠNG TÁC',
    osTabAria: 'Chọn hệ điều hành để xem hướng dẫn cài đặt',
    stepListAria: 'Các bước cài đặt',
    prevLabel: 'Bước trước',
    nextLabel: 'Bước tiếp theo',
    stepPrefix: 'Bước',
    copyLabel: 'Sao chép',
    copiedLabel: 'Đã chép',
    visuals: {
      macChipTitle: 'About This Mac (Giới thiệu về máy Mac này)',
      macChipApple: 'Chip: Apple M1 / M2 / M3 / M4',
      macChipOk: 'Tương thích',
      macChipIntel: 'Processor: Intel Core i5 / i7',
      macChipNo: 'Chưa hỗ trợ bản beta này',
      macDmgDragHint: 'Kéo biểu tượng NexaDock thả vào thư mục Applications',
      macAppsHint: 'Finder → Applications → nháy đúp NexaDock',
      macSpotlightHint: 'Hoặc nhấn ⌘ + Space, gõ "NexaDock" rồi Enter',
      macDialogTitle: '“NexaDock” Not Opened',
      macDialogBody: 'Apple could not verify “NexaDock” is free of malware that may harm your Mac.',
      macDialogBtn: 'Done / OK',
      macSettingsTitle: 'System Settings → Privacy & Security',
      macSettingsRow: '“NexaDock” was blocked to protect your Mac.',
      macSettingsBtn: 'Open Anyway',
      winSmartTitle: 'Windows protected your PC',
      winSmartBody: 'Microsoft Defender SmartScreen prevented an unrecognized app from starting.',
      winSmartMore: '1. Bấm More info',
      winSmartRun: '2. Bấm Run anyway',
      primaryLabel: 'Zalo chính của bạn',
      primaryState: 'Giữ nguyên (Chỉ đọc)',
      secondaryLabel: 'Tài khoản phụ trong NexaDock',
      secondaryAction: '+ Thêm tài khoản → Quét QR'
    },
    platforms: [
      {
        id: 'macos',
        label: 'macOS (Apple Silicon)',
        badge: 'macOS 11+ (Apple M1/M2/M3/M4)',
        badgeKind: 'warn',
        steps: [
          {
            id: 'mac-1',
            shortTitle: 'Kiểm tra chip máy Mac',
            title: 'Bước 1: Đảm bảo máy dùng chip Apple Silicon (M1 trở lên)',
            summary:
              'Bấm biểu tượng Apple () ở góc trên bên trái màn hình → chọn "About This Mac" (Giới thiệu về máy Mac này).',
            checklist: [
              'Nếu dòng Chip ghi Apple M1, M2, M3 hoặc M4: máy của bạn hoàn toàn tương thích.',
              'Nếu ghi Intel: bản beta hiện tại chưa hỗ trợ máy Mac Intel.',
              'Để dùng thêm chế độ Web độc lập, máy cần macOS 14 (Sonoma) trở lên.'
            ],
            visual: 'mac-chip'
          },
          {
            id: 'mac-2',
            shortTitle: 'Tải & kéo vào Applications',
            title: 'Bước 2: Mở file .dmg và kéo NexaDock vào Applications',
            summary:
              'Sau khi tải file NexaDock-...-macos-arm64.dmg về máy, hãy nháy đúp vào file đó trong thư mục Downloads.',
            checklist: [
              'Một cửa sổ Finder sẽ bật ra với biểu tượng NexaDock và mũi tên sang thư mục Applications.',
              'Kéo và thả biểu tượng NexaDock sang thư mục Applications (Ứng dụng).',
              'Quan trọng: Không chạy trực tiếp app bên trong cửa sổ .dmg — hãy kéo vào Applications trước.'
            ],
            visual: 'mac-dmg-drag'
          },
          {
            id: 'mac-3',
            shortTitle: 'Mở từ Applications',
            title: 'Bước 3: Mở NexaDock từ thư mục Applications',
            summary:
              'Đóng cửa sổ .dmg, sau đó mở app đã chép vào máy bằng Finder hoặc Spotlight.',
            checklist: [
              'Mở Finder → chọn mục Applications ở thanh bên trái → nháy đúp NexaDock.',
              'Hoặc nhấn tổ hợp phím ⌘ + Space, gõ NexaDock rồi bấm Enter.'
            ],
            visual: 'mac-applications'
          },
          {
            id: 'mac-4',
            shortTitle: 'Cho phép mở lần đầu (Gatekeeper)',
            title: 'Bước 4: Nếu macOS báo chưa xác minh nhà phát triển',
            summary:
              'Vì bản beta chưa ký Apple Developer thương mại, lần đầu mở macOS sẽ hỏi xác nhận bảo mật. Bạn chỉ cần thao tác 1 lần duy nhất (không cần gõ lệnh):',
            checklist: [
              'Ở hộp thoại cảnh báo “NexaDock” Not Opened, bấm nút Done (hoặc OK).',
              'Mở System Settings (Cài đặt hệ thống) → Privacy & Security (Quyền riêng tư & Bảo mật).',
              'Kéo xuống phần Security (Bảo mật), bấm nút Open Anyway (Vẫn mở) bên cạnh dòng NexaDock, nhập mật khẩu máy hoặc Touch ID là xong.'
            ],
            tip: 'Từ các lần mở sau, NexaDock sẽ mở thẳng bình thường như mọi ứng dụng khác.',
            optionalCmdLabel: 'Cách nhanh qua Terminal (tùy chọn — không bắt buộc cho người dùng phổ thông):',
            optionalCmd: 'xattr -cr /Applications/NexaDock.app',
            visual: 'mac-gatekeeper'
          },
          {
            id: 'mac-5',
            shortTitle: 'Thêm tài khoản Zalo phụ',
            title: 'Bước 5: Đăng nhập tài khoản Zalo thứ 2 bằng mã QR',
            summary:
              'Zalo chính đang dùng trên máy của bạn được bảo vệ hoàn toàn ở chế độ chỉ đọc.',
            checklist: [
              'Trong giao diện NexaDock, bấm nút + Thêm tài khoản.',
              'Đặt tên gợi nhớ (ví dụ: Công việc, Bán hàng) rồi bấm Mở Zalo.',
              'Quét mã QR bằng điện thoại như bình thường — dữ liệu mỗi tài khoản nằm ở thư mục riêng.'
            ],
            visual: 'mac-zalo-qr'
          }
        ]
      },
      {
        id: 'windows',
        label: 'Windows 10 / 11',
        badge: 'Windows x64 (.exe / .msi)',
        badgeKind: 'ok',
        steps: [
          {
            id: 'win-1',
            shortTitle: 'Tải & chạy bộ cài',
            title: 'Bước 1: Tải file setup.exe và nháy đúp để cài đặt',
            summary:
              'Chọn file có đuôi -windows-x64-setup.exe ở khung tải bên trên (dễ dùng nhất cho mọi người dùng Windows).',
            checklist: [
              'Đảm bảo máy đã cài sẵn Zalo Desktop chính thức từ zalo.me.',
              'Nháy đúp vào file NexaDock-...-windows-x64-setup.exe vừa tải về.'
            ],
            visual: 'win-installer'
          },
          {
            id: 'win-2',
            shortTitle: 'Vượt màn hình SmartScreen',
            title: 'Bước 2: Nếu hiện bảng xanh "Windows protected your PC"',
            summary:
              'Do bản beta chưa gắn chữ ký số thương mại Authenticode, Windows Defender SmartScreen sẽ hỏi xác nhận lần đầu:',
            checklist: [
              'Bấm vào dòng chữ nhỏ More info (Thông tin thêm) ngay dưới đoạn thông báo.',
              'Nút Run anyway (Vẫn chạy) sẽ hiện ra ở góc dưới — bấm vào đó để tiếp tục cài đặt.',
              'Làm theo các bước Next → Install là hoàn tất.'
            ],
            visual: 'win-smartscreen'
          },
          {
            id: 'win-3',
            shortTitle: 'Máy bật Smart App Control',
            title: 'Bước 3: Lưu ý riêng nếu máy Windows 11 bật Smart App Control',
            summary:
              'Một số máy Windows 11 mới cài đặt bật sẵn Smart App Control (SAC=1) sẽ chặn mọi file .exe chưa ký số.',
            checklist: [
              'Bạn KHÔNG cần tắt Smart App Control trên máy.',
              'Trong trường hợp bị SAC chặn, hãy dùng chế độ Signed-Host Bridge trong trang Tài liệu (chạy giao diện qua Node.js và PowerShell chuẩn của hệ thống).'
            ],
            visual: 'win-sac'
          },
          {
            id: 'win-4',
            shortTitle: 'Quét QR thêm tài khoản',
            title: 'Bước 4: Mở NexaDock và thêm tài khoản Zalo thứ 2',
            summary:
              'Mở NexaDock từ Start Menu hoặc biểu tượng Desktop để bắt đầu dùng nhiều tài khoản song song.',
            checklist: [
              'Zalo chính của bạn luôn được nhận diện ở trạng thái Chỉ đọc (không bị ghi đè hay tắt ngang).',
              'Bấm + Thêm tài khoản → Mở Zalo → quét QR trên điện thoại.'
            ],
            visual: 'win-zalo-qr'
          }
        ]
      },
      {
        id: 'ubuntu',
        label: 'Ubuntu / Linux',
        badge: 'Thử nghiệm Manager (.deb / .AppImage)',
        badgeKind: 'neutral',
        steps: [
          {
            id: 'lin-1',
            shortTitle: 'Chọn gói .deb hoặc .AppImage',
            title: 'Bước 1: Cài đặt trên Ubuntu 24.04 x64',
            summary:
              'Với người dùng Ubuntu, file .deb là cách cài quen thuộc nhất qua trình cài đặt phần mềm.',
            checklist: [
              'Nếu tải .deb: nháy đúp file để mở trong App Center / Software Install và bấm Install.',
              'Nếu tải .AppImage: chuột phải vào file → Properties → Permissions → bật "Executable as Program", sau đó nháy đúp để chạy.'
            ],
            optionalCmdLabel: 'Hoặc cài nhanh bằng dòng lệnh (tùy chọn):',
            optionalCmd: 'sudo apt install ./NexaDock-*-ubuntu24.04-amd64.deb',
            visual: 'linux-pkg'
          },
          {
            id: 'lin-2',
            shortTitle: 'Lưu ý về Zalo trên Linux',
            title: 'Bước 2: Phạm vi hỗ trợ trên Linux',
            summary:
              'VNG hiện không phát hành ứng dụng Zalo Desktop chính thức cho Linux, nhưng có route repack community Experimental — NexaDock nhận diện được.',
            checklist: [
              'Bản Ubuntu (.deb / .AppImage) phục vụ kiểm thử giao diện NexaDock Manager và môi trường mô phỏng (fixture).',
              'Muốn chạy Zalo Desktop thực tế trên Linux: dùng route repack community không chính thức (Experimental) — tự dựng từ thư mục tools/zalo-linux-repack/ trong repo với file DMG bạn tự tải từ zalo.me; xem tài liệu docs/operations.md.',
              'Lưu ý trung thực: client repack không chính thức (không phải của VNG), gọi thoại/video và đồng bộ tin nhắn chưa được kiểm chứng.'
            ],
            links: [
              {
                label: 'tools/zalo-linux-repack/ (GitHub)',
                href: 'https://github.com/manhquydev/nexadock/tree/main/tools/zalo-linux-repack'
              },
              {
                label: 'docs/operations.md (GitHub)',
                href: 'https://github.com/manhquydev/nexadock/blob/main/docs/operations.md'
              }
            ],
            visual: 'linux-note'
          }
        ]
      }
    ]
  },
  en: {
    figTag: 'FIG. 05',
    heading: 'Visual Step-by-Step Installation Walkthrough',
    subtitle:
      'Pick your operating system and step through each screen you will see — designed for everyday users with zero terminal required.',
    simBadge: 'INTERACTIVE WALKTHROUGH',
    osTabAria: 'Choose your operating system for step-by-step install guide',
    stepListAria: 'Installation steps',
    prevLabel: 'Previous step',
    nextLabel: 'Next step',
    stepPrefix: 'Step',
    copyLabel: 'Copy',
    copiedLabel: 'Copied',
    visuals: {
      macChipTitle: 'About This Mac',
      macChipApple: 'Chip: Apple M1 / M2 / M3 / M4',
      macChipOk: 'Supported',
      macChipIntel: 'Processor: Intel Core i5 / i7',
      macChipNo: 'Not supported in beta',
      macDmgDragHint: 'Drag NexaDock onto the Applications folder',
      macAppsHint: 'Finder → Applications → double-click NexaDock',
      macSpotlightHint: 'Or press ⌘ + Space, type "NexaDock", and press Return',
      macDialogTitle: '“NexaDock” Not Opened',
      macDialogBody: 'Apple could not verify “NexaDock” is free of malware that may harm your Mac.',
      macDialogBtn: 'Done / OK',
      macSettingsTitle: 'System Settings → Privacy & Security',
      macSettingsRow: '“NexaDock” was blocked to protect your Mac.',
      macSettingsBtn: 'Open Anyway',
      winSmartTitle: 'Windows protected your PC',
      winSmartBody: 'Microsoft Defender SmartScreen prevented an unrecognized app from starting.',
      winSmartMore: '1. Click More info',
      winSmartRun: '2. Click Run anyway',
      primaryLabel: 'Your Primary Zalo',
      primaryState: 'Preserved (Read-only)',
      secondaryLabel: 'Secondary Account in NexaDock',
      secondaryAction: '+ Add Account → Scan QR'
    },
    platforms: [
      {
        id: 'macos',
        label: 'macOS (Apple Silicon)',
        badge: 'macOS 11+ (Apple M1/M2/M3/M4)',
        badgeKind: 'warn',
        steps: [
          {
            id: 'mac-1',
            shortTitle: 'Check Mac chip',
            title: 'Step 1: Confirm your Mac has an Apple Silicon chip',
            summary:
              'Click the Apple menu () in the top-left corner of your screen and choose "About This Mac".',
            checklist: [
              'If Chip says Apple M1, M2, M3, or M4: your Mac is fully supported.',
              'If it says Intel: Intel Macs are not included in this beta release.',
              'For optional Isolated Web mode, macOS 14 (Sonoma) or newer is required.'
            ],
            visual: 'mac-chip'
          },
          {
            id: 'mac-2',
            shortTitle: 'Open .dmg & drag to Applications',
            title: 'Step 2: Open the .dmg file and drag NexaDock to Applications',
            summary:
              'Open your Downloads folder and double-click NexaDock-...-macos-arm64.dmg.',
            checklist: [
              'A Finder window opens showing NexaDock and an arrow pointing to Applications.',
              'Drag the NexaDock icon onto the Applications folder.',
              'Important: Do not launch NexaDock directly inside the .dmg window — always copy it to Applications first.'
            ],
            visual: 'mac-dmg-drag'
          },
          {
            id: 'mac-3',
            shortTitle: 'Launch from Applications',
            title: 'Step 3: Open NexaDock from Applications',
            summary:
              'Close the .dmg installer window, then launch NexaDock from your Mac.',
            checklist: [
              'Open Finder → select Applications in the sidebar → double-click NexaDock.',
              'Or press ⌘ + Space, type NexaDock, and press Return.'
            ],
            visual: 'mac-applications'
          },
          {
            id: 'mac-4',
            shortTitle: 'First-launch security prompt',
            title: 'Step 4: Allow first launch in Privacy & Security (no terminal needed)',
            summary:
              'Because this beta is not yet Apple-notarized, macOS asks once before opening it the first time:',
            checklist: [
              'On the “NexaDock” Not Opened popup, click Done (or OK).',
              'Open System Settings → Privacy & Security.',
              'Scroll down to the Security section, click Open Anyway next to NexaDock, and confirm with Touch ID or your Mac password.'
            ],
            tip: 'After this one-time approval, NexaDock opens normally every time.',
            optionalCmdLabel: 'Optional Terminal shortcut (not required for everyday users):',
            optionalCmd: 'xattr -cr /Applications/NexaDock.app',
            visual: 'mac-gatekeeper'
          },
          {
            id: 'mac-5',
            shortTitle: 'Add secondary Zalo',
            title: 'Step 5: Sign in to your extra Zalo accounts via QR',
            summary:
              'Your primary Zalo installation remains untouched and strictly read-only.',
            checklist: [
              'Click + Add Account inside NexaDock.',
              'Give the workspace a clear name (e.g., Work, Sales) and click Open Zalo.',
              'Scan the QR code with your phone as usual — each account runs in its own isolated folder.'
            ],
            visual: 'mac-zalo-qr'
          }
        ]
      },
      {
        id: 'windows',
        label: 'Windows 10 / 11',
        badge: 'Windows x64 (.exe / .msi)',
        badgeKind: 'ok',
        steps: [
          {
            id: 'win-1',
            shortTitle: 'Download & run setup',
            title: 'Step 1: Download the setup.exe installer and double-click it',
            summary:
              'Download the -windows-x64-setup.exe file above (recommended for most Windows users).',
            checklist: [
              'Make sure official Zalo Desktop from zalo.me is installed.',
              'Double-click NexaDock-...-windows-x64-setup.exe in your Downloads folder.'
            ],
            visual: 'win-installer'
          },
          {
            id: 'win-2',
            shortTitle: 'SmartScreen prompt',
            title: 'Step 2: If you see "Windows protected your PC"',
            summary:
              'Pre-release builds do not yet have a commercial Authenticode certificate, so SmartScreen asks once:',
            checklist: [
              'Click the small "More info" link under the message.',
              'Click the "Run anyway" button that appears at the bottom.',
              'Follow the standard installer steps (Next → Install).'
            ],
            visual: 'win-smartscreen'
          },
          {
            id: 'win-3',
            shortTitle: 'Smart App Control note',
            title: 'Step 3: If your Windows 11 PC has Smart App Control enabled',
            summary:
              'Fresh Windows 11 installations with Smart App Control (SAC=1) block unsigned executables.',
            checklist: [
              'Do NOT turn off Smart App Control.',
              'Instead, run the Signed-Host Bridge from the Docs page (launches the UI via system-signed Node.js and PowerShell).'
            ],
            visual: 'win-sac'
          },
          {
            id: 'win-4',
            shortTitle: 'Scan QR to add account',
            title: 'Step 4: Open NexaDock and add your secondary Zalo account',
            summary:
              'Launch NexaDock from the Start Menu or Desktop shortcut.',
            checklist: [
              'Your primary Zalo is automatically detected in Read-Only mode.',
              'Click + Add Account → Open Zalo → scan the QR code on your phone.'
            ],
            visual: 'win-zalo-qr'
          }
        ]
      },
      {
        id: 'ubuntu',
        label: 'Ubuntu / Linux',
        badge: 'Experimental Manager (.deb / .AppImage)',
        badgeKind: 'neutral',
        steps: [
          {
            id: 'lin-1',
            shortTitle: 'Install .deb or .AppImage',
            title: 'Step 1: Install on Ubuntu 24.04 x64',
            summary:
              'Double-click the .deb package to install via Ubuntu App Center, or run the .AppImage directly.',
            checklist: [
              'For .deb: double-click the file to open App Center / Software Install and click Install.',
              'For .AppImage: right-click → Properties → Permissions → enable "Executable as Program", then double-click.'
            ],
            optionalCmdLabel: 'Optional terminal install:',
            optionalCmd: 'sudo apt install ./NexaDock-*-ubuntu24.04-amd64.deb',
            visual: 'linux-pkg'
          },
          {
            id: 'lin-2',
            shortTitle: 'Linux support scope',
            title: 'Step 2: Understand Linux scope',
            summary:
              'VNG does not publish an official Zalo Desktop client for Linux, but an Experimental community repack route exists — NexaDock detects it.',
            checklist: [
              'The Linux packages let you evaluate the NexaDock Manager UI and synthetic fixtures.',
              'To run the real Zalo Desktop on Linux: use the unofficial community repack route (Experimental) — build it yourself from the tools/zalo-linux-repack/ folder in this repo with a DMG you download from zalo.me; see docs/operations.md.',
              'Honest caveats: the repack client is unofficial (not from VNG); voice/video calls and message sync are unverified.'
            ],
            links: [
              {
                label: 'tools/zalo-linux-repack/ (GitHub)',
                href: 'https://github.com/manhquydev/nexadock/tree/main/tools/zalo-linux-repack'
              },
              {
                label: 'docs/operations.md (GitHub)',
                href: 'https://github.com/manhquydev/nexadock/blob/main/docs/operations.md'
              }
            ],
            visual: 'linux-note'
          }
        ]
      }
    ]
  }
};
