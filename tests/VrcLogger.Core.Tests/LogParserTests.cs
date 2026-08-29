using VrcLogger.Core.Parsing;

namespace VrcLogger.Core.Tests;

public class LogParserTests
{
    private static ParseResult Parse(string log)
    {
        using var reader = new StringReader(log);
        return LogParser.Parse(reader);
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
}
