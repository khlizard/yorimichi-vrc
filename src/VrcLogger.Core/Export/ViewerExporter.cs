using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Unicode;
using Microsoft.Data.Sqlite;
using VrcLogger.Core.Data;

namespace VrcLogger.Core.Export;

/// <summary>Writes a self-contained HTML viewer with the encounter data embedded as JSON.</summary>
public static class ViewerExporter
{
    public const string Placeholder = "/*__VRCLOG_DATA__*/null";
    private const string ResourceName = "VrcLogger.Core.Export.viewer.html";

    private static readonly JsonWriterOptions WriterOptions = new()
    {
        Indented = false,
        Encoder = JavaScriptEncoder.Create(UnicodeRanges.All),
    };

    public static void Export(SqliteConnection conn, string outputPath, DateTime now)
    {
        var template = LoadTemplate();
        var json = BuildJson(conn, now);
        var html = template.Replace(Placeholder, json, StringComparison.Ordinal);

        var dir = Path.GetDirectoryName(Path.GetFullPath(outputPath));
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);

        var tmp = outputPath + ".tmp";
        try
        {
            File.WriteAllText(tmp, html, new UTF8Encoding(false));
            File.Move(tmp, outputPath, overwrite: true);
        }
        catch
        {
            try { if (File.Exists(tmp)) File.Delete(tmp); } catch { /* best effort */ }
            throw;
        }
    }

    private static string LoadTemplate()
    {
        var asm = typeof(ViewerExporter).Assembly;
        using var stream = asm.GetManifestResourceStream(ResourceName)
            ?? throw new InvalidOperationException($"Embedded resource '{ResourceName}' was not found.");
        using var reader = new StreamReader(stream, Encoding.UTF8);
        var template = reader.ReadToEnd();

        var first = template.IndexOf(Placeholder, StringComparison.Ordinal);
        if (first < 0 || template.IndexOf(Placeholder, first + Placeholder.Length, StringComparison.Ordinal) >= 0)
            throw new InvalidOperationException(
                $"Embedded resource '{ResourceName}' must contain the placeholder '{Placeholder}' exactly once.");
        return template;
    }

    public static string BuildJson(SqliteConnection conn, DateTime now)
    {
        using var ms = new MemoryStream();
        using (var w = new Utf8JsonWriter(ms, WriterOptions))
        {
            w.WriteStartObject();
            w.WriteString("generatedAt", Db.Ts(now));

            WriteArray(conn, w, "players",
                """
                SELECT p.id, p.user_id, p.display_name, p.is_local,
                       (SELECT group_concat(pn.display_name, ', ') FROM player_names pn
                         WHERE pn.player_id = p.id AND pn.display_name <> p.display_name)
                FROM players p ORDER BY p.id
                """);
            WriteArray(conn, w, "visits",
                "SELECT id, world_name, instance_id, join_time, leave_time, ended_cleanly FROM visits ORDER BY join_time, id");
            WriteArray(conn, w, "enc",
                "SELECT visit_id, player_id, display_name, join_time, leave_time FROM encounters ORDER BY join_time, id");
            WriteArray(conn, w, "notif",
                "SELECT event_time, kind, sender_name, message, details, accepted_at FROM notifications ORDER BY event_time, id");

            w.WriteEndObject();
        }
        return Encoding.UTF8.GetString(ms.ToArray());
    }

    private static void WriteArray(SqliteConnection conn, Utf8JsonWriter w, string name, string sql)
    {
        w.WriteStartArray(name);
        using var cmd = conn.CreateCommand();
        cmd.CommandText = sql;
        using var r = cmd.ExecuteReader();
        while (r.Read())
        {
            w.WriteStartArray();
            for (var i = 0; i < r.FieldCount; i++)
            {
                if (r.IsDBNull(i)) { w.WriteNullValue(); continue; }
                switch (r.GetValue(i))
                {
                    case long l: w.WriteNumberValue(l); break;
                    case int n: w.WriteNumberValue(n); break;
                    case double d: w.WriteNumberValue(d); break;
                    default: w.WriteStringValue(Convert.ToString(r.GetValue(i))); break;
                }
            }
            w.WriteEndArray();
        }
        w.WriteEndArray();
    }
}
