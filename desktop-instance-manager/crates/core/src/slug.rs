//! Vietnamese-aware profile slugification, display name validation, and 8-hex ID generation.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::DimError;

/// Maximum permitted length of a profile display name in characters.
pub const MAX_PROFILE_NAME_LEN: usize = 64;

/// Maximum permitted length of the slug prefix in `<slug>_<8hex>`.
pub const MAX_SLUG_LEN: usize = 40;

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Validates and trims a user-supplied profile display name.
pub fn validate_profile_name(raw_name: &str) -> Result<String, DimError> {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() {
        return Err(DimError::InvalidProfileName(
            "profile name cannot be empty".to_owned(),
        ));
    }
    if trimmed.chars().count() > MAX_PROFILE_NAME_LEN {
        return Err(DimError::InvalidProfileName(format!(
            "profile name exceeds {MAX_PROFILE_NAME_LEN} characters"
        )));
    }
    if trimmed.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(DimError::InvalidProfileName(
            "profile name contains NUL or control characters".to_owned(),
        ));
    }
    if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
        return Err(DimError::InvalidProfileName(
            "profile name cannot contain path separators or '..'".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// Normalizes Vietnamese diacritics (`đ`/`Đ` -> `d`, vowels -> ASCII) and generates a safe directory slug.
#[must_use]
pub fn slugify_profile_name(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len().min(MAX_SLUG_LEN));
    let mut last_was_dash = true;

    for ch in raw.chars() {
        if is_combining_mark(ch) {
            continue;
        }
        if let Some(mapped) = fold_vietnamese_char(ch) {
            if mapped.is_ascii_alphanumeric() {
                slug.push(mapped.to_ascii_lowercase());
                last_was_dash = false;
            } else if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        } else if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }

        if slug.len() >= MAX_SLUG_LEN {
            break;
        }
    }

    let trimmed = slug.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "profile".to_owned()
    } else {
        trimmed
    }
}

/// Builds a `<slug>_<8hex>` directory name from a display name and 8-hex profile ID.
#[must_use]
pub fn build_directory_name(display_name: &str, id: &str) -> String {
    format!("{}_{}", slugify_profile_name(display_name), id)
}

/// Generates a random 8-character lowercase hexadecimal identifier using OS-seeded `RandomState`.
#[must_use]
pub fn generate_profile_id() -> String {
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(seq);
    hasher.write_u128(nanos);
    hasher.write_u32(std::process::id());
    let mixed = hasher.finish();
    format!("{:08x}", (mixed ^ (mixed >> 32)) as u32)
}

/// Validates that a profile ID consists of exactly 8 lowercase hexadecimal ASCII characters.
#[must_use]
pub fn is_valid_profile_id(id: &str) -> bool {
    id.len() == 8
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

const fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF
    )
}

const fn fold_vietnamese_char(ch: char) -> Option<char> {
    match ch {
        'đ' | 'Đ' => Some('d'),
        'a' | 'A' | 'á' | 'Á' | 'à' | 'À' | 'ả' | 'Ả' | 'ã' | 'Ã' | 'ạ' | 'Ạ' | 'ă' | 'Ă'
        | 'ắ' | 'Ắ' | 'ằ' | 'Ằ' | 'ẳ' | 'Ẳ' | 'ẵ' | 'Ẵ' | 'ặ' | 'Ặ' | 'â' | 'Â' | 'ấ' | 'Ấ'
        | 'ầ' | 'Ầ' | 'ẩ' | 'Ẩ' | 'ẫ' | 'Ẫ' | 'ậ' | 'Ậ' => Some('a'),
        'e' | 'E' | 'é' | 'É' | 'è' | 'È' | 'ẻ' | 'Ẻ' | 'ẽ' | 'Ẽ' | 'ẹ' | 'Ẹ' | 'ê' | 'Ê'
        | 'ế' | 'Ế' | 'ề' | 'Ề' | 'ể' | 'Ể' | 'ễ' | 'Ễ' | 'ệ' | 'Ệ' => Some('e'),
        'i' | 'I' | 'í' | 'Í' | 'ì' | 'Ì' | 'ỉ' | 'Ỉ' | 'ĩ' | 'Ĩ' | 'ị' | 'Ị' => Some('i'),
        'o' | 'O' | 'ó' | 'Ó' | 'ò' | 'Ò' | 'ỏ' | 'Ỏ' | 'õ' | 'Õ' | 'ọ' | 'Ọ' | 'ô' | 'Ô'
        | 'ố' | 'Ố' | 'ồ' | 'Ồ' | 'ổ' | 'Ổ' | 'ỗ' | 'Ỗ' | 'ộ' | 'Ộ' | 'ơ' | 'Ơ' | 'ớ' | 'Ớ'
        | 'ờ' | 'Ờ' | 'ở' | 'Ở' | 'ỡ' | 'Ỡ' | 'ợ' | 'Ợ' => Some('o'),
        'u' | 'U' | 'ú' | 'Ú' | 'ù' | 'Ù' | 'ủ' | 'Ủ' | 'ũ' | 'Ũ' | 'ụ' | 'Ụ' | 'ư' | 'Ư'
        | 'ứ' | 'Ứ' | 'ừ' | 'Ừ' | 'ử' | 'Ử' | 'ữ' | 'Ữ' | 'ự' | 'Ự' => Some('u'),
        'y' | 'Y' | 'ý' | 'Ý' | 'ỳ' | 'Ỳ' | 'ỷ' | 'Ỷ' | 'ỹ' | 'Ỹ' | 'ỵ' | 'Ỵ' => Some('y'),
        _ => None,
    }
}
