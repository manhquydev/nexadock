#!/usr/bin/env node
/**
 * Pure Node binary header, Apple ICNS, & OS package inspector + safe read-only OS tool checks
 * for Multi-Zalo Desktop cross-platform build artifacts.
 *
 * Enforces strict classification rules:
 *   - Raw `.exe` / Mach-O / ELF binaries are `compiled-executable` (`is_installer: false`) — NEVER installers!
 *   - NSIS `*-setup.exe` under `bundle/nsis/` -> `installer-nsis` (`is_installer: true`)
 *   - `.msi` -> `installer-msi` (`is_installer: true`)
 *   - `.app` -> `macos-app-bundle` (`is_installer: false`)
 *   - `.dmg` -> `macos-dmg-installer` (`is_installer: true`)
 *   - `.icns` -> `macos-icns-icon` (`is_installer: false`)
 *   - `.deb` -> `debian-package` (`is_installer: true`)
 *   - `.AppImage` -> `linux-appimage` (`is_installer: false`)
 *   - Unsigned or ad-hoc signed builds strictly report `UNSIGNED_DEVELOPMENT_ARTIFACT`!
 *
 * CLI:
 *   node inspect-artifact.mjs --file <artifact-path> [--out <inspection-json>]
 */

import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

function safeRealpath(p) {
  try {
    return fs.realpathSync(p);
  } catch {
    return path.resolve(p);
  }
}

function isMainModule(metaUrl) {
  if (!process.argv[1]) return false;
  const entry = safeRealpath(process.argv[1]);
  const self = safeRealpath(fileURLToPath(metaUrl));
  return process.platform === 'win32'
    ? entry.toLowerCase() === self.toLowerCase()
    : entry === self;
}

function probeReadOnlyCmd(bin, args) {
  const res = spawnSync(bin, args, {
    encoding: 'utf8',
    timeout: 15000,
    windowsHide: true,
  });
  return {
    ok: !res.error && res.status === 0,
    exit_code: res.status,
    stdout: (res.stdout || '').trim(),
    stderr: (res.stderr || '').trim(),
    error: res.error ? res.error.message : null,
  };
}

export function computeFileSha256AndSize(filePath) {
  const stat = fs.statSync(filePath);
  if (stat.isDirectory()) {
    const relFiles = [];
    let totalBytes = 0;
    const walk = (dir) => {
      const entries = fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
      for (const entry of entries) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(full);
        } else if (entry.isFile()) {
          const s = fs.statSync(full);
          totalBytes += s.size;
          relFiles.push({
            rel: path.relative(filePath, full).replace(/\\/g, '/'),
            full,
          });
        } else if (entry.isSymbolicLink()) {
          const target = fs.readlinkSync(full);
          relFiles.push({
            rel: path.relative(filePath, full).replace(/\\/g, '/'),
            symlinkTarget: target,
          });
        }
      }
    };
    walk(filePath);
    relFiles.sort((a, b) => a.rel.localeCompare(b.rel));
    const hash = crypto.createHash('sha256');
    for (const item of relFiles) {
      hash.update(`${item.rel}\0`, 'utf8');
      if (item.symlinkTarget !== undefined) {
        hash.update(`SYMLINK:${item.symlinkTarget}\0`, 'utf8');
      } else {
        hash.update(fs.readFileSync(item.full));
      }
    }
    return {
      file_size_bytes: totalBytes,
      sha256: hash.digest('hex'),
      is_directory_bundle: true,
      bundle_file_count: relFiles.length,
    };
  }

  const hash = crypto.createHash('sha256');
  const fd = fs.openSync(filePath, 'r');
  try {
    const chunk = Buffer.allocUnsafe(1024 * 1024);
    let bytesRead = 0;
    while ((bytesRead = fs.readSync(fd, chunk, 0, chunk.length, null)) > 0) {
      hash.update(chunk.subarray(0, bytesRead));
    }
  } finally {
    fs.closeSync(fd);
  }
  return {
    file_size_bytes: stat.size,
    sha256: hash.digest('hex'),
    is_directory_bundle: false,
  };
}

function readInspectableBuffer(filePath, maxFullBytes = 64 * 1024 * 1024) {
  const stat = fs.statSync(filePath);
  if (stat.size <= maxFullBytes) {
    return { buffer: fs.readFileSync(filePath), truncated: false, totalSize: stat.size };
  }
  const headLen = 8 * 1024 * 1024;
  const tailLen = 2 * 1024 * 1024;
  const fd = fs.openSync(filePath, 'r');
  try {
    const head = Buffer.allocUnsafe(headLen);
    const tail = Buffer.allocUnsafe(tailLen);
    fs.readSync(fd, head, 0, headLen, 0);
    fs.readSync(fd, tail, 0, tailLen, stat.size - tailLen);
    return { buffer: Buffer.concat([head, tail]), tailBuffer: tail, truncated: true, totalSize: stat.size };
  } finally {
    fs.closeSync(fd);
  }
}

