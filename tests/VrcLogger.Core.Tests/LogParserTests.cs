using VrcLogger.Core.Parsing;

namespace VrcLogger.Core.Tests;

public class LogParserTests
{
    private static ParseResult Parse(string log, ParseOptions? options = null)
    {
        using var reader = new StringReader(log);
        return LogParser.Parse(reader, options);
    }

    private const string P = "Debug      -  ";

    [Fact]
    public void ParsesBasicVisitWithPlayers()
    {
        var result = Parse($"""
            2026.06.23 21:38:17 {P}User Authenticated: テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
            2026.06.23 21:38:23 {P}[Behaviour] Entering Room: Sample World
            2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:91278~private(usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)~region(jp)
            2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
            2026.06.23 21:40:46 {P}[Behaviour] OnPlayerLeft テスト／Tester (usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa)
            2026.06.23 21:40:46 {P}[Behaviour] OnLeftRoom
            """);

        Assert.Equal("usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", result.LocalUserId);
        Assert.Equal("テスト／Tester", result.LocalUserName);
        var visit = Assert.Single(result.Visits);
        Assert.Equal("Sample World", visit.WorldName);
        Assert.Equal("wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", visit.WorldId);
        Assert.StartsWith("91278~private", visit.InstanceId);
        Assert.True(visit.EndedCleanly);
        Assert.Equal(new DateTime(2026, 6, 23, 21, 38, 23), visit.JoinTime);
        Assert.Equal(new DateTime(2026, 6, 23, 21, 40, 46), visit.LeaveTime);
        var enc = Assert.Single(visit.Encounters);
        Assert.Equal("テスト／Tester", enc.DisplayName);
        Assert.True(enc.EndedCleanly);
    }

    [Fact]
    public void PlayerLeftRoomLineIsNotAPlayerLeft()
    {
        var result = Parse($"""
            2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
            2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined A B (C) (usr_11111111-1111-1111-1111-111111111111)
            2026.06.23 21:49:59 {P}[Behaviour] OnPlayerLeftRoom
            2026.06.23 21:50:00 {P}[Behaviour] OnLeftRoom
            """);

        var enc = Assert.Single(result.Visits.Single().Encounters);
        Assert.Equal("A B (C)", enc.DisplayName); // name with spaces and parens, last (usr_...) wins
        Assert.Equal(new DateTime(2026, 6, 23, 21, 50, 0), enc.LeaveTime); // closed by OnLeftRoom, not OnPlayerLeftRoom
    }

    [Fact]
    public void ContinuationLinesAreSkipped()
    {
        var result = Parse($"""
            2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
            2026.06.23 22:08:53 Error      -  HTTP/1.1 404 Not Found
            Cysharp.Threading.Tasks.UnityWebRequestException: HTTP/1.1 404 Not Found
              at VRC.DataModel.Textures.NativeTextureDownloader.DownloadTexture (...)
            2026.06.23 22:09:00 {P}[Behaviour] OnLeftRoom
            """);

        Assert.Single(result.Visits);
        // Error line and continuation lines are excluded from raw events.
        Assert.Equal(2, result.RawEvents.Count);
        Assert.DoesNotContain(result.RawEvents, e => e.Message.Contains("404"));
    }

    [Fact]
    public void CrashEofClosesVisitAndEncountersUncleanly()
    {
        var result = Parse($"""
            2026.06.23 21:38:23 {P}[Behaviour] Joining wrld_bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb:1
            2026.06.23 21:38:33 {P}[Behaviour] OnPlayerJoined X (usr_11111111-1111-1111-1111-111111111111)
            2026.06.23 23:00:00 {P}some later line
            """);

        var visit = Assert.Single(result.Visits);
        Assert.False(visit.EndedCleanly);
        Assert.Equal(new DateTime(2026, 6, 23, 23, 0, 0), visit.LeaveTime);
        var enc = Assert.Single(visit.Encounters);
        Assert.False(enc.EndedCleanly);
        Assert.Equal(new DateTime(2026, 6, 23, 23, 0, 0), enc.LeaveTime);
    }

    [Fact]
    public void BackToBackJoiningClosesPreviousVisit()
    {
        var result = Parse($"""
            2026.06.23 21:00:00 {P}[Behaviour] Entering Room: First
            2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 22:00:00 {P}[Behaviour] Entering Room: Second
            2026.06.23 22:00:00 {P}[Behaviour] Joining wrld_22222222-2222-2222-2222-222222222222:2
            2026.06.23 23:00:00 {P}[Behaviour] OnLeftRoom
            """);

        Assert.Equal(2, result.Visits.Count);
        Assert.False(result.Visits[0].EndedCleanly);
        Assert.Equal(new DateTime(2026, 6, 23, 22, 0, 0), result.Visits[0].LeaveTime);
        Assert.Equal("Second", result.Visits[1].WorldName);
        Assert.True(result.Visits[1].EndedCleanly);
    }

