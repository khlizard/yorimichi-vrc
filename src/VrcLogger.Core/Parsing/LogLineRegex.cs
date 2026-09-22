using System.Text.RegularExpressions;

namespace VrcLogger.Core.Parsing;

public static partial class LogLineRegex
{
    [GeneratedRegex(@"^(\d{4}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2}) (\w+)\s+-\s{2}(.*)$")]
    public static partial Regex LinePrefix();

    [GeneratedRegex(@"^\[Behaviour\] Entering Room: (.*)$")]
    public static partial Regex EnteringRoom();

    [GeneratedRegex(@"^\[Behaviour\] Joining (wrld_[0-9a-fA-F\-]+):(\S+)$")]
    public static partial Regex JoiningWorld();

    // Trailing " (usr_...)$" is anchored so greedy (.*) leaves the LAST usr group
    // even when the display name itself contains parentheses.
    [GeneratedRegex(@"^\[Behaviour\] OnPlayerJoined (.*) \((usr_[0-9a-fA-F\-]+)\)$")]
    public static partial Regex PlayerJoined();

    // Requires a space after OnPlayerLeft so "OnPlayerLeftRoom" does not match.
    [GeneratedRegex(@"^\[Behaviour\] OnPlayerLeft (.*) \((usr_[0-9a-fA-F\-]+)\)$")]
    public static partial Regex PlayerLeft();

    [GeneratedRegex(@"^\[Behaviour\] OnLeftRoom$")]
    public static partial Regex LeftRoom();

    [GeneratedRegex(@"^User Authenticated: (.*) \((usr_[0-9a-fA-F\-]+)\)")]
    public static partial Regex Authenticated();

    [GeneratedRegex(@"^\[Video Playback\] (?:Attempting to resolve|Resolving) URL '(.*)'$")]
    public static partial Regex VideoUrl();

    [GeneratedRegex(@"^\[String Download\] Attempting to load String from URL '(.*)'$")]
    public static partial Regex StringUrl();

    [GeneratedRegex(@"^\[Image Download\] Attempting to load image from URL '(.*)'$")]
    public static partial Regex ImageUrl();

    // Received Notification: <Notification from username:NAME, sender user id:usr_… to usr_… of type: TYPE,
    //   id: not_…, created at: MM/DD/YYYY HH:MM:SS UTC, details: {{k=v, …}}, type:TYPE, m seen:False, message: "TEXT">
    // The message may span lines; the parser is line based so only the first line is captured.
    [GeneratedRegex(@"^Received Notification: <Notification from username:(.*?), sender user id:(\S*) to (\S*) of type: ([^,]+), id: ([^,]+), created at: ([^,]*?)(?: UTC)?, details: \{\{(.*?)\}\}, type:[^,]*, m seen:\w+, message: ""(.*)$")]
    public static partial Regex Notification();

    [GeneratedRegex(@"^(?:Accept|Answer)Notification for notification:<Notification .*?, id: ([^,]+),")]
    public static partial Regex NotificationAnswered();

    // [ModerationManager] NAME is now Muted / is no longer Muted / is now Blocked …
    [GeneratedRegex(@"^\[ModerationManager\] (.*) is (now|no longer) (\w+)$")]
    public static partial Regex Moderation();

    [GeneratedRegex(@"^\[ModerationManager\] A vote kick has been initiated against (.*), do you agree\?$")]
    public static partial Regex VoteKick();

    // [Behaviour] Switching NAME to avatar AVATAR
    [GeneratedRegex(@"^\[Behaviour\] Switching (.*) to avatar (.*)$")]
    public static partial Regex AvatarSwitch();

    // [AssetBundleDownloadManager] Download for avatar (Worn:0 Dist:9.03) (44.2 MB) started 0 seconds and completed 3 seconds after queueing.
    [GeneratedRegex(@"^\[AssetBundleDownloadManager\] Download for avatar \([^)]*\) \(([\d.]+) MB\) started (-?\d+) seconds and completed (-?\d+) seconds")]
    public static partial Regex AvatarDownload();

    // [AssetBundleDownloadManager] [114] Unpacking Avatar (AVATAR by NAME)
    // Greedy first group: avatar names like "Dress by Shop" are common, so split at the LAST " by ".
    [GeneratedRegex(@"^\[AssetBundleDownloadManager\] \[\d+\] Unpacking Avatar \((.*) by (.*)\)$")]
    public static partial Regex AvatarUnpack();
}