export function classifyArtifact(artifactPath, headerBytes = null) {
  const norm = artifactPath.replace(/\\/g, '/');
  const base = path.basename(norm);
  const lowerBase = base.toLowerCase();
  const lowerNorm = norm.toLowerCase();

  if (lowerBase.endsWith('.app')) {
    return {
      artifact_kind: 'macos-app-bundle',
      is_installer: false,
      os: 'macos',
      classification_note: 'macOS .app application bundle directory (requires DMG or notarized archive for distribution)',
    };
  }
  if (lowerBase.endsWith('.dmg')) {
    return {
      artifact_kind: 'macos-dmg-installer',
      is_installer: true,
      os: 'macos',
      classification_note: 'macOS UDIF .dmg disk image installer container',
    };
  }
  if (lowerBase.endsWith('.icns')) {
    return {
      artifact_kind: 'macos-icns-icon',
      is_installer: false,
      os: 'macos',
      classification_note: 'Apple Icon Image (.icns) multi-resolution icon container',
    };
  }
  if (lowerBase.endsWith('.msi')) {
    return {
      artifact_kind: 'installer-msi',
      is_installer: true,
      os: 'windows',
      classification_note: 'Windows Installer (.msi) package built via WiX',
    };
  }
  if (lowerBase.endsWith('.deb')) {
    return {
      artifact_kind: 'debian-package',
      is_installer: true,
      os: 'linux',
      classification_note: 'Debian/Ubuntu (.deb) system installer package',
    };
  }
  if (lowerBase.endsWith('.appimage')) {
    return {
      artifact_kind: 'linux-appimage',
      is_installer: false,
      os: 'linux',
      classification_note: 'Portable self-mounting Linux AppImage bundle (not a system package installer)',
    };
  }
  if (lowerBase.endsWith('.exe')) {
    const isNsisInstaller =
      lowerNorm.includes('/bundle/nsis/') ||
      /-setup\.exe$/i.test(base) ||
      /[._-]setup[._-].*\.exe$/i.test(base);
    if (isNsisInstaller) {
      return {
        artifact_kind: 'installer-nsis',
        is_installer: true,
        os: 'windows',
        classification_note: 'Windows NSIS setup installer executable',
      };
    }
    return {
      artifact_kind: 'compiled-executable',
      is_installer: false,
      os: 'windows',
      classification_note: 'Raw compiled Windows PE executable (NOT an installer)',
    };
  }

  if (headerBytes && headerBytes.length >= 8) {
    if (headerBytes.subarray(0, 4).toString('ascii') === 'icns') {
      return {
        artifact_kind: 'macos-icns-icon',
        is_installer: false,
        os: 'macos',
        classification_note: 'Apple Icon Image (.icns) multi-resolution icon container',
      };
    }
  }

  if (headerBytes && headerBytes.length >= 16) {
    if (headerBytes[0] === 0x4d && headerBytes[1] === 0x5a) {
      return {
        artifact_kind: 'compiled-executable',
        is_installer: false,
        os: 'windows',
        classification_note: 'Raw compiled Windows PE executable (NOT an installer)',
      };
    }
    if (
      headerBytes[0] === 0x7f &&
      headerBytes[1] === 0x45 &&
      headerBytes[2] === 0x4c &&
      headerBytes[3] === 0x46
    ) {
      if (headerBytes[8] === 0x41 && headerBytes[9] === 0x49 && headerBytes[10] === 0x02) {
        return {
          artifact_kind: 'linux-appimage',
          is_installer: false,
          os: 'linux',
          classification_note: 'Portable Linux Type-2 AppImage (AI\\x02 magic)',
        };
      }
      return {
        artifact_kind: 'compiled-executable',
        is_installer: false,
        os: 'linux',
        classification_note: 'Raw compiled Linux ELF executable (NOT an installer)',
      };
    }
    const magicLE = headerBytes.readUInt32LE(0);
    const magicBE = headerBytes.readUInt32BE(0);
    if (
      magicLE === 0xfeedfacf ||
      magicBE === 0xfeedfacf ||
      magicLE === 0xfeedface ||
      magicBE === 0xfeedface ||
      (magicBE === 0xcafebabe && headerBytes.readUInt32BE(4) >= 1 && headerBytes.readUInt32BE(4) <= 16)
    ) {
      return {
        artifact_kind: 'compiled-executable',
        is_installer: false,
        os: 'macos',
        classification_note: 'Raw compiled macOS Mach-O executable (NOT an installer)',
      };
    }
  }

  return {
    artifact_kind: 'unknown-artifact',
    is_installer: false,
    os: 'unknown',
    classification_note: 'Unrecognized artifact extension/magic',
  };
}