    [Fact]
    public void UrlEventsAreDedupedWithinWindow()
    {
        var result = Parse($"""
            2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:43:55 {P}[Video Playback] Resolving URL 'rtspt://example.com/live/stream'
            2026.06.23 21:43:57 {P}[Video Playback] Attempting to resolve URL 'rtspt://example.com/live/stream'
            2026.06.23 21:45:37 {P}[Video Playback] Resolving URL 'rtspt://example.com/live/stream'
            2026.06.23 21:46:00 {P}[String Download] Attempting to load String from URL 'https://example.com/data.json'
            2026.06.23 21:46:05 {P}[Image Download] Attempting to load image from URL 'https://example.com/img.png'
            2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
            """);

        var urls = result.Visits.Single().UrlEvents;
        Assert.Equal(4, urls.Count);
        Assert.Equal(2, urls.Count(u => u.Kind == "video")); // 21:43:55 kept, :57 deduped, 21:45:37 far enough
        Assert.Single(urls, u => u.Kind == "string");
        Assert.Single(urls, u => u.Kind == "image");
    }

    [Fact]
    public void RawEventsLinkToCurrentVisit()
    {
        var result = Parse($"""
            2026.06.23 20:59:00 {P}before any visit
            2026.06.23 21:00:00 {P}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:30:00 {P}[Behaviour] Pickup object 'Pen'
            2026.06.23 22:00:00 {P}[Behaviour] OnLeftRoom
            2026.06.23 22:01:00 {P}after leaving
            """);

        Assert.Null(result.RawEvents[0].VisitIndex);
        Assert.Equal(0, result.RawEvents[1].VisitIndex); // Joining line belongs to the new visit
        Assert.Equal(0, result.RawEvents[2].VisitIndex);
        Assert.Equal(0, result.RawEvents[3].VisitIndex); // OnLeftRoom belongs to the visit it ends
        Assert.Null(result.RawEvents[4].VisitIndex);
    }

    [Fact]
    public void ParsesCommunicationNotificationsAndSkipsGroupBulletins()
    {
        var result = Parse($$$"""
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:39:00 {{{P}}}Received Notification: <Notification from username:Friend, A, sender user id:usr_11111111-1111-1111-1111-111111111111 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: invite, id: not_11111111-1111-1111-1111-111111111111, created at: 06/23/2026 12:39:00 UTC, details: {{worldId=wrld_22222222-2222-2222-2222-222222222222:1~region(jp), worldName=Some World}}, type:invite, m seen:False, message: "">
            2026.06.23 21:39:00 {{{P}}}Received Notification: <Notification from username:Friend B, sender user id:usr_22222222-2222-2222-2222-222222222222 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: boop, id: not_22222222-2222-2222-2222-222222222222, created at: 06/23/2026 12:39:00 UTC, details: {{emojiId=default_heart, emojiVersion=, inventoryItemId=}}, type:boop, m seen:False, message: "Friend B Booped You!">
            2026.06.23 21:39:00 {{{P}}}Received Notification: <Notification from username:Friend C, sender user id:usr_22222222-2222-2222-2222-222222222222 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: friendRequest, id: frq_33333333-3333-3333-3333-333333333333, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:friendRequest, m seen:False, message: "">
            2026.06.23 21:39:00 {{{P}}}Received Notification: <Notification from username:, sender user id: to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: group, id: not_44444444-4444-4444-4444-444444444444, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:group, m seen:False, message: "Group bulletin text">
            2026.06.23 21:40:00 {{{P}}}AcceptNotification for notification:<Notification from username:Friend, A, sender user id:usr_11111111-1111-1111-1111-111111111111 to usr_aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa of type: invite, id: not_11111111-1111-1111-1111-111111111111, created at: 06/23/2026 12:39:00 UTC, details: {{}}, type:invite, m seen:False, message: "">
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            """);

        Assert.Equal(3, result.Notifications.Count);
        Assert.DoesNotContain(result.Notifications, n => n.Kind == "group");

        var invite = result.Notifications.Single(n => n.Kind == "invite");
        Assert.Equal("not_11111111-1111-1111-1111-111111111111", invite.NotificationId);
        Assert.Equal("Friend, A", invite.SenderName); // comma in the name is not a field separator
        Assert.Equal("usr_11111111-1111-1111-1111-111111111111", invite.SenderUserId);
        Assert.Contains("worldName=Some World", invite.Details);
        Assert.Equal("", invite.Message);
        Assert.Equal("06/23/2026 12:39:00", invite.CreatedAtUtc);
        Assert.Equal(new DateTime(2026, 6, 23, 21, 39, 0), invite.Time);

        var boop = result.Notifications.Single(n => n.Kind == "boop");
        Assert.Equal("Friend B Booped You!", boop.Message); // trailing "> stripped

        Assert.Equal("frq_33333333-3333-3333-3333-333333333333", result.Notifications.Single(n => n.Kind == "friendRequest").NotificationId);

        var answered = Assert.Single(result.AnsweredNotifications);
        Assert.Equal("not_11111111-1111-1111-1111-111111111111", answered.NotificationId);
        Assert.Equal(new DateTime(2026, 6, 23, 21, 40, 0), answered.Time);

        // Notification lines are still kept in raw events (they are not on the noise list).
        Assert.Equal(4, result.RawEvents.Count(r => r.Message.StartsWith("Received Notification")));
    }

