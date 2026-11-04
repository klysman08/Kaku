//! Local SQLite database: usage metrics (always) and text history (optional).

use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::Path;

pub struct Db {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct UsageRecord<'a> {
    pub ts: i64,
    pub action: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
    pub ok: bool,
    pub error_code: Option<String>,
    /// "popup" | "shortcut" | "cli"
    pub source: &'a str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub model: String,
    pub target_language: Option<String>,
    pub input: String,
    pub output: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub requests: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub avg_latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayBucket {
    pub day: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyBucket {
    pub key: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub totals: Totals,
    pub by_day: Vec<DayBucket>,
    pub by_action: Vec<KeyBucket>,
    pub by_model: Vec<KeyBucket>,
}

type R<T> = rusqlite::Result<T>;

impl Db {
    pub fn open(path: &Path) -> R<Self> {
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn memory() -> R<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> R<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS usage (
               id INTEGER PRIMARY KEY,
               ts INTEGER NOT NULL,
               action TEXT NOT NULL,
               provider TEXT NOT NULL,
               model TEXT NOT NULL,
               input_tokens INTEGER NOT NULL DEFAULT 0,
               output_tokens INTEGER NOT NULL DEFAULT 0,
               latency_ms INTEGER NOT NULL DEFAULT 0,
               ok INTEGER NOT NULL,
               error_code TEXT,
               source TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS usage_ts ON usage(ts);
             CREATE TABLE IF NOT EXISTS history (
               id INTEGER PRIMARY KEY,
               ts INTEGER NOT NULL,
               action TEXT NOT NULL,
               model TEXT NOT NULL,
               target_language TEXT,
               input TEXT NOT NULL,
               output TEXT NOT NULL,
               input_tokens INTEGER NOT NULL DEFAULT 0,
               output_tokens INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS history_ts ON history(ts);",
        )?;
        Ok(Self { conn })
    }

    pub fn record_usage(&self, r: &UsageRecord) -> R<()> {
        self.conn.execute(
            "INSERT INTO usage (ts, action, provider, model, input_tokens, output_tokens, latency_ms, ok, error_code, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                r.ts,
                r.action,
                r.provider,
                r.model,
                r.input_tokens as i64,
                r.output_tokens as i64,
                r.latency_ms as i64,
                r.ok,
                r.error_code,
                r.source
            ],
        )?;
        Ok(())
    }

    /// Aggregates for the last `days` days (0 = all time). `tz_offset_secs` groups by local day.
    pub fn usage_summary(&self, now: i64, days: u32, tz_offset_secs: i64) -> R<UsageSummary> {
        let since = if days == 0 {
            0
        } else {
            now - i64::from(days) * 86_400
        };
        let totals = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(1 - ok), 0), COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0), COALESCE(AVG(CASE WHEN ok = 1 THEN latency_ms END), 0)
             FROM usage WHERE ts >= ?1",
            [since],
            |r| {
                Ok(Totals {
                    requests: r.get::<_, i64>(0)? as u64,
                    errors: r.get::<_, i64>(1)? as u64,
                    input_tokens: r.get::<_, i64>(2)? as u64,
                    output_tokens: r.get::<_, i64>(3)? as u64,
                    avg_latency_ms: r.get::<_, f64>(4)?.round() as u64,
                })
            },
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT date(ts + ?2, 'unixepoch') AS d, COUNT(*), SUM(input_tokens), SUM(output_tokens)
             FROM usage WHERE ts >= ?1 GROUP BY d ORDER BY d",
        )?;
        let by_day = stmt
            .query_map(params![since, tz_offset_secs], |r| {
                Ok(DayBucket {
                    day: r.get(0)?,
                    requests: r.get::<_, i64>(1)? as u64,
                    input_tokens: r.get::<_, i64>(2)? as u64,
                    output_tokens: r.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<R<Vec<_>>>()?;

        let group = |col: &str| -> R<Vec<KeyBucket>> {
            let sql = format!(
                "SELECT {col}, COUNT(*), SUM(input_tokens), SUM(output_tokens)
                 FROM usage WHERE ts >= ?1 AND ok = 1 GROUP BY {col} ORDER BY COUNT(*) DESC"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            stmt.query_map([since], |r| {
                Ok(KeyBucket {
                    key: r.get(0)?,
                    requests: r.get::<_, i64>(1)? as u64,
                    input_tokens: r.get::<_, i64>(2)? as u64,
                    output_tokens: r.get::<_, i64>(3)? as u64,
                })
            })?
            .collect()
        };

        Ok(UsageSummary {
            totals,
            by_day,
            by_action: group("action")?,
            by_model: group("model")?,
        })
    }

    pub fn reset_usage(&self) -> R<()> {
        self.conn.execute("DELETE FROM usage", [])?;
        Ok(())
    }

    pub fn add_history(&self, item: &HistoryItem) -> R<i64> {
        self.conn.execute(
            "INSERT INTO history (ts, action, model, target_language, input, output, input_tokens, output_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                item.ts,
                item.action,
                item.model,
                item.target_language,
                item.input,
                item.output,
                item.input_tokens as i64,
                item.output_tokens as i64
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Newest first. `query` matches input, output or action (case-insensitive).
    pub fn list_history(&self, query: &str, limit: u32, offset: u32) -> R<(Vec<HistoryItem>, u64)> {
        let pattern = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
        let filter = "WHERE input LIKE ?1 ESCAPE '\\' OR output LIKE ?1 ESCAPE '\\' OR action LIKE ?1 ESCAPE '\\'";
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM history {filter}"),
            [&pattern],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, ts, action, model, target_language, input, output, input_tokens, output_tokens
             FROM history {filter} ORDER BY ts DESC, id DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let items = stmt
            .query_map(params![pattern, limit, offset], row_to_history)?
            .collect::<R<Vec<_>>>()?;
        Ok((items, total as u64))
    }

    pub fn all_history(&self) -> R<Vec<HistoryItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ts, action, model, target_language, input, output, input_tokens, output_tokens
             FROM history ORDER BY ts DESC, id DESC",
        )?;
        stmt.query_map([], row_to_history)?.collect()
    }

    #[cfg(test)]
    pub fn last_history(&self) -> R<Option<HistoryItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ts, action, model, target_language, input, output, input_tokens, output_tokens
             FROM history ORDER BY ts DESC, id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], row_to_history)?;
        rows.next().transpose()
    }

    pub fn delete_history(&self, id: i64) -> R<()> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_history(&self) -> R<()> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Remove history older than `days` (0 = keep forever).
    pub fn prune_history(&self, now: i64, days: u32) -> R<usize> {
        if days == 0 {
            return Ok(0);
        }
        self.conn.execute(
            "DELETE FROM history WHERE ts < ?1",
            [now - i64::from(days) * 86_400],
        )
    }
}

