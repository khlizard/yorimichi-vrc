//! Compiled line patterns. Port of `LogLineRegex.cs` (the C# `""` in verbatim strings is a single `"`).

use std::sync::LazyLock;

use ::regex::Regex;

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid regex")
}

pub static LINE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\d{4}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2}) (\w+)\s+-\s{2}(.*)$"));

pub static ENTERING_ROOM: LazyLock<Regex> = LazyLock::new(|| re(r"^\[Behaviour\] Entering Room: (.*)$"));

pub static JOINING_WORLD: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[Behaviour\] Joining (wrld_[0-9a-fA-F\-]+):(\S+)$"));

// Trailing " (usr_...)$" is anchored so greedy (.*) leaves the LAST usr group
// even when the display name itself contains parentheses.
pub static PLAYER_JOINED: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[Behaviour\] OnPlayerJoined (.*) \((usr_[0-9a-fA-F\-]+)\)$"));

// Requires a space after OnPlayerLeft so "OnPlayerLeftRoom" does not match.
pub static PLAYER_LEFT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[Behaviour\] OnPlayerLeft (.*) \((usr_[0-9a-fA-F\-]+)\)$"));

pub static LEFT_ROOM: LazyLock<Regex> = LazyLock::new(|| re(r"^\[Behaviour\] OnLeftRoom$"));

pub static AUTHENTICATED: LazyLock<Regex> =
    LazyLock::new(|| re(r"^User Authenticated: (.*) \((usr_[0-9a-fA-F\-]+)\)"));

pub static VIDEO_URL: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[Video Playback\] (?:Attempting to resolve|Resolving) URL '(.*)'$"));

pub static STRING_URL: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[String Download\] Attempting to load String from URL '(.*)'$"));

pub static IMAGE_URL: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[Image Download\] Attempting to load image from URL '(.*)'$"));

// Received Notification: <Notification from username:NAME, sender user id:usr_... to usr_... of type: TYPE,
//   id: not_..., created at: MM/DD/YYYY HH:MM:SS UTC, details: {{k=v, ...}}, type:TYPE, m seen:False, message: "TEXT">
// The message may span lines; the parser is line based so only the first line is captured.
pub static NOTIFICATION: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"^Received Notification: <Notification from username:(.*?), sender user id:(\S*) to (\S*) of type: ([^,]+), id: ([^,]+), created at: ([^,]*?)(?: UTC)?, details: \{\{(.*?)\}\}, type:[^,]*, m seen:\w+, message: "(.*)$"#)
});

pub static NOTIFICATION_ANSWERED: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(?:Accept|Answer)Notification for notification:<Notification .*?, id: ([^,]+),"));

// [ModerationManager] NAME is now Muted / is no longer Muted / is now Blocked ...
pub static MODERATION: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[ModerationManager\] (.*) is (now|no longer) (\w+)$"));

pub static VOTE_KICK: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^\[ModerationManager\] A vote kick has been initiated against (.*), do you agree\?$")
});

// [Behaviour] Switching NAME to avatar AVATAR
pub static AVATAR_SWITCH: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[Behaviour\] Switching (.*) to avatar (.*)$"));

// [AssetBundleDownloadManager] Download for avatar (Worn:0 Dist:9.03) (44.2 MB) started 0 seconds and completed 3 seconds after queueing.
pub static AVATAR_DOWNLOAD: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^\[AssetBundleDownloadManager\] Download for avatar \([^)]*\) \(([\d.]+) MB\) started (-?\d+) seconds and completed (-?\d+) seconds")
});

// [AssetBundleDownloadManager] [114] Unpacking Avatar (AVATAR by NAME)
// Greedy first group: avatar names like "Dress by Shop" are common, so split at the LAST " by ".
pub static AVATAR_UNPACK: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\[AssetBundleDownloadManager\] \[\d+\] Unpacking Avatar \((.*) by (.*)\)$"));