export function parseIcnsHeaders(buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length < 16) {
    return { valid_icns: false, error: 'Buffer smaller than minimum ICNS header + chunk (16 bytes)' };
  }
  const magicAscii = buffer.subarray(0, 4).toString('ascii');
  const magicHex = buffer.subarray(0, 4).toString('hex');
  if (magicAscii !== 'icns') {
    return { valid_icns: false, magic_hex: magicHex, error: 'Missing icns (69636e73) file magic' };
  }

  const declaredTotalBytes = buffer.readUInt32BE(4);
  if (declaredTotalBytes !== buffer.length) {
    return {
      valid_icns: false,
      magic_hex: magicHex,
      declared_total_bytes: declaredTotalBytes,
      actual_total_bytes: buffer.length,
      error: `ICNS header length (${declaredTotalBytes}) does not match file size (${buffer.length})`,
    };
  }

  const pngSig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const chunks = [];
  let offset = 8;
  while (offset < declaredTotalBytes) {
    if (offset + 8 > declaredTotalBytes) {
      return { valid_icns: false, error: `Truncated ICNS chunk header at offset ${offset}` };
    }
    const osType = buffer.subarray(offset, offset + 4).toString('ascii');
    const chunkLen = buffer.readUInt32BE(offset + 4);
    if (chunkLen < 8 || offset + chunkLen > declaredTotalBytes) {
      return {
        valid_icns: false,
        error: `Invalid ICNS chunk length ${chunkLen} for ${osType} at offset ${offset}`,
      };
    }
    const payload = buffer.subarray(offset + 8, offset + chunkLen);
    const isPngPayload = payload.length >= 24 && payload.subarray(0, 8).equals(pngSig);
    let pngWidth = null;
    let pngHeight = null;
    if (isPngPayload && payload.subarray(12, 16).toString('ascii') === 'IHDR') {
      pngWidth = payload.readUInt32BE(16);
      pngHeight = payload.readUInt32BE(20);
    }
    chunks.push({
      os_type: osType,
      chunk_bytes: chunkLen,
      payload_bytes: payload.length,
      is_png: isPngPayload,
      width: pngWidth,
      height: pngHeight,
    });
    offset += chunkLen;
  }

  return {
    valid_icns: chunks.length > 0 && offset === declaredTotalBytes,
    magic_hex: magicHex,
    declared_total_bytes: declaredTotalBytes,
    actual_total_bytes: buffer.length,
    chunks_count: chunks.length,
    chunks,
    has_ic07_128x128: chunks.some((c) => c.os_type === 'ic07' && c.is_png && c.width === 128 && c.height === 128),
    has_ic08_256x256: chunks.some((c) => c.os_type === 'ic08' && c.is_png && c.width === 256 && c.height === 256),
  };
}

