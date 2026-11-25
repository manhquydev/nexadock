//! Telegram Bot API transport: token validation, chat capture, and document
//! uploads against `https://api.telegram.org` with plain HTTP (rustls).
//!
//! A bot pairing is stateless: reconnecting only rebuilds
//! [`BotTransport`] from the stored token and chat identifier. The token and
//! chat identifier are never written to logs or error messages.

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use ureq::Agent;

use crate::error::VaultError;
use crate::transport::{RemoteRef, TelegramTransport};
use crate::Result;

/// Base URL of the public Telegram Bot API.
const TELEGRAM_API_BASE_URL: &str = "https://api.telegram.org";
/// Wall-clock limit for one `getMe` token validation request.
const GET_ME_TIMEOUT: Duration = Duration::from_secs(15);
/// Wall-clock limit for one `sendDocument` upload.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(180);
/// Long-poll window requested from `getUpdates` on each iteration (Bot API caps this at 50 s).
const GET_UPDATES_LONG_POLL_SECS: u64 = 50;
/// Slack added on top of the long-poll window for the HTTP request timeout.
const GET_UPDATES_SLACK_SECS: u64 = 5;
/// Hard ceiling for the total `wait_for_chat` wait.
const WAIT_FOR_CHAT_MAX_SECS: u64 = 60;
/// Maximum Telegram Bot API document size (decimal 50 MB).
const BOT_API_MAX_DOCUMENT_BYTES: u64 = 50_000_000;
fn validate_bot_upload_size(size: u64) -> Result<()> {
    if size > BOT_API_MAX_DOCUMENT_BYTES {
        return Err(VaultError::Transport(
            "file exceeds the Bot API 50 MB limit; use the full Telegram account tier for large files".into(),
        ));
    }
    Ok(())
}

/// Public metadata about a validated Telegram bot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotInfo {
    /// The bot's public username without the leading `@`.
    pub username: String,
}

/// Telegram Bot API transport uploading documents to one configured chat.
pub struct BotTransport {
    token: String,
    chat_id: i64,
    base_url: String,
    http: Agent,
}

impl BotTransport {
    /// Builds a bot transport for one validated token and captured chat.
    pub fn new(token: impl Into<String>, chat_id: i64) -> Self {
        Self {
            token: token.into(),
            chat_id,
            base_url: TELEGRAM_API_BASE_URL.to_owned(),
            http: Agent::new(),
        }
    }

