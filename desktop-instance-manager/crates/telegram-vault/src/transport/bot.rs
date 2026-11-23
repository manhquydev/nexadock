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

    /// Long-polls `getUpdates` until the bot receives a private chat message.
    ///
    /// Updates are consumed by advancing the poll offset. Therefore, a second
    /// pairing that uses the same bot token will not see messages already
    /// consumed by the first pairing; each bot token is single-owner and must
    /// be dedicated to one profile.
    ///
    /// Uses a monotonically increasing `offset` so updates are consumed once,
    /// waits at most `min(timeout_secs, 60)` seconds in total, and returns the
    /// chat identifier of the first private conversation. The wait elapsing
    /// without a message yields [`VaultError::Timeout`].
    pub fn wait_for_chat(token: &str, timeout_secs: u64) -> Result<i64> {
        let token = token.trim();
        validate_token_shape(token)?;
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
            let poll_secs = remaining.as_secs().min(GET_UPDATES_LONG_POLL_SECS);
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
                .post(&api_url(TELEGRAM_API_BASE_URL, token, "getUpdates"))
                .timeout(Duration::from_secs(
                    poll_secs.saturating_add(GET_UPDATES_SLACK_SECS),
                ));
            let bytes = into_body(request.send_json(serde_json::Value::Object(body)), token)?;
            let page = parse_updates_response(&bytes)?;
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
        let size = fs::metadata(path)
            .map_err(|error| VaultError::io(path, error))?
            .len();
        validate_bot_upload_size(size)?;
        // Same source-mutation guard as the MTProto transport: never upload a
        // file that no longer matches its queued digest.
        let actual_sha256 = crate::watch::sha256_file(path)?;
        if actual_sha256 != sha256 {
            return Err(VaultError::Transport(format!(
                "source file changed after enqueue: {}",
                path.display()
            )));
        }
        let file = fs::File::open(path).map_err(|error| VaultError::io(path, error))?;
        let file_name = sanitize_file_name(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file"),
        );
        let boundary = upload_boundary(sha256);
        let body = multipart_head(&boundary, &file_name)
            .chain(file)
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
}

/// Parsed page of a `getUpdates` response.
#[derive(Debug, Default)]
struct UpdatesPage {
    /// Highest observed update identifier, for the next `offset`.
    max_update_id: Option<i64>,
    /// Chat identifier of the first observed private chat message.
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
/// private chat identifier.
fn parse_updates_response(bytes: &[u8]) -> Result<UpdatesPage> {
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
            if message.chat.kind == "private" && page.private_chat_id.is_none() {
                page.private_chat_id = Some(message.chat.id);
            }
        }
    }
    Ok(page)
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

/// Builds the chat, caption parts, and the closing boundary.
fn multipart_tail(boundary: &str, chat_id: i64, caption: &str) -> Cursor<Vec<u8>> {
    Cursor::new(
        format!(
            "\r\n--{boundary}\r\n\
             Content-Disposition: form-data; name=\"chat\"\r\n\r\n\
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
    fn updates_response_finds_private_chat_and_offset_anchor() {
        let body = br#"{"ok":true,"result":[
            {"update_id":10,"message":{"message_id":1,"from":{"id":1,"is_bot":false,"first_name":"O"},"chat":{"id":987654321,"first_name":"O","type":"private"},"date":1700000000,"text":"/start"}},
            {"update_id":11,"message":{"message_id":2,"from":{"id":2,"is_bot":false,"first_name":"G"},"chat":{"id":-100200,"title":"Group","type":"supergroup"},"date":1700000001,"text":"hi"}}
        ]}"#;
        let page = parse_updates_response(body).unwrap();
        assert_eq!(page.private_chat_id, Some(987_654_321));
        assert_eq!(page.max_update_id, Some(11));
        let empty = parse_updates_response(br#"{"ok":true,"result":[]}"#).unwrap();
        assert_eq!(empty.private_chat_id, None);
        assert_eq!(empty.max_update_id, None);
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
    fn multipart_parts_frame_the_document_chat_and_caption() {
        let boundary = "dimvault0";
        let head = String::from_utf8(multipart_head(boundary, "a.bin").into_inner()).unwrap();
        assert!(head.contains("name=\"document\"; filename=\"a.bin\""));
        assert!(head.starts_with("--dimvault0\r\n"));
        let tail = String::from_utf8(
            multipart_tail(boundary, 987_654_321, "{\"sha256\":\"x\"}").into_inner(),
        )
        .unwrap();
        assert!(tail.contains("name=\"chat\"\r\n\r\n987654321\r\n"));
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