export function parsePeHeaders(buffer, filePath = null) {
  if (!Buffer.isBuffer(buffer) || buffer.length < 64) {
    return { valid_pe: false, error: 'Buffer smaller than DOS header (64 bytes)' };
  }
  if (buffer[0] !== 0x4d || buffer[1] !== 0x5a) {
    return { valid_pe: false, error: 'Missing MZ DOS header signature' };
  }
  const eLfanew = buffer.readUInt32LE(0x3c);
  if (eLfanew + 24 > buffer.length) {
    return { valid_pe: false, error: `Invalid e_lfanew offset (${eLfanew})` };
  }
  if (
    buffer[eLfanew] !== 0x50 ||
    buffer[eLfanew + 1] !== 0x45 ||
    buffer[eLfanew + 2] !== 0x00 ||
    buffer[eLfanew + 3] !== 0x00
  ) {
    return { valid_pe: false, error: 'Missing PE\\0\\0 signature' };
  }

  const coffOffset = eLfanew + 4;
  const machineCode = buffer.readUInt16LE(coffOffset);
  const numberOfSections = buffer.readUInt16LE(coffOffset + 2);
  const sizeOfOptionalHeader = buffer.readUInt16LE(coffOffset + 16);
  const characteristics = buffer.readUInt16LE(coffOffset + 18);

  const machineMap = {
    0x8664: 'x86_64',
    0xaa64: 'aarch64',
    0x014c: 'x86',
  };
  const cpuArch = machineMap[machineCode] || `unknown(0x${machineCode.toString(16)})`;

  const optOffset = coffOffset + 20;
  let peFormat = 'unknown';
  let subsystem = null;
  let certTableOffset = 0;
  let certTableSize = 0;

  if (sizeOfOptionalHeader >= 96 && optOffset + sizeOfOptionalHeader <= buffer.length) {
    const optMagic = buffer.readUInt16LE(optOffset);
    if (optMagic === 0x020b) peFormat = 'PE32+ (64-bit)';
    else if (optMagic === 0x010b) peFormat = 'PE32 (32-bit)';

    if (optOffset + 70 <= buffer.length) {
      const subsysCode = buffer.readUInt16LE(optOffset + 68);
      subsystem = subsysCode === 2 ? 'WINDOWS_GUI' : subsysCode === 3 ? 'WINDOWS_CUI' : `0x${subsysCode.toString(16)}`;
    }

    const rvaCountOffset = optMagic === 0x020b ? optOffset + 108 : optOffset + 92;
    const dataDirOffset = optMagic === 0x020b ? optOffset + 112 : optOffset + 96;
    if (rvaCountOffset + 4 <= buffer.length) {
      const numRvaAndSizes = buffer.readUInt32LE(rvaCountOffset);
      const secEntryOffset = dataDirOffset + 4 * 8;
      if (numRvaAndSizes > 4 && secEntryOffset + 8 <= optOffset + sizeOfOptionalHeader && secEntryOffset + 8 <= buffer.length) {
        certTableOffset = buffer.readUInt32LE(secEntryOffset);
        certTableSize = buffer.readUInt32LE(secEntryOffset + 4);
      }
    }
  }

  const hasAuthenticodeCertTable = certTableOffset > 0 && certTableSize > 0;

  const latin1Text = buffer.toString('latin1');
  const embeddedManifest = {
    present: false,
    blocks_count: 0,
    requested_execution_level: null,
    ui_access: null,
    long_path_aware: null,
    common_controls_v6: false,
    dpi_aware: null,
    dpi_awareness: null,
    supported_os_guids: [],
  };

  const guidSet = new Set();
  for (const m of latin1Text.matchAll(/<assembly[\s>][\s\S]{1,65536}?<\/assembly>/g)) {
    const xml = m[0];
    embeddedManifest.present = true;
    embeddedManifest.blocks_count += 1;

    const reqMatch = xml.match(/<(?:[\w-]+:)?requestedExecutionLevel\b[^>]*\blevel\s*=\s*"([^"]+)"/i);
    if (reqMatch && !embeddedManifest.requested_execution_level) {
      embeddedManifest.requested_execution_level = reqMatch[1];
    }
    const uiMatch = xml.match(/<(?:[\w-]+:)?requestedExecutionLevel\b[^>]*\buiAccess\s*=\s*"([^"]+)"/i);
    if (uiMatch && !embeddedManifest.ui_access) {
      embeddedManifest.ui_access = uiMatch[1];
    }
    const lpMatch = xml.match(/<(?:[\w-]+:)?longPathAware\b[^>]*>([^<]+)<\/(?:[\w-]+:)?longPathAware>/i);
    if (lpMatch && !embeddedManifest.long_path_aware) {
      embeddedManifest.long_path_aware = lpMatch[1].trim();
    }
    if (/Microsoft\.Windows\.Common-Controls/i.test(xml)) {
      embeddedManifest.common_controls_v6 = true;
    }
    const dpiAwareMatch = xml.match(/<(?:[\w-]+:)?dpiAware\b[^>]*>([^<]+)<\/(?:[\w-]+:)?dpiAware>/i);
    if (dpiAwareMatch && !embeddedManifest.dpi_aware) {
      embeddedManifest.dpi_aware = dpiAwareMatch[1].trim();
    }
    const dpiAwarenessMatch = xml.match(/<(?:[\w-]+:)?dpiAwareness\b[^>]*>([^<]+)<\/(?:[\w-]+:)?dpiAwareness>/i);
    if (dpiAwarenessMatch && !embeddedManifest.dpi_awareness) {
      embeddedManifest.dpi_awareness = dpiAwarenessMatch[1].trim();
    }
    for (const gm of xml.matchAll(/<supportedOS\b[^>]*\bId\s*=\s*"([^"]+)"/gi)) {
      guidSet.add(gm[1]);
    }
  }
  embeddedManifest.supported_os_guids = Array.from(guidSet);

  const importsWebView2Loader = /WebView2Loader\.dll/i.test(latin1Text);
  let sidecarWebView2Loader = { present: false, path: null, size_bytes: null };
  if (filePath) {
    const sidecarCandidate = path.join(path.dirname(filePath), 'WebView2Loader.dll');
    if (fs.existsSync(sidecarCandidate) && fs.statSync(sidecarCandidate).isFile()) {
      sidecarWebView2Loader = {
        present: true,
        path: path.basename(sidecarCandidate),
        size_bytes: fs.statSync(sidecarCandidate).size,
      };
    }
  }

  return {
    valid_pe: true,
    pe_format: peFormat,
    coff_machine_hex: `0x${machineCode.toString(16).padStart(4, '0')}`,
    cpu_architecture: cpuArch,
    sections_count: numberOfSections,
    subsystem,
    is_dll: (characteristics & 0x2000) !== 0,
    authenticode: {
      certificate_table_offset: certTableOffset,
      certificate_table_size: certTableSize,
      has_certificate_table: hasAuthenticodeCertTable,
      signing_status: hasAuthenticodeCertTable
        ? 'CERTIFICATE_TABLE_PRESENT_REQUIRES_CHAIN_VALIDATION'
        : 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    },
    embedded_manifest: embeddedManifest,
    imports_webview2_loader: importsWebView2Loader,
    sidecar_webview2_loader: sidecarWebView2Loader,
  };
}