    /// Overrides the API base URL (offline tests only).
    #[cfg(test)]
    fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.to_owned();
        self
    }

    /// Validates a bot token against `getMe` and returns the bot's username.
    ///
    /// Malformed tokens are rejected without touching the network; otherwise
    /// `GET .../bot<token>/getMe` decides validity and API error bodies are
    /// surfaced as [`VaultError::Transport`] with the API `description`.
    pub fn validate_token(token: &str) -> Result<BotInfo> {
        let token = token.trim();
        validate_token_shape(token)?;
        let http = Agent::new();
        let request = http
            .get(&api_url(TELEGRAM_API_BASE_URL, token, "getMe"))
            .timeout(GET_ME_TIMEOUT);
        let body = into_body(request.call(), token)?;
        parse_get_me_response(&body)
    }

    /// Generates a random single-use nonce suitable for Telegram `?start=<nonce>` deep links.
    #[must_use]
    pub fn generate_pair_nonce() -> String {
        let bytes = os_entropy_bytes();
        let mut hex = String::with_capacity(32);
        for byte in &bytes[..16] {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        hex
    }

    /// Long-polls `getUpdates` until the bot receives `/start <expected_nonce>` in a private chat.
    ///
    /// Updates are consumed by advancing the poll offset past stale or
    /// non-matching messages. Waits at most `min(timeout_secs, 60)` seconds in
    /// total, capping every HTTP `getUpdates` request timeout by `remaining`.
    pub fn wait_for_chat(token: &str, expected_nonce: &str, timeout_secs: u64) -> Result<i64> {
        Self::wait_for_chat_against(TELEGRAM_API_BASE_URL, token, expected_nonce, timeout_secs)
    }

    fn wait_for_chat_against(
        base_url: &str,
        token: &str,
        expected_nonce: &str,
        timeout_secs: u64,
    ) -> Result<i64> {
        let token = token.trim();
        validate_token_shape(token)?;
        let expected_nonce = expected_nonce.trim();
        validate_pair_nonce(expected_nonce)?;
        let http = Agent::new();
        let total = Duration::from_secs(timeout_secs.clamp(1, WAIT_FOR_CHAT_MAX_SECS));
        let deadline = Instant::now() + total;
        let mut offset: Option<i64> = None;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(VaultError::Timeout(
                    "no private chat messaged the bot in time".into(),
                ));
            }
            let poll_secs = remaining
                .as_secs()
                .saturating_sub(1)
                .min(GET_UPDATES_LONG_POLL_SECS);
            let http_timeout =
                Duration::from_secs(poll_secs.saturating_add(GET_UPDATES_SLACK_SECS))
                    .min(remaining);
            let mut body = serde_json::Map::new();
            body.insert("timeout".into(), serde_json::Value::from(poll_secs));
            if let Some(offset) = offset {
                body.insert("offset".into(), serde_json::Value::from(offset));
            }
            body.insert(
                "allowed_updates".into(),
                serde_json::Value::from(["message"]),
            );
            let request = http
                .post(&api_url(base_url, token, "getUpdates"))
                .timeout(http_timeout);
            let response = match request.send_json(serde_json::Value::Object(body)) {
                Ok(response) => response,
                Err(error) if Instant::now() >= deadline && is_timeout_ureq_error(&error) => {
                    return Err(VaultError::Timeout(
                        "no private chat messaged the bot in time".into(),
                    ));
                }
                Err(error) => return Err(bot_api_error(error, token)),
            };
            let bytes = response_body(response, token)?;
            let page = parse_updates_response(&bytes, expected_nonce)?;
            if let Some(chat_id) = page.private_chat_id {
                return Ok(chat_id);
            }
            if let Some(max_update_id) = page.max_update_id {
                offset = Some(max_update_id.saturating_add(1));
            }
        }
    }
}

impl TelegramTransport for BotTransport {
    fn upload_document(
        &self,
        _profile_id: &str,
        path: &Path,
        sha256: &str,
        caption: &str,
    ) -> Result<RemoteRef> {
        let mut file = fs::File::open(path).map_err(|error| VaultError::io(path, error))?;
        let pre_size = file
            .metadata()
            .map_err(|error| VaultError::io(path, error))?
            .len();
        validate_bot_upload_size(pre_size)?;
        // Same-FD verification: hash, check metadata stability, and stream from
        // the identical opened descriptor so path swaps cannot cause TOCTOU.
        let size = crate::watch::verify_opened_file(&mut file, path, sha256)?;
        validate_bot_upload_size(size)?;
        let file_name = sanitize_file_name(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file"),
        );
        let boundary = upload_boundary(sha256);
        let body = multipart_head(&boundary, &file_name)
            .chain(file.take(size))
            .chain(multipart_tail(&boundary, self.chat_id, caption));
        let content_type = format!("multipart/form-data; boundary={boundary}");
        let request = self
            .http
            .post(&api_url(&self.base_url, &self.token, "sendDocument"))
            .timeout(UPLOAD_TIMEOUT)
            .set("Content-Type", &content_type);
        let response = into_body(request.send(body), &self.token)?;
        parse_send_document_response(&response)
    }
}

/// Builds a Bot API method URL; `token` is embedded as `/bot<token>/`.
fn api_url(base_url: &str, token: &str, method: &str) -> String {
    format!("{base_url}/bot{token}/{method}")
}

/// Rejects tokens that cannot be valid `<bot id>:<secret>` strings upfront.
fn validate_token_shape(token: &str) -> Result<()> {
    let Some((bot_id, secret)) = token.split_once(':') else {
        return Err(VaultError::Transport(
            "Telegram bot token must look like <bot id>:<secret>".into(),
        ));
    };
    if bot_id.is_empty() || !bot_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(VaultError::Transport(
            "Telegram bot token identifier must be numeric".into(),
        ));
    }
    if secret.is_empty()
        || !secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(VaultError::Transport(
            "Telegram bot token secret has an unexpected shape".into(),
        ));
    }
    Ok(())
}

