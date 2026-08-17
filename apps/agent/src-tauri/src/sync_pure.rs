//! Pure sync helpers (no HTTP). Covered by unit tests; not duplicated in release artifact beyond code size.

use base64::Engine;

/// Batch of unsynced rows (oldest first) for `perform_sync`. `limit` is clamped to 1..=5000.
pub(crate) fn select_unsynced_pending_sql(limit: usize) -> String {
    let lim = limit.max(1).min(5000);
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
    const OMIT: &str = "[... earlier activity omitted; excerpt is the most recent part of the batch ...]\n\n";
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