export function parseMsiHeaders(buffer) {
  const cfbMagic = Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
  const validCfb = Buffer.isBuffer(buffer) && buffer.length >= 8 && buffer.subarray(0, 8).equals(cfbMagic);
  return {
    valid_cfb_header: validCfb,
    signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
  };
}

function mapMachOCpuType(cpuType) {
  if (cpuType === 0x0100000c) return 'aarch64';
  if (cpuType === 0x01000007) return 'x86_64';
  return `unknown(0x${cpuType.toString(16)})`;
}

export function parseMachOHeaders(buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length < 32) {
    return { valid_macho: false, error: 'Buffer smaller than Mach-O header (32 bytes)' };
  }

  const magicBE = buffer.readUInt32BE(0);
  const magicLE = buffer.readUInt32LE(0);

  if (magicBE === 0xcafebabe || magicBE === 0xcafebabf) {
    const nfatArch = buffer.readUInt32BE(4);
    if (nfatArch >= 1 && nfatArch <= 16 && buffer.length >= 8 + nfatArch * 20) {
      const slices = [];
      const stride = magicBE === 0xcafebabf ? 32 : 20;
      for (let i = 0; i < nfatArch; i += 1) {
        const off = 8 + i * stride;
        if (off + 8 <= buffer.length) {
          const cpuType = buffer.readUInt32BE(off);
          slices.push(mapMachOCpuType(cpuType));
        }
      }
      return {
        valid_macho: true,
        macho_format: 'Universal Fat Binary',
        magic_hex: `0x${magicBE.toString(16)}`,
        is_universal: true,
        architectures: slices,
        cpu_architecture: slices.length > 1 ? `universal(${slices.join('+')})` : (slices[0] || 'unknown'),
      };
    }
  }

  if (magicLE === 0xfeedfacf || magicBE === 0xfeedfacf) {
    const isLE = magicLE === 0xfeedfacf;
    const readU32 = (off) => (isLE ? buffer.readUInt32LE(off) : buffer.readUInt32BE(off));
    const cpuType = readU32(4);
    const fileType = readU32(12);
    const ncmds = readU32(16);
    const arch = mapMachOCpuType(cpuType);

    let hasCodeSigLoadCmd = false;
    let cmdOffset = 32;
    for (let i = 0; i < Math.min(ncmds, 256); i += 1) {
      if (cmdOffset + 8 > buffer.length) break;
      const cmd = readU32(cmdOffset);
      const cmdSize = readU32(cmdOffset + 4);
      if (cmd === 0x1d) {
        hasCodeSigLoadCmd = true;
        break;
      }
      if (cmdSize < 8) break;
      cmdOffset += cmdSize;
    }

    return {
      valid_macho: true,
      macho_format: 'Mach-O 64-bit',
      magic_hex: '0xfeedfacf',
      endianness: isLE ? 'little' : 'big',
      is_universal: false,
      architectures: [arch],
      cpu_architecture: arch,
      macho_filetype: fileType === 2 ? 'MH_EXECUTE' : `0x${fileType.toString(16)}`,
      has_lc_code_signature: hasCodeSigLoadCmd,
    };
  }

  return { valid_macho: false, error: 'Unrecognized Mach-O magic bytes' };
}

export function parseInfoPlistXml(plistText) {
  const getKeyString = (key) => {
    const re = new RegExp(`<key>\\s*${key}\\s*<\\/key>\\s*<string>([^<]*)<\\/string>`, 'm');
    const m = plistText.match(re);
    return m ? m[1].trim() : null;
  };
  return {
    bundle_identifier: getKeyString('CFBundleIdentifier'),
    bundle_name: getKeyString('CFBundleName'),
    bundle_display_name: getKeyString('CFBundleDisplayName'),
    short_version: getKeyString('CFBundleShortVersionString'),
    bundle_version: getKeyString('CFBundleVersion'),
    executable: getKeyString('CFBundleExecutable'),
    icon_file: getKeyString('CFBundleIconFile'),
    minimum_system_version: getKeyString('LSMinimumSystemVersion'),
  };
}