/// Envelope of every Bot API JSON response.
#[derive(Debug, Deserialize)]
struct BotResponse<T> {
    /// Whether the method succeeded.
    #[serde(default)]
    ok: bool,
    /// Human-readable failure reason, present on failures.
    description: Option<String>,
    /// Method payload, present on successes.
    result: Option<T>,
}

/// `result` object of `getMe`.
#[derive(Debug, Deserialize)]
struct BotUser {
    /// Public username, absent for user-scoped identities.
    #[serde(default)]
    username: Option<String>,
}

/// Chat referenced by a Bot API message.
#[derive(Debug, Deserialize)]
struct BotChat {
    /// Numeric chat identifier.
    id: i64,
    /// Chat type such as `private` or `group`.
    #[serde(rename = "type")]
    kind: String,
}

/// `result` message of `sendDocument`.
#[derive(Debug, Deserialize)]
struct BotMessage {
    /// Message identifier assigned by Telegram.
    message_id: i64,
    /// Destination chat of the message.
    chat: BotChat,
}

/// One `getUpdates` entry.
#[derive(Debug, Deserialize)]
struct BotUpdate {
    /// Monotonically increasing update identifier.
    update_id: i64,
    /// Message payload; other update kinds carry different fields.
    message: Option<BotUpdateMessage>,
}

/// Message body of a `getUpdates` entry.
#[derive(Debug, Deserialize)]
struct BotUpdateMessage {
    /// Chat the message was sent to.
    chat: BotChat,
    /// Message text (expected `/start <nonce>` during pairing).
    #[serde(default)]
    text: Option<String>,
}

/// Parsed page of a `getUpdates` response.
#[derive(Debug, Default)]
struct UpdatesPage {
    /// Highest observed update identifier, for the next `offset`.
    max_update_id: Option<i64>,
    /// Chat identifier of the first matching `/start <nonce>` private chat message.
    private_chat_id: Option<i64>,
}

/// Parses a `getMe` response body into the bot's public username.
fn parse_get_me_response(bytes: &[u8]) -> Result<BotInfo> {
    let parsed: BotResponse<BotUser> = serde_json::from_slice(bytes)
        .map_err(|error| VaultError::Transport(format!("unrecognized getMe response: {error}")))?;
    let user = require_ok(parsed, "getMe")?;
    let username = user
        .username
        .as_deref()
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .ok_or_else(|| {
            VaultError::Transport("the bot has no public @username to link against".into())
        })?;
    Ok(BotInfo {
        username: username.trim_start_matches('@').to_owned(),
    })
}

/// Parses a `sendDocument` response body into the sent message reference.
fn parse_send_document_response(bytes: &[u8]) -> Result<RemoteRef> {
    let parsed: BotResponse<BotMessage> = serde_json::from_slice(bytes).map_err(|error| {
        VaultError::Transport(format!("unrecognized sendDocument response: {error}"))
    })?;
    let message = require_ok(parsed, "sendDocument")?;
    Ok(RemoteRef {
        peer_id: message.chat.id.to_string(),
        message_id: message.message_id,
    })
}

/// Parses a `getUpdates` response body, keeping the offset anchor and any
/// private chat matching `/start <expected_nonce>`.
fn parse_updates_response(bytes: &[u8], expected_nonce: &str) -> Result<UpdatesPage> {
    let parsed: BotResponse<Vec<BotUpdate>> = serde_json::from_slice(bytes).map_err(|error| {
        VaultError::Transport(format!("unrecognized getUpdates response: {error}"))
    })?;
    let updates = require_ok(parsed, "getUpdates")?;
    let mut page = UpdatesPage::default();
    for update in updates {
        page.max_update_id = Some(match page.max_update_id {
            Some(current) if current >= update.update_id => current,
            _ => update.update_id,
        });
        if let Some(message) = update.message {
            if message.chat.kind == "private"
                && page.private_chat_id.is_none()
                && matches_start_nonce(message.text.as_deref(), expected_nonce)
            {
                page.private_chat_id = Some(message.chat.id);
            }
        }
    }
    Ok(page)
}

fn validate_pair_nonce(nonce: &str) -> Result<()> {
    if nonce.is_empty()
        || nonce.len() > 64
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(VaultError::Transport("invalid bot pairing nonce".into()));
    }
    Ok(())
}