    [Fact]
    public void ParsesModerationEvents()
    {
        var result = Parse($$$"""
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:39:00 {{{P}}}[ModerationManager] Some One is now Muted
            2026.06.23 21:39:30 {{{P}}}[ModerationManager] Some One is no longer Muted
            2026.06.23 21:40:00 {{{P}}}[ModerationManager] Bad Actor is now Blocked
            2026.06.23 21:41:00 {{{P}}}[ModerationManager] A vote kick has been initiated against Bad Actor, do you agree?
            2026.06.23 21:42:00 {{{P}}}[ModerationManager] Some One Avatar will follow Safety Settings
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            2026.06.23 22:01:00 {{{P}}}[ModerationManager] Outside Visit is no longer Blocked
            """);

        Assert.Equal(
            [("mute", "Some One"), ("unmute", "Some One"), ("block", "Bad Actor"), ("votekick", "Bad Actor"), ("unblock", "Outside Visit")],
            result.ModerationEvents.Select(m => (m.Kind, m.TargetName)).ToList());
        Assert.All(result.ModerationEvents.Take(4), m => Assert.Equal(0, m.VisitIndex));
        Assert.Null(result.ModerationEvents[^1].VisitIndex);
        // "Avatar will follow Safety Settings" is noise: neither a moderation event nor a raw event.
        Assert.DoesNotContain(result.RawEvents, r => r.Message.Contains("Safety Settings"));
    }

    [Fact]
    public void NoisePatternsAreDroppedFromRawEventsButUdonOutputIsKept()
    {
        var result = Parse($$$"""
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:38:30 {{{P}}}[Behaviour] Using default gesture mask (hands)
            2026.06.23 21:38:31 {{{P}}}[Behaviour] CacheComponents: ParticleSystems 0, AudioSources 0
            2026.06.23 21:38:32 {{{P}}}Missing button from radio selector group
            2026.06.23 21:38:33 {{{P}}}[EOSManager] EOS Initialized Successfully
            2026.06.23 21:38:34 {{{P}}}MyWorldScript: player entered trigger zone 3
            2026.06.23 21:38:35 {{{P}}}[Udon] custom log line
            2026.06.23 21:38:36 {{{P}}}[Behaviour] Switching Some One to avatar Cool Avatar
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            """);

        var kept = result.RawEvents.Select(r => r.Message).ToList();
        Assert.Equal(
        [
            "[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1",
            "MyWorldScript: player entered trigger zone 3",
            "[Udon] custom log line",
            "[Behaviour] Switching Some One to avatar Cool Avatar",
            "[Behaviour] OnLeftRoom",
        ], kept);
    }

    [Fact]
    public void CustomNoisePatternsOverrideDefaults()
    {
        var options = new ParseOptions(["[Udon]", "*trigger zone"]);
        var result = Parse($$$"""
            2026.06.23 21:38:23 {{{P}}}[Behaviour] Joining wrld_11111111-1111-1111-1111-111111111111:1
            2026.06.23 21:38:30 {{{P}}}[Behaviour] Using default gesture mask (hands)
            2026.06.23 21:38:34 {{{P}}}MyWorldScript: player entered trigger zone 3
            2026.06.23 21:38:35 {{{P}}}[Udon] custom log line
            2026.06.23 22:00:00 {{{P}}}[Behaviour] OnLeftRoom
            """, options);

        var kept = result.RawEvents.Select(r => r.Message).ToList();
        Assert.Contains("[Behaviour] Using default gesture mask (hands)", kept); // default list not applied
        Assert.DoesNotContain(kept, m => m.StartsWith("[Udon]"));
        Assert.DoesNotContain(kept, m => m.Contains("trigger zone"));
    }
}