export function parseDmgTrailer(tailBuffer) {
  if (!Buffer.isBuffer(tailBuffer) || tailBuffer.length < 512) {
    return { valid_udif_koly_trailer: false };
  }
  const trailer = tailBuffer.subarray(tailBuffer.length - 512);
  const isKoly = trailer.subarray(0, 4).toString('ascii') === 'koly';
  return {
    valid_udif_koly_trailer: isKoly,
  };
}

function compareGlibcVer(a, b) {
  const pa = a.split('.').map((n) => Number.parseInt(n, 10) || 0);
  const pb = b.split('.').map((n) => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
    if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) > (pb[i] || 0) ? 1 : -1;
  }
  return 0;
}

export function parseElfHeaders(buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length < 64) {
    return { valid_elf: false, error: 'Buffer smaller than ELF64 header (64 bytes)' };
  }
  if (buffer[0] !== 0x7f || buffer[1] !== 0x45 || buffer[2] !== 0x4c || buffer[3] !== 0x46) {
    return { valid_elf: false, error: 'Missing \\x7fELF magic' };
  }

  const eiClass = buffer[4];
  const eiData = buffer[5];
  const isLE = eiData === 1;
  const readU16 = (off) => (isLE ? buffer.readUInt16LE(off) : buffer.readUInt16BE(off));

  const eType = readU16(16);
  const eMachine = readU16(18);
  const machineMap = {
    0x3e: 'x86_64',
    0xb7: 'aarch64',
    0x03: 'x86',
  };
  const cpuArch = machineMap[eMachine] || `unknown(0x${eMachine.toString(16)})`;
  const isAppImageType2 = buffer[8] === 0x41 && buffer[9] === 0x49 && buffer[10] === 0x02;

  const text = buffer.toString('latin1');
  const glibcSet = new Set();
  for (const m of text.matchAll(/GLIBC_(\d+\.\d+(?:\.\d+)?)/g)) {
    glibcSet.add(m[1]);
  }
  const glibcVersions = Array.from(glibcSet).sort(compareGlibcVer);
  const maxGlibc = glibcVersions.length > 0 ? glibcVersions[glibcVersions.length - 1] : null;

  return {
    valid_elf: true,
    elf_class: eiClass === 2 ? 'ELF64' : eiClass === 1 ? 'ELF32' : `class(${eiClass})`,
    endianness: isLE ? 'little' : 'big',
    elf_type: eType === 3 ? 'ET_DYN (PIE/Shared)' : eType === 2 ? 'ET_EXEC' : `0x${eType.toString(16)}`,
    cpu_architecture: cpuArch,
    is_appimage_type2_magic: isAppImageType2,
    glibc_versions: glibcVersions,
    max_glibc_version: maxGlibc,
  };
}

export function parseAppImageHeaders(buffer) {
  const elf = parseElfHeaders(buffer);
  return {
    valid_elf: elf.valid_elf,
    is_type2_appimage: Boolean(elf.valid_elf && elf.is_appimage_type2_magic),
    appimage_magic_offset_8: Buffer.isBuffer(buffer) && buffer.length >= 11
      ? buffer.subarray(8, 11).toString('hex')
      : null,
    cpu_architecture: elf.cpu_architecture || 'unknown',
    max_glibc_version: elf.max_glibc_version || null,
  };
}

export function parseDebHeaders(buffer, filePath = null) {
  const arMagic = '!<arch>\n';
  const validAr = Buffer.isBuffer(buffer) && buffer.length >= 8 && buffer.subarray(0, 8).toString('ascii') === arMagic;
  let hasDebianBinaryMember = false;
  if (validAr && buffer.length >= 68) {
    const firstMemberName = buffer.subarray(8, 24).toString('ascii').trim();
    hasDebianBinaryMember = firstMemberName.startsWith('debian-binary');
  }

  let dpkgDebFields = null;
  if (filePath && process.platform === 'linux') {
    const probe = probeReadOnlyCmd('dpkg-deb', ['-I', filePath]);
    if (probe.ok) {
      const getField = (name) => {
        const m = probe.stdout.match(new RegExp(`^\\s*${name}:\\s*(.+)$`, 'mi'));
        return m ? m[1].trim() : null;
      };
      dpkgDebFields = {
        package: getField('Package'),
        version: getField('Version'),
        architecture: getField('Architecture'),
        depends: getField('Depends'),
        installed_size: getField('Installed-Size'),
        maintainer: getField('Maintainer'),
      };
    }
  }

  let mappedArch = 'unknown';
  if (dpkgDebFields?.architecture === 'amd64') mappedArch = 'x86_64';
  else if (dpkgDebFields?.architecture === 'arm64') mappedArch = 'aarch64';
  else if (filePath) {
    if (/amd64|x86_64/i.test(path.basename(filePath))) mappedArch = 'x86_64';
    else if (/arm64|aarch64/i.test(path.basename(filePath))) mappedArch = 'aarch64';
  }

  return {
    valid_ar_header: validAr,
    has_debian_binary_member: hasDebianBinaryMember,
    cpu_architecture: mappedArch,
    dpkg_deb_control: dpkgDebFields,
  };
}