fn row_to_history(r: &rusqlite::Row) -> R<HistoryItem> {
    Ok(HistoryItem {
        id: r.get(0)?,
        ts: r.get(1)?,
        action: r.get(2)?,
        model: r.get(3)?,
        target_language: r.get(4)?,
        input: r.get(5)?,
        output: r.get(6)?,
        input_tokens: r.get::<_, i64>(7)? as u64,
        output_tokens: r.get::<_, i64>(8)? as u64,
    })
}

/// RFC 4180 CSV with a UTF-8 BOM so Excel opens accents correctly.
pub fn history_to_csv(items: &[HistoryItem]) -> String {
    fn esc(s: &str) -> String {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }
    let mut out = String::from(
        "\u{feff}timestamp,action,model,target_language,input_tokens,output_tokens,input,output\r\n",
    );
    for i in items {
        let ts = chrono::DateTime::from_timestamp(i.ts, 0)
            .map(|d| d.to_rfc3339())
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\r\n",
            ts,
            esc(crate::actions::label(&i.action)),
            esc(&i.model),
            esc(i.target_language.as_deref().unwrap_or("")),
            i.input_tokens,
            i.output_tokens,
            esc(&i.input),
            esc(&i.output)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(ts: i64, action: &'static str, ok: bool) -> UsageRecord<'static> {
        UsageRecord {
            ts,
            action,
            provider: "gemini",
            model: "gemini-3.7-flash",
            input_tokens: 10,
            output_tokens: 5,
            latency_ms: 100,
            ok,
            error_code: (!ok).then(|| "429".into()),
            source: "shortcut",
        }
    }

    fn hist(ts: i64, input: &str) -> HistoryItem {
        HistoryItem {
            id: 0,
            ts,
            action: "review".into(),
            model: "m".into(),
            target_language: None,
            input: input.into(),
            output: format!("{input}!"),
            input_tokens: 1,
            output_tokens: 1,
        }
    }

    #[test]
    fn summarizes_usage() {
        let db = Db::memory().unwrap();
        let now = 1_800_000_000;
        db.record_usage(&usage(now - 10, "review", true)).unwrap();
        db.record_usage(&usage(now - 20, "review", true)).unwrap();
        db.record_usage(&usage(now - 30, "translate", false))
            .unwrap();
        db.record_usage(&usage(now - 40 * 86_400, "rewrite", true))
            .unwrap();

        let s = db.usage_summary(now, 30, 0).unwrap();
        assert_eq!(s.totals.requests, 3);
        assert_eq!(s.totals.errors, 1);
        assert_eq!(s.totals.input_tokens, 30);
        assert_eq!(s.by_action[0].key, "review");
        assert_eq!(
            s.by_action.len(),
            1,
            "failed requests are excluded from breakdowns"
        );
        assert_eq!(s.by_day.len(), 1);

        assert_eq!(db.usage_summary(now, 0, 0).unwrap().totals.requests, 4);
        db.reset_usage().unwrap();
        assert_eq!(db.usage_summary(now, 0, 0).unwrap().totals.requests, 0);
    }

    #[test]
    fn history_search_paging_and_prune() {
        let db = Db::memory().unwrap();
        let now = 1_800_000_000;
        db.add_history(&hist(now - 1, "hello world")).unwrap();
        db.add_history(&hist(now - 2, "olá mundo")).unwrap();
        db.add_history(&hist(now - 100 * 86_400, "old 100%"))
            .unwrap();

        let (items, total) = db.list_history("", 2, 0).unwrap();
        assert_eq!(total, 3);
        assert_eq!(items[0].input, "hello world");
        assert_eq!(db.list_history("MUNDO", 10, 0).unwrap().1, 1);
        assert_eq!(db.list_history("100%", 10, 0).unwrap().1, 1);
        assert_eq!(db.list_history("%", 10, 0).unwrap().1, 1, "% is literal");

        assert_eq!(db.prune_history(now, 30).unwrap(), 1);
        let id = db.last_history().unwrap().unwrap().id;
        db.delete_history(id).unwrap();
        assert_eq!(db.list_history("", 10, 0).unwrap().1, 1);
        db.clear_history().unwrap();
        assert!(db.last_history().unwrap().is_none());
    }

    #[test]
    fn csv_escapes_and_has_bom() {
        let mut h = hist(0, "a, \"b\"\nc");
        h.action = "translate".into();
        let csv = history_to_csv(&[h]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Translate"));
        assert!(csv.contains("\"a, \"\"b\"\"\nc\""));
    }
}
