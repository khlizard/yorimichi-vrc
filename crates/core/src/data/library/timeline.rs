//! Day timeline, calendar activity, "on this day" and statistics.
use super::*;

impl LibraryService<'_> {
    // ---- timeline ----------------------------------------------------------------------

    /// One logical day. `show_first_met = false` forces every `first_met` to false; reunion
    /// badges follow `reunion_days` (0 disables them). Always returns a day, possibly empty.
    pub fn timeline_day(
        &self,
        day: NaiveDate,
        day_start_hour: u32,
        reunion_days: i32,
        show_first_met: bool,
    ) -> Result<TimelineDay> {
        let (start, end) = window(day, day_start_hour);
        let earliest = earliest_join(day, day_start_hour);
        let secs = secs_sql("v.join_time", "v.leave_time");
        let sql = format!(
            r"SELECT v.id, v.world_name, w.world_id, v.world_id, coalesce(n.favorite, 0), coalesce(n.memo, ''),
                     v.instance_id, v.location, v.join_time, v.leave_time, {secs}
              FROM visits v LEFT JOIN worlds w ON w.id = v.world_id LEFT JOIN world_notes n ON n.world_id = v.world_id
              WHERE v.join_time >= @b AND v.join_time < @e
              ORDER BY v.join_time, v.id"
        );
        let mut visits = query_rows(self.conn, &sql, &[("@b", &earliest), ("@e", &end)], |r| {
            let join_time: String = r.get(8)?;
            let leave_time: String = r.get(9)?;
            Ok(TimelineVisit {
                visit_id: r.get(0)?,
                world_name: r.get(1)?,
                world_id: r.get(2)?,
                world_ref_id: r.get(3)?,
                world_favorite: r.get(4)?,
                world_memo: r.get(5)?,
                instance_id: r.get(6)?,
                location: r.get(7)?,
                duration: format_duration(&join_time, &leave_time),
                join_time,
                leave_time,
                seconds: r.get(10)?,
                carried_over: false,
                players: Vec::new(),
                photos: Vec::new(),
            })
        })?;
        // Keep the visits of this day: those joined in it, and one from before that was still
        // going when it began. Only the part inside the day counts toward its total.
        let mut total_seconds = 0;
        visits.retain_mut(|v| match seconds_on(&v.join_time, &v.leave_time, day, day_start_hour) {
            Some(secs) => {
                v.carried_over = v.join_time < start;
                total_seconds += secs;
                true
            }
            None => false,
        });

        let mut people: HashSet<i64> = HashSet::new();
        for v in &mut visits {
            v.players = self.visit_people(v.visit_id, &v.join_time, reunion_days)?;
            if !show_first_met {
                for p in &mut v.players {
                    p.first_met = false;
                }
            }
            people.extend(v.players.iter().map(|p| p.player_id));
        }

        // A photo belongs to the day its timestamp falls in: either to a visit of this day it is
        // linked to, or (no visit / visit of another day) to the loose list.
        let mut loose_photos = Vec::new();
        for photo in self.window_photos(&start, &end)? {
            match photo.visit_id.and_then(|id| visits.iter_mut().find(|v| v.visit_id == id)) {
                Some(v) => v.photos.push(photo),
                None => loose_photos.push(photo),
            }
        }

        let photo_count = (loose_photos.len() + visits.iter().map(|v| v.photos.len()).sum::<usize>()) as i64;
        Ok(TimelineDay {
            day: day.format("%Y-%m-%d").to_string(),
            total_seconds,
            visit_count: visits.len() as i64,
            people_count: people.len() as i64,
            photo_count,
            visits,
            loose_photos,
            start,
            end,
        })
    }

    /// Non-missing photos taken in `[start, end)`, ascending.
    fn window_photos(&self, start: &str, end: &str) -> Result<Vec<PhotoRow>> {
        let sql = format!(
            "{} WHERE p.missing = 0 AND p.taken_at >= @s AND p.taken_at < @e ORDER BY p.taken_at, p.id",
            select_photo()
        );
        Ok(query_rows(self.conn, &sql, &[("@s", &start), ("@e", &end)], photo_row)?)
    }

    /// Per-day totals in `[from, to]` (logical days, inclusive); only days with a visit or a
    /// non-missing photo, ascending. A visit counts on every day it was in (see [`visit_days`]),
    /// with its time split at the day starts; people are those met in the day's visits.
    pub fn activity(&self, from: NaiveDate, to: NaiveDate, day_start_hour: u32) -> Result<Vec<DayActivity>> {
        let (start, _) = window(from, day_start_hour);
        let (_, end) = window(to, day_start_hour);
        let earliest = earliest_join(from, day_start_hour);
        let mut days: BTreeMap<String, DayActivity> = BTreeMap::new();

        let visit_rows = query_rows(
            self.conn,
            "SELECT id, join_time, leave_time FROM visits WHERE join_time >= @b AND join_time < @e",
            &[("@b", &earliest), ("@e", &end)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )?;
        let mut days_of_visit: HashMap<i64, Vec<String>> = HashMap::new();
        for (id, join, leave) in visit_rows {
            for (d, secs) in visit_days(&join, &leave, day_start_hour) {
                if d < from || d > to {
                    continue;
                }
                let key = d.format("%Y-%m-%d").to_string();
                let a = days.entry(key.clone()).or_insert_with(|| empty_day(key.clone()));
                a.seconds += secs;
                a.visit_count += 1;
                days_of_visit.entry(id).or_default().push(key);
            }
        }
        let mut people: HashMap<String, HashSet<i64>> = HashMap::new();
        for (visit, players) in self.players_by_visit(&earliest, &end)? {
            for d in days_of_visit.get(&visit).into_iter().flatten() {
                people.entry(d.clone()).or_default().extend(&players);
            }
        }
        for (d, set) in people {
            if let Some(a) = days.get_mut(&d) {
                a.people_count = set.len() as i64;
            }
        }

        let photo_rows = query_rows(
            self.conn,
            &format!(
                "SELECT {}, count(*) FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e GROUP BY 1",
                day_expr("taken_at", day_start_hour)
            ),
            &[("@s", &start), ("@e", &end)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (day, n) in photo_rows {
            days.entry(day.clone()).or_insert_with(|| empty_day(day)).photo_count = n;
        }
        Ok(days.into_values().collect())
    }

    /// Nearest logical day after (`forward`) or before `day` that has a visit or a non-missing photo.
    pub fn adjacent_active_day(&self, day: NaiveDate, day_start_hour: u32, forward: bool) -> Result<Option<String>> {
        let (start, end) = window(day, day_start_hour);
        let d = hour(day_start_hour);
        let sql = if forward {
            format!(
                r"SELECT date(min(t), '-{d} hours') FROM (
                    SELECT min(join_time) AS t FROM visits WHERE join_time >= @b
                    UNION ALL SELECT min(taken_at) FROM photos WHERE missing = 0 AND taken_at >= @b)"
            )
        } else {
            format!(
                r"SELECT date(max(t), '-{d} hours') FROM (
                    SELECT max(join_time) AS t FROM visits WHERE join_time < @b
                    UNION ALL SELECT max(taken_at) FROM photos WHERE missing = 0 AND taken_at < @b)"
            )
        };
        let bound = if forward { &end } else { &start };
        Ok(self.conn.query_row(&sql, rusqlite::named_params! {"@b": bound}, |r| r.get(0))?)
    }

    /// The most recent logical day with a visit or a non-missing photo.
    pub fn latest_active_day(&self, day_start_hour: u32) -> Result<Option<String>> {
        Ok(self.conn.query_row(
            &format!(
                r"SELECT date(max(t), '-{} hours') FROM (
                    SELECT max(join_time) AS t FROM visits
                    UNION ALL SELECT max(taken_at) FROM photos WHERE missing = 0)",
                hour(day_start_hour)
            ),
            [],
            |r| r.get(0),
        )?)
    }

    /// Calendar years that have visits or non-missing photos, newest first.
    pub fn years(&self) -> Result<Vec<i32>> {
        Ok(query_rows(
            self.conn,
            r"SELECT y FROM (
                SELECT CAST(substr(join_time, 1, 4) AS INTEGER) AS y FROM visits
                UNION SELECT CAST(substr(taken_at, 1, 4) AS INTEGER) FROM photos WHERE missing = 0)
              WHERE y > 0 ORDER BY y DESC",
            &[],
            |r| r.get(0),
        )?)
    }

    /// The same month/day in each earlier year with activity, newest year first.
    /// Feb 29 only matches leap years.
    pub fn on_this_day(&self, day: NaiveDate, day_start_hour: u32) -> Result<Vec<MemoryYear>> {
        let Some(&oldest) = self.years()?.last() else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        // One year below the oldest calendar year: a logical day can start on Dec 31.
        for year in ((oldest - 1)..day.year()).rev() {
            let Some(d) = NaiveDate::from_ymd_opt(year, day.month(), day.day()) else { continue };
            if let Some(m) = self.memory_year(d, day_start_hour)? {
                out.push(m);
            }
        }
        Ok(out)
    }

    fn memory_year(&self, day: NaiveDate, h: u32) -> Result<Option<MemoryYear>> {
        let (start, end) = window(day, h);
        let earliest = earliest_join(day, h);
        let rows: Vec<(i64, String, String, String)> = query_rows(
            self.conn,
            "SELECT id, world_name, join_time, leave_time FROM visits WHERE join_time >= @b AND join_time < @e ORDER BY join_time, id",
            &[("@b", &earliest), ("@e", &end)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let (ids, names): (HashSet<i64>, Vec<String>) = rows
            .into_iter()
            .filter(|(_, _, join, leave)| seconds_on(join, leave, day, h).is_some())
            .map(|(id, name, _, _)| (id, name))
            .unzip();
        let photos = self.window_photos(&start, &end)?;
        if names.is_empty() && photos.is_empty() {
            return Ok(None);
        }
        let people = self
            .players_by_visit(&earliest, &end)?
            .into_iter()
            .filter(|(visit, _)| ids.contains(visit))
            .flat_map(|(_, players)| players)
            .collect::<HashSet<i64>>()
            .len() as i64;
        let mut worlds: Vec<String> = Vec::new();
        for n in &names {
            if worlds.len() < 5 && !worlds.contains(n) {
                worlds.push(n.clone());
            }
        }
        Ok(Some(MemoryYear {
            year: day.year(),
            day: day.format("%Y-%m-%d").to_string(),
            visit_count: names.len() as i64,
            people_count: people,
            photo_count: photos.len() as i64,
            worlds,
            photos: pick_memory_photos(photos, 8),
        }))
    }

    /// Non-local players met in each visit joined in `[from, to)`.
    fn players_by_visit(&self, from: &str, to: &str) -> Result<HashMap<i64, Vec<i64>>> {
        let rows = query_rows(
            self.conn,
            r"SELECT DISTINCT e.visit_id, e.player_id
              FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players p ON p.id = e.player_id
              WHERE p.is_local = 0 AND v.join_time >= @b AND v.join_time < @e",
            &[("@b", &from), ("@e", &to)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        for (visit, player) in rows {
            out.entry(visit).or_default().push(player);
        }
        Ok(out)
    }

    // ---- stats -------------------------------------------------------------------------

    /// Totals over logical days `from..=to` (open ends allowed); top lists by seconds, non-local only.
    pub fn stats(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        day_start_hour: u32,
        top_n: usize,
    ) -> Result<StatsSummary> {
        let (start, end) = bounds(from, to, day_start_hour);
        let limit = top_n as i64;
        let p: &[(&str, &dyn rusqlite::ToSql)] = &[("@s", &start), ("@e", &end), ("@n", &limit)];
        let vsecs = secs_sql("v.join_time", "v.leave_time");
        let esecs = secs_sql("e.join_time", "e.leave_time");
        let in_v = "v.join_time >= @s AND v.join_time < @e";

        let (total_seconds, visit_count, world_count): (i64, i64, i64) = query_rows(
            self.conn,
            &format!("SELECT coalesce(sum({vsecs}), 0), count(*), count(DISTINCT v.world_id) FROM visits v WHERE {in_v}"),
            p,
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?[0];
        let people_count: i64 = query_rows(
            self.conn,
            &format!(
                r"SELECT count(DISTINCT e.player_id)
                  FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players pl ON pl.id = e.player_id
                  WHERE pl.is_local = 0 AND {in_v}"
            ),
            p,
            |r| r.get(0),
        )?[0];
        let photo_count: i64 = query_rows(
            self.conn,
            "SELECT count(*) FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e",
            p,
            |r| r.get(0),
        )?[0];
        let active_days: i64 = query_rows(
            self.conn,
            &format!(
                r"SELECT count(*) FROM (
                    SELECT {} AS d FROM visits v WHERE {in_v}
                    UNION SELECT {} FROM photos WHERE missing = 0 AND taken_at >= @s AND taken_at < @e)",
                day_expr("v.join_time", day_start_hour),
                day_expr("taken_at", day_start_hour)
            ),
            p,
            |r| r.get(0),
        )?[0];
        let top_players = query_rows(
            self.conn,
            &format!(
                r"SELECT pl.id, pl.user_id, pl.display_name, sum({esecs}) AS secs, count(DISTINCT e.visit_id)
                  FROM encounters e JOIN visits v ON v.id = e.visit_id JOIN players pl ON pl.id = e.player_id
                  WHERE pl.is_local = 0 AND {in_v}
                  GROUP BY pl.id ORDER BY secs DESC, pl.id LIMIT @n"
            ),
            p,
            |r| {
                Ok(TopPlayer {
                    player_id: r.get(0)?,
                    user_id: r.get(1)?,
                    display_name: r.get(2)?,
                    seconds: r.get(3)?,
                    count: r.get(4)?,
                })
            },
        )?;
        let top_worlds = query_rows(
            self.conn,
            &format!(
                r"SELECT v.world_id, coalesce(w.name, v.world_name), sum({vsecs}) AS secs, count(*)
                  FROM visits v LEFT JOIN worlds w ON w.id = v.world_id
                  WHERE {in_v}
                  GROUP BY coalesce(CAST(v.world_id AS TEXT), 'n:' || v.world_name)
                  ORDER BY secs DESC, 1 LIMIT @n"
            ),
            p,
            |r| Ok(TopWorld { world_ref_id: r.get(0)?, name: r.get(1)?, seconds: r.get(2)?, count: r.get(3)? }),
        )?;
        Ok(StatsSummary {
            total_seconds,
            visit_count,
            world_count,
            people_count,
            photo_count,
            active_days,
            top_players,
            top_worlds,
        })
    }
}

fn empty_day(day: String) -> DayActivity {
    DayActivity { day, seconds: 0, visit_count: 0, people_count: 0, photo_count: 0 }
}

fn day_start(day: NaiveDate, h: u32) -> NaiveDateTime {
    day.and_hms_opt(hour(h), 0, 0).expect("hour is clamped to 0..=23")
}

/// Joins before this cannot reach logical day `day`: a visit counts for at most [`MAX_SECONDS`].
fn earliest_join(day: NaiveDate, h: u32) -> String {
    db::ts(day_start(day, h) - Duration::seconds(MAX_SECONDS))
}

/// The logical days a visit counts on, each with the seconds spent in it: the day it was joined,
/// then every following day it was still in the world when that day began (a night that runs
/// past the day start also opens the next day). The visit is capped at [`MAX_SECONDS`] like
/// [`secs_sql`]; a join time that does not parse gives no days.
fn visit_days(join: &str, leave: &str, h: u32) -> Vec<(NaiveDate, i64)> {
    let Ok(join) = NaiveDateTime::parse_from_str(join, TS_FORMAT) else { return Vec::new() };
    let leave = NaiveDateTime::parse_from_str(leave, TS_FORMAT)
        .unwrap_or(join)
        .clamp(join, join + Duration::seconds(MAX_SECONDS));
    let mut day = (join - Duration::hours(i64::from(hour(h)))).date();
    let mut from = join;
    let mut out = Vec::new();
    loop {
        let next = day_start(day, h) + Duration::days(1);
        out.push((day, (leave.min(next) - from).num_seconds()));
        if leave <= next {
            return out;
        }
        from = next;
        day = next.date();
    }
}

/// Seconds a visit spent in logical day `day`, or `None` when it does not count on that day.
fn seconds_on(join: &str, leave: &str, day: NaiveDate, h: u32) -> Option<i64> {
    visit_days(join, leave, h).into_iter().find(|(d, _)| *d == day).map(|(_, secs)| secs)
}

/// Favorites first (time order), then the others spread evenly over the day, up to `max` in total.
fn pick_memory_photos(photos: Vec<PhotoRow>, max: usize) -> Vec<PhotoRow> {
    let (mut picked, rest): (Vec<PhotoRow>, Vec<PhotoRow>) = photos.into_iter().partition(|p| p.favorite);
    picked.truncate(max);
    let slots = max - picked.len();
    if slots > 0 && !rest.is_empty() {
        let n = rest.len();
        if n <= slots {
            picked.extend(rest);
        } else {
            let keep: HashSet<usize> = (0..slots).map(|i| i * n / slots).collect();
            picked.extend(rest.into_iter().enumerate().filter(|(i, _)| keep.contains(i)).map(|(_, p)| p));
        }
    }
    picked
}