function inspectDarwinCodesignReadOnly(targetPath) {
  if (process.platform !== 'darwin') {
    return { checked_on_darwin: false, signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT' };
  }
  const res = probeReadOnlyCmd('codesign', ['-dv', '--verbose=4', targetPath]);
  const combined = `${res.stdout}\n${res.stderr}`;
  if (!res.ok || /code object is not signed/i.test(combined)) {
    return {
      checked_on_darwin: true,
      signed: false,
      is_adhoc: false,
      signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    };
  }
  const isAdhoc = /Signature=adhoc/i.test(combined) || !/Authority=Developer ID Application/i.test(combined);
  return {
    checked_on_darwin: true,
    signed: !isAdhoc,
    is_adhoc: isAdhoc,
    authority: (combined.match(/^Authority=(.+)$/m) || [])[1] || null,
    team_identifier: (combined.match(/^TeamIdentifier=(.+)$/m) || [])[1] || null,
    signing_status: isAdhoc ? 'UNSIGNED_DEVELOPMENT_ARTIFACT' : 'APPLE_DEVELOPER_ID_SIGNED',
  };
}

export function inspectArtifact(artifactPath) {
  const absPath = path.resolve(artifactPath);
  if (!fs.existsSync(absPath)) {
    throw new Error(`Artifact not found: ${absPath}`);
  }

  const stat = fs.statSync(absPath);
  const hashAndSize = computeFileSha256AndSize(absPath);

  if (stat.isDirectory()) {
    const classification = classifyArtifact(absPath, null);
    if (classification.artifact_kind === 'macos-app-bundle') {
      const plistPath = path.join(absPath, 'Contents', 'Info.plist');
      const plist = fs.existsSync(plistPath)
        ? parseInfoPlistXml(fs.readFileSync(plistPath, 'utf8'))
        : null;
      const macosBinDir = path.join(absPath, 'Contents', 'MacOS');
      let execName = plist?.executable || null;
      if (!execName && fs.existsSync(macosBinDir)) {
        const files = fs.readdirSync(macosBinDir);
        execName = files[0] || null;
      }
      const mainExecPath = execName ? path.join(macosBinDir, execName) : null;
      const macho = mainExecPath && fs.existsSync(mainExecPath)
        ? parseMachOHeaders(readInspectableBuffer(mainExecPath).buffer)
        : { valid_macho: false, error: 'Main executable missing inside Contents/MacOS' };
      const codesign = inspectDarwinCodesignReadOnly(absPath);

      return {
        name: path.basename(absPath),
        path: absPath,
        ...classification,
        cpu_architecture: macho.cpu_architecture || 'unknown',
        file_size_bytes: hashAndSize.file_size_bytes,
        sha256: hashAndSize.sha256,
        signing_status: codesign.signing_status,
        details: {
          bundle_file_count: hashAndSize.bundle_file_count,
          info_plist: plist,
          main_executable: execName,
          macho,
          codesign,
        },
      };
    }
    throw new Error(`Directory artifact "${absPath}" is not a .app bundle`);
  }

  const { buffer, tailBuffer } = readInspectableBuffer(absPath);
  const classification = classifyArtifact(absPath, buffer.subarray(0, 32));
  const posixExecutableBit = (stat.mode & 0o111) !== 0;

  let cpuArchitecture = 'unknown';
  let signingStatus = 'UNSIGNED_DEVELOPMENT_ARTIFACT';
  let details = {};

  if (classification.artifact_kind === 'compiled-executable' && classification.os === 'windows') {
    const pe = parsePeHeaders(buffer, absPath);
    cpuArchitecture = pe.cpu_architecture || 'unknown';
    signingStatus = 'UNSIGNED_DEVELOPMENT_ARTIFACT';
    details = { pe };
  } else if (classification.artifact_kind === 'installer-nsis') {
    const pe = parsePeHeaders(buffer, absPath);
    cpuArchitecture = /x64|x86_64/i.test(path.basename(absPath)) ? 'x86_64' : (pe.cpu_architecture || 'x86_64');
    details = { pe, nsis_stub_arch: pe.cpu_architecture };
  } else if (classification.artifact_kind === 'installer-msi') {
    const msi = parseMsiHeaders(buffer);
    cpuArchitecture = /arm64|aarch64/i.test(path.basename(absPath)) ? 'aarch64' : 'x86_64';
    details = { msi };
  } else if (classification.artifact_kind === 'macos-icns-icon') {
    const icns = parseIcnsHeaders(buffer);
    cpuArchitecture = 'any';
    details = { icns };
  } else if (classification.artifact_kind === 'compiled-executable' && classification.os === 'macos') {
    const macho = parseMachOHeaders(buffer);
    const codesign = inspectDarwinCodesignReadOnly(absPath);
    cpuArchitecture = macho.cpu_architecture || 'unknown';
    signingStatus = codesign.signing_status;
    details = {
      macho,
      codesign,
      posix_executable_bit: posixExecutableBit,
    };
  } else if (classification.artifact_kind === 'macos-dmg-installer') {
    const dmg = parseDmgTrailer(tailBuffer || buffer);
    const codesign = inspectDarwinCodesignReadOnly(absPath);
    if (/aarch64|arm64/i.test(path.basename(absPath))) cpuArchitecture = 'aarch64';
    else if (/x64|x86_64/i.test(path.basename(absPath))) cpuArchitecture = 'x86_64';
    signingStatus = codesign.signing_status;
    details = { dmg, codesign };
  } else if (classification.artifact_kind === 'compiled-executable' && classification.os === 'linux') {
    const elf = parseElfHeaders(buffer);
    cpuArchitecture = elf.cpu_architecture || 'unknown';
    let lddSummary = null;
    if (process.platform === 'linux') {
      const lddRes = probeReadOnlyCmd('ldd', [absPath]);
      if (lddRes.ok) {
        const lines = lddRes.stdout.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
        const missingLibs = lines.filter((l) => l.includes('not found'));
        lddSummary = {
          linked_count: lines.length,
          missing_libraries: missingLibs,
          all_resolved: missingLibs.length === 0,
        };
      }
    }
    details = {
      elf,
      posix_executable_bit: posixExecutableBit,
      posix_mode_octal: `0${(stat.mode & 0o777).toString(8)}`,
      posix_permissions_note: process.platform === 'win32'
        ? 'Inspected on Windows NTFS (POSIX mode bits verified natively on Linux)'
        : null,
      ldd: lddSummary,
    };
  } else if (classification.artifact_kind === 'linux-appimage') {
    const appimage = parseAppImageHeaders(buffer);
    cpuArchitecture = appimage.cpu_architecture || 'x86_64';
    details = {
      appimage,
      posix_executable_bit: posixExecutableBit,
      posix_mode_octal: `0${(stat.mode & 0o777).toString(8)}`,
    };
  } else if (classification.artifact_kind === 'debian-package') {
    const deb = parseDebHeaders(buffer, absPath);
    cpuArchitecture = deb.cpu_architecture || 'x86_64';
    details = { deb };
  }

  return {
    name: path.basename(absPath),
    path: absPath,
    ...classification,
    cpu_architecture: cpuArchitecture,
    file_size_bytes: hashAndSize.file_size_bytes,
    sha256: hashAndSize.sha256,
    signing_status: signingStatus,
    details,
  };
}

function parseArgs(argv) {
  const opts = { file: null, out: null, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') opts.help = true;
    else if (arg === '--file' && i + 1 < argv.length) opts.file = argv[++i];
    else if (arg.startsWith('--file=')) opts.file = arg.slice('--file='.length);
    else if (arg === '--out' && i + 1 < argv.length) opts.out = argv[++i];
    else if (arg.startsWith('--out=')) opts.out = arg.slice('--out='.length);
    else if (!arg.startsWith('--') && !opts.file) opts.file = arg;
    else throw new Error(`Unknown or incomplete CLI option: ${arg}`);
  }
  return opts;
}

function cliMain() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help || !opts.file) {
    console.log('Usage: node inspect-artifact.mjs --file <artifact-path> [--out <inspection-json>]');
    process.exit(opts.help ? 0 : 1);
  }

  const inspection = inspectArtifact(opts.file);
  if (opts.out) {
    const outAbs = path.resolve(opts.out);
    fs.mkdirSync(path.dirname(outAbs), { recursive: true });
    fs.writeFileSync(outAbs, `${JSON.stringify(inspection, null, 2)}\n`, 'utf8');
  }
  console.log(JSON.stringify(inspection, null, 2));
}

if (isMainModule(import.meta.url)) {
  try {
    cliMain();
  } catch (err) {
    console.error(`[inspect-artifact] ERROR: ${err.message}`);
    process.exit(1);
  }
}
