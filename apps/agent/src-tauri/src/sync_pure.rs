//! Pure sync helpers (no HTTP). Covered by unit tests; not duplicated in release artifact beyond code size.

use base64::Engine;

/// Keep the pending queue indexed without retaining synchronized history in
/// the index. The predicate and ordering match `select_unsynced_pending_sql`.
pub(crate) const PENDING_REPORT_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS reports_pending_by_id ON reports(id) WHERE synced = 0";

/// Batch of unsynced rows (oldest first) for `perform_sync`. `limit` is clamped to 1..=5000.
pub(crate) fn select_unsynced_pending_sql(limit: usize) -> String {
    let lim = limit.clamp(1, 5000);
    format!(
        "SELECT id, description, activity_type, duration_seconds, jira_ticket_id \
         FROM reports \
         WHERE synced = 0 \
         ORDER BY id ASC \
         LIMIT {}",
        lim
    )
}

pub(crate) fn jwt_exp(token: &str) -> i64 {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return 0;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))
        .unwrap_or_default();
    serde_json::from_slice::<serde_json::Value>(&decoded)
        .ok()
        .and_then(|v| v["exp"].as_i64())
        .unwrap_or(0)
}

/// Shorten a single task description so many rows fit under `FLOWSIGHT_SUMMARY_MAX_CHARS`.
pub(crate) fn clamp_line_for_summary(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        return s.to_string();
    }
    let take = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

/// When the batch exceeds the summary budget, keep the **suffix** (newest tasks: `full_text` is oldest→newest).
pub(crate) fn truncate_tasks_for_summary(text: &str, max_chars: usize) -> String {
    let n = text.chars().count();
    if n <= max_chars {
        return text.to_string();
    }
    const OMIT: &str =
        "[... earlier activity omitted; excerpt is the most recent part of the batch ...]\n\n";
    let overhead = OMIT.chars().count();
    let budget = max_chars.saturating_sub(overhead);
    let skip = n.saturating_sub(budget);
    let suffix: String = text.chars().skip(skip).collect();
    format!("{}{}", OMIT, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn pending_index_preserves_batch_and_avoids_scanning_synced_history() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                id INTEGER PRIMARY KEY, description TEXT, activity_type TEXT,
                duration_seconds INTEGER, jira_ticket_id TEXT, synced INTEGER
             );
             WITH RECURSIVE ids(id) AS (
                VALUES(1) UNION ALL SELECT id + 1 FROM ids WHERE id < 10000
             ) INSERT INTO reports(id, synced) SELECT id, 1 FROM ids;
             UPDATE reports SET synced = 0 WHERE id IN (7, 9000, 9999);
             UPDATE reports SET synced = NULL WHERE id = 6;",
        )
        .unwrap();
        let sql = select_unsynced_pending_sql(2);
        let batch = || {
            conn.prepare(&sql)
                .unwrap()
                .query_map([], |row| row.get::<_, i64>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let expected = batch();
        conn.execute_batch(PENDING_REPORT_INDEX_SQL).unwrap();
        conn.execute_batch(PENDING_REPORT_INDEX_SQL).unwrap();
        assert_eq!(expected, [7, 9000]);
        assert_eq!(batch(), expected);
        let plan: String = conn
            .query_row(&format!("EXPLAIN QUERY PLAN {sql}"), [], |row| row.get(3))
            .unwrap();
        assert!(plan.contains("reports_pending_by_id"), "{plan}");
        conn.execute("UPDATE reports SET synced = 1 WHERE id = 7", [])
            .unwrap();
        assert_eq!(batch(), [9000, 9999]);
    }

    fn jwt_with_exp(exp: i64) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = serde_json::json!({ "exp": exp });
        let payload_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
        format!("{}.{}.sig", header, payload_b64)
    }

    #[test]
    fn jwt_exp_reads_payload() {
        assert_eq!(jwt_exp(&jwt_with_exp(1_700_000_000)), 1_700_000_000);
    }

    #[test]
    fn jwt_exp_invalid_returns_zero() {
        assert_eq!(jwt_exp(""), 0);
        assert_eq!(jwt_exp("not-a-jwt"), 0);
        assert_eq!(jwt_exp("a.b.c"), 0);
    }

    #[test]
    fn truncate_keeps_short_text() {
        let s = "hello 世界";
        assert_eq!(truncate_tasks_for_summary(s, 100), s);
    }

    #[test]
    fn truncate_unicode_boundary_keeps_suffix_and_budget() {
        let s: String = (0..6000).map(|_| 'a').collect();
        let t = truncate_tasks_for_summary(&s, 5000);
        assert_eq!(t.chars().count(), 5000);
        assert!(t.contains("omitted"));
        assert!(t.ends_with('a'));
    }

    #[test]
    fn clamp_line_adds_ellipsis_when_long() {
        let s: String = (0..500).map(|_| 'b').collect();
        let t = clamp_line_for_summary(&s, 20);
        assert_eq!(t.chars().count(), 20);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn pending_query_selects_all_unsynced_oldest_first() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE reports (
                id INTEGER PRIMARY KEY,
                description TEXT,
                activity_type TEXT,
                synced INTEGER DEFAULT 0,
                created_at TEXT,
                jira_ticket_id TEXT,
                duration_seconds INTEGER DEFAULT 30
            );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO reports (description, activity_type, synced, created_at) VALUES ('old', 'x', 0, datetime('now', '-30 minutes', 'localtime'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO reports (description, activity_type, synced, created_at) VALUES ('new', 'y', 0, datetime('now', '-2 minutes', 'localtime'))",
            [],
        )
        .unwrap();

        let mut stmt = conn.prepare(&select_unsynced_pending_sql(500)).unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], "old");
        assert_eq!(rows[1], "new");
    }
}