fn matches_start_nonce(text: Option<&str>, expected_nonce: &str) -> bool {
    let Some(text) = text else {
        return false;
    };
    let mut parts = text.split_whitespace();
    let Some(command) = parts.next() else {
        return false;
    };
    let Some(nonce) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    if command != "/start" && !command.starts_with("/start@") {
        return false;
    }
    constant_time_eq(nonce.as_bytes(), expected_nonce.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (&left, &right) in a.iter().zip(b.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn os_entropy_bytes() -> [u8; 32] {
    #[cfg(unix)]
    if let Ok(mut file) = fs::File::open("/dev/urandom") {
        let mut buf = [0_u8; 32];
        if file.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    fallback_entropy_bytes()
}

fn fallback_entropy_bytes() -> [u8; 32] {
    let pair = std::thread::spawn(|| {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let mut first = RandomState::new().build_hasher();
        first.write_u8(0x01);
        let mut second = RandomState::new().build_hasher();
        second.write_u8(0x02);
        (first.finish(), second.finish())
    })
    .join()
    .unwrap_or((0, 0));
    let mut hasher = Sha256::new();
    hasher.update(pair.0.to_le_bytes());
    hasher.update(pair.1.to_le_bytes());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    hasher.update(nanos.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.finalize().into()
}

fn is_timeout_ureq_error(error: &ureq::Error) -> bool {
    let ureq::Error::Transport(transport) = error else {
        return false;
    };
    let mut source: Option<&dyn std::error::Error> = Some(transport);
    while let Some(err) = source {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            if matches!(
                io_err.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                return true;
            }
        }
        source = err.source();
    }
    false
}

/// Unwraps the `ok` envelope, mapping failures to the API `description`.
fn require_ok<T>(parsed: BotResponse<T>, method: &str) -> Result<T> {
    if !parsed.ok {
        return Err(VaultError::Transport(parsed.description.unwrap_or_else(
            || format!("Telegram Bot API {method} returned ok=false without a description"),
        )));
    }
    parsed.result.ok_or_else(|| {
        VaultError::Transport(format!(
            "Telegram Bot API {method} response is missing its result"
        ))
    })
}

/// Reads a completed response into bytes with token redaction on failures.
fn response_body(response: ureq::Response, token: &str) -> Result<Vec<u8>> {
    let body = response.into_string().map_err(|error| {
        VaultError::Transport(redact_token(
            &format!("reading Bot API response failed: {error}"),
            token,
        ))
    })?;
    Ok(body.into_bytes())
}

/// Maps an already-executed Bot API result onto its redacted response body.
///
/// HTTP errors still carry the API JSON body; its `description` becomes the
/// error detail. Every message is token-redacted before it leaves this module.
fn into_body(
    result: std::result::Result<ureq::Response, ureq::Error>,
    token: &str,
) -> Result<Vec<u8>> {
    match result {
        Ok(response) => response_body(response, token),
        Err(error) => Err(bot_api_error(error, token)),
    }
}

/// Maps a `ureq` failure onto a token-redacted [`VaultError::Transport`].
fn bot_api_error(error: ureq::Error, token: &str) -> VaultError {
    let description = match error {
        ureq::Error::Status(_, response) => response
            .into_string()
            .ok()
            .and_then(|body| body_description(&body))
            .unwrap_or_else(|| "Telegram Bot API rejected the request".to_owned()),
        ureq::Error::Transport(inner) => inner.to_string(),
    };
    VaultError::Transport(redact_token(&description, token))
}

/// Extracts `description` from an `ok:false` API body, if that shape parses.
fn body_description(body: &str) -> Option<String> {
    match serde_json::from_str::<BotResponse<serde_json::Value>>(body) {
        Ok(parsed) if !parsed.ok => parsed.description,
        _ => None,
    }
}

/// Removes the token from any message that might embed the request URL.
fn redact_token(message: &str, token: &str) -> String {
    if token.is_empty() {
        message.to_owned()
    } else {
        message.replace(token, "***")
    }
}

/// Replaces characters that would break the `filename` header parameter.
fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            '"' | '\\' | '\r' | '\n' => '_',
            _ => character,
        })
        .collect()
}

/// Derives a per-upload multipart boundary from the digest and current time.
fn upload_boundary(sha256: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(sha256.as_bytes());
    hasher.update(nanos.to_le_bytes());
    format!("dimvault{:x}", hasher.finalize())
}

/// Builds the opening multipart part headers for the document file.
fn multipart_head(boundary: &str, file_name: &str) -> Cursor<Vec<u8>> {
    Cursor::new(
        format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"document\"; filename=\"{file_name}\"\r\n\
             Content-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes(),
    )
}

/// Builds the `chat_id`, caption parts, and the closing boundary.
fn multipart_tail(boundary: &str, chat_id: i64, caption: &str) -> Cursor<Vec<u8>> {
    Cursor::new(
        format!(
            "\r\n--{boundary}\r\n\
             Content-Disposition: form-data; name=\"chat_id\"\r\n\r\n\
             {chat_id}\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"caption\"\r\n\r\n\
             {caption}\r\n\
             --{boundary}--\r\n"
        )
        .into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn token_shape_accepts_official_shape_and_rejects_garbage() {
        assert!(validate_token_shape("123456789:AAE_abc-DEFxyz0123456789").is_ok());
        assert!(validate_token_shape("").is_err());
        assert!(validate_token_shape("not-a-token").is_err());
        assert!(validate_token_shape("abc:def").is_err());
        assert!(validate_token_shape("123:").is_err());
        assert!(validate_token_shape(":secret").is_err());
    }

    #[test]
    fn api_url_embeds_token_and_method() {
        assert_eq!(
            api_url("https://api.telegram.org", "12:ab", "getMe"),
            "https://api.telegram.org/bot12:ab/getMe"
        );
        assert_eq!(
            api_url("http://127.0.0.1:8081", "12:ab", "sendDocument"),
            "http://127.0.0.1:8081/bot12:ab/sendDocument"
        );
    }

    #[test]
    fn get_me_response_yields_username() {
        let body = br#"{"ok":true,"result":{"id":42,"is_bot":true,"first_name":"Vault","username":"vault_bot"}}"#;
        assert_eq!(
            parse_get_me_response(body).unwrap(),
            BotInfo {
                username: "vault_bot".into()
            }
        );
        assert_eq!(
            parse_get_me_response(
                br#"{"ok":true,"result":{"id":42,"is_bot":true,"first_name":"Vault","username":"@vault_bot"}}"#
            )
            .unwrap()
            .username,
            "vault_bot"
        );
    }

    #[test]
    fn ok_false_bodies_map_to_their_description() {
        let error =
            parse_get_me_response(br#"{"ok":false,"error_code":401,"description":"Unauthorized"}"#)
                .unwrap_err();
        assert!(error.to_string().contains("Unauthorized"));
        let error = parse_send_document_response(
            br#"{"ok":false,"error_code":400,"description":"chat not found"}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("chat not found"));
    }

    #[test]
    fn send_document_response_maps_to_remote_ref() {
        let body = br#"{"ok":true,"result":{"message_id":777,"from":{"id":42,"is_bot":true,"first_name":"Vault"},"chat":{"id":987654321,"first_name":"Owner","type":"private"},"date":1700000000,"document":{"file_name":"a.bin","mime_type":"application/octet-stream","file_id":"X"}}}"#;
        let reference = parse_send_document_response(body).unwrap();
        assert_eq!(reference.message_id, 777);
        assert_eq!(reference.peer_id, "987654321");
    }

    #[test]
    fn updates_response_requires_matching_private_start_nonce_and_advances_offset() {
        let body = br#"{"ok":true,"result":[
            {"update_id":10,"message":{"message_id":1,"from":{"id":1,"is_bot":false,"first_name":"Attacker"},"chat":{"id":111,"first_name":"A","type":"private"},"date":1700000000,"text":"/start"}},
            {"update_id":11,"message":{"message_id":2,"from":{"id":2,"is_bot":false,"first_name":"Wrong"},"chat":{"id":222,"first_name":"W","type":"private"},"date":1700000001,"text":"/start stale_nonce"}},
            {"update_id":12,"message":{"message_id":3,"from":{"id":3,"is_bot":false,"first_name":"Group"},"chat":{"id":-100200,"title":"Group","type":"supergroup"},"date":1700000002,"text":"/start good_nonce_42"}},
            {"update_id":13,"message":{"message_id":4,"from":{"id":4,"is_bot":false,"first_name":"Owner"},"chat":{"id":987654321,"first_name":"O","type":"private"},"date":1700000003,"text":"/start good_nonce_42"}}
        ]}"#;
        let page = parse_updates_response(body, "good_nonce_42").unwrap();
        assert_eq!(page.private_chat_id, Some(987_654_321));
        assert_eq!(page.max_update_id, Some(13));

        // Without the matching nonce, stale/attacker/group messages never match,
        // while max_update_id still advances past them.
        let uncaptured = parse_updates_response(body, "different_nonce").unwrap();
        assert_eq!(uncaptured.private_chat_id, None);
        assert_eq!(uncaptured.max_update_id, Some(13));

        let empty = parse_updates_response(br#"{"ok":true,"result":[]}"#, "good_nonce_42").unwrap();
        assert_eq!(empty.private_chat_id, None);
        assert_eq!(empty.max_update_id, None);
    }

    #[test]
    fn generate_pair_nonce_produces_unique_telegram_start_tokens() {
        let first = BotTransport::generate_pair_nonce();
        let second = BotTransport::generate_pair_nonce();
        assert_ne!(first, second);
        assert!(validate_pair_nonce(&first).is_ok());
        assert!(validate_pair_nonce(&second).is_ok());
        assert!(first.len() >= 24);
    }

    fn read_http_request_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
        use std::io::Read;

        let mut raw = Vec::new();
        let mut buf = [0_u8; 4096];
        let header_end;
        loop {
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0, "unexpected EOF reading request headers");
            raw.extend_from_slice(&buf[..n]);
            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = pos + 4;
                break;
            }
        }
        let headers = String::from_utf8_lossy(&raw[..header_end]);
        let is_chunked = headers.lines().any(|line| {
            line.split_once(':').is_some_and(|(k, v)| {
                k.trim().eq_ignore_ascii_case("transfer-encoding")
                    && v.to_ascii_lowercase().contains("chunked")
            })
        });
        if is_chunked {
            let mut decoded = Vec::new();
            let mut pos = header_end;
            loop {
                let line_rel = loop {
                    if let Some(rel) = raw[pos..].windows(2).position(|w| w == b"\r\n") {
                        break rel;
                    }
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0, "unexpected EOF reading chunk size");
                    raw.extend_from_slice(&buf[..n]);
                };
                let size_line = std::str::from_utf8(&raw[pos..pos + line_rel]).unwrap();
                let size_hex = size_line.split(';').next().unwrap_or("").trim();
                let chunk_size = usize::from_str_radix(size_hex, 16).unwrap();
                pos += line_rel + 2;
                if chunk_size == 0 {
                    while raw.len() < pos + 2 {
                        let n = stream.read(&mut buf).unwrap();
                        assert!(n > 0, "unexpected EOF reading chunk terminator");
                        raw.extend_from_slice(&buf[..n]);
                    }
                    if &raw[pos..pos + 2] != b"\r\n" {
                        while raw[pos..]
                            .windows(4)
                            .position(|w| w == b"\r\n\r\n")
                            .is_none()
                        {
                            let n = stream.read(&mut buf).unwrap();
                            assert!(n > 0, "unexpected EOF reading chunk trailers");
                            raw.extend_from_slice(&buf[..n]);
                        }
                    }
                    break;
                }
                while raw.len() < pos + chunk_size + 2 {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0, "unexpected EOF reading chunk payload");
                    raw.extend_from_slice(&buf[..n]);
                }
                decoded.extend_from_slice(&raw[pos..pos + chunk_size]);
                pos += chunk_size + 2;
            }
            return decoded;
        }
        let content_len = headers
            .lines()
            .find_map(|line| {
                let (k, v) = line.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| v.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        while raw.len() < header_end + content_len {
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0, "unexpected EOF reading request body");
            raw.extend_from_slice(&buf[..n]);
        }
        raw[header_end..header_end + content_len].to_vec()
    }

    fn write_json_response(stream: &mut std::net::TcpStream, payload: &str) {
        use std::io::Write;

        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        stream.write_all(resp.as_bytes()).unwrap();
        stream.flush().unwrap();
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }

    #[test]
    fn wait_for_chat_advances_offset_past_stale_updates_until_matching_nonce() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let nonce = "pair_nonce_abc123";
        let server = std::thread::spawn(move || {
            let mut seen_offsets = Vec::new();
            for step in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let body = read_http_request_body(&mut stream);
                let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
                seen_offsets.push(parsed.get("offset").and_then(|v| v.as_i64()));
                let payload = if step == 0 {
                    r#"{"ok":true,"result":[{"update_id":100,"message":{"chat":{"id":555,"type":"private"},"text":"/start wrong"}}]}"#.to_owned()
                } else {
                    format!(
                        r#"{{"ok":true,"result":[{{"update_id":101,"message":{{"chat":{{"id":777888,"type":"private"}},"text":"/start {nonce}"}}}}]}}"#
                    )
                };
                write_json_response(&mut stream, &payload);
            }
            seen_offsets
        });

        let base_url = format!("http://{addr}");
        let chat_id =
            BotTransport::wait_for_chat_against(&base_url, "123456:valid_secret", nonce, 5)
                .unwrap();
        assert_eq!(chat_id, 777_888);
        let offsets = server.join().unwrap();
        assert_eq!(offsets, vec![None, Some(101)]);
    }

    #[test]
    fn multipart_upload_sends_chat_id_over_http() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let body = read_http_request_body(&mut stream);
            let payload =
                r#"{"ok":true,"result":{"message_id":99,"chat":{"id":4242,"type":"private"}}}"#;
            write_json_response(&mut stream, payload);
            String::from_utf8_lossy(&body).into_owned()
        });

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("doc.txt");
        std::fs::write(&path, b"hello-vault").unwrap();
        let digest = crate::watch::sha256_file(&path).unwrap();
        let transport =
            BotTransport::new("123456:valid_secret", 4242).with_base_url(&format!("http://{addr}"));
        let remote = transport
            .upload_document("work_1", &path, &digest, "{\"sha256\":\"x\"}")
            .unwrap();
        assert_eq!(remote.message_id, 99);
        assert_eq!(remote.peer_id, "4242");

        let captured = server.join().unwrap();
        assert!(captured.contains("name=\"chat_id\"\r\n\r\n4242\r\n"));
        assert!(!captured.contains("name=\"chat\"\r\n"));
        assert!(captured.contains("hello-vault"));
    }

    #[test]
    fn transport_errors_never_contain_the_token() {
        let token = "123456789:AAE_abc-DEFxyz0123456789";
        let transport = BotTransport::new(token, 42)
            // Port 9 refuses connections immediately; no network round trip.
            .with_base_url("http://127.0.0.1:9");
        let file = tempfile::tempdir().unwrap();
        let path = file.path().join("upload.bin");
        std::fs::write(&path, b"payload").unwrap();
        let digest = crate::watch::sha256_file(&path).unwrap();
        let error = transport
            .upload_document(
                "profile-a",
                &path,
                &digest,
                "{\"file_name\":\"upload.bin\"}",
            )
            .unwrap_err();
        let detail = error.to_string();
        assert!(!detail.contains(token), "token leaked: {detail}");
    }

    #[test]
    fn multipart_parts_frame_the_document_chat_id_and_caption() {
        let boundary = "dimvault0";
        let head = String::from_utf8(multipart_head(boundary, "a.bin").into_inner()).unwrap();
        assert!(head.contains("name=\"document\"; filename=\"a.bin\""));
        assert!(head.starts_with("--dimvault0\r\n"));
        let tail = String::from_utf8(
            multipart_tail(boundary, 987_654_321, "{\"sha256\":\"x\"}").into_inner(),
        )
        .unwrap();
        assert!(tail.contains("name=\"chat_id\"\r\n\r\n987654321\r\n"));
        assert!(tail.contains("name=\"caption\"\r\n\r\n{\"sha256\":\"x\"}"));
        assert!(tail.ends_with("--dimvault0--\r\n"));
    }

    #[test]
    fn bot_upload_size_accepts_limit_and_rejects_larger_files() {
        assert!(validate_bot_upload_size(50_000_000).is_ok());
        assert!(validate_bot_upload_size(0).is_ok());
        let error = validate_bot_upload_size(50_000_001).unwrap_err();
        assert!(matches!(
            error,
            VaultError::Transport(message)
                if message == "file exceeds the Bot API 50 MB limit; use the full Telegram account tier for large files"
        ));
    }

    #[test]
    fn file_names_with_quotes_are_sanitized() {
        assert_eq!(sanitize_file_name("re\"port\\v2\n.txt"), "re_port_v2_.txt");
    }
}
