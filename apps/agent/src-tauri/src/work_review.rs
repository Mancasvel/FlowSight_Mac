//! User-chosen review decisions. Local SQLite only; never part of reports/MCP/sync.
use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDecision {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub period_start: String,
    pub period_end: String,
    pub review_date: String,
    pub status: String,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionInput {
    pub id: Option<String>,
    pub kind: String,
    pub text: String,
    pub period_start: String,
    pub period_end: String,
    pub review_date: String,
    pub status: String,
    pub note: String,
}

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS work_review_decisions (
        id TEXT PRIMARY KEY, kind TEXT NOT NULL, text TEXT NOT NULL,
        period_start TEXT NOT NULL, period_end TEXT NOT NULL, review_date TEXT NOT NULL,
        status TEXT NOT NULL, note TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS work_review_updated ON work_review_decisions(updated_at);",
    )
    .map_err(|e| e.to_string())
}

fn date(value: &str) -> Result<NaiveDate, String> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "Choose a valid date.".to_string())?;
    if parsed.format("%Y-%m-%d").to_string() != value {
        return Err("Choose a valid date.".into());
    }
    Ok(parsed)
}

fn validate(input: &DecisionInput) -> Result<(), String> {
    if !["change", "keep"].contains(&input.kind.as_str())
        || !["planned", "tried", "not_tried", "discarded"].contains(&input.status.as_str())
    {
        return Err("Invalid review choice.".into());
    }
    if input.text.trim().is_empty()
        || input.text.chars().count() > 500
        || input.note.chars().count() > 1000
    {
        return Err("Use a decision up to 500 characters and a note up to 1000 characters.".into());
    }
    if date(&input.period_start)? > date(&input.period_end)? {
        return Err("Invalid report period.".into());
    }
    date(&input.review_date)?;
    Ok(())
}

pub(crate) fn list(conn: &Connection) -> Result<Vec<ReviewDecision>, String> {
    ensure_schema(conn)?;
    let mut stmt = conn.prepare("SELECT id,kind,text,period_start,period_end,review_date,status,note,created_at,updated_at
        FROM work_review_decisions ORDER BY updated_at DESC,id DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ReviewDecision {
                id: row.get(0)?,
                kind: row.get(1)?,
                text: row.get(2)?,
                period_start: row.get(3)?,
                period_end: row.get(4)?,
                review_date: row.get(5)?,
                status: row.get(6)?,
                note: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub(crate) fn save(conn: &Connection, mut input: DecisionInput) -> Result<ReviewDecision, String> {
    validate(&input)?;
    ensure_schema(conn)?;
    input.text = input.text.trim().to_string();
    input.note = input.note.trim().to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let id = match input.id {
        Some(id) => {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM work_review_decisions WHERE id=?1)",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !exists {
                return Err("This decision is no longer available. Refresh the review.".into());
            }
            conn.execute("UPDATE work_review_decisions SET kind=?2,text=?3,review_date=?4,status=?5,note=?6,updated_at=?7 WHERE id=?1",
                params![id,input.kind,input.text,input.review_date,input.status,input.note,now]).map_err(|e| e.to_string())?;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO work_review_decisions VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                params![
                    id,
                    input.kind,
                    input.text,
                    input.period_start,
                    input.period_end,
                    input.review_date,
                    input.status,
                    input.note,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            id
        }
    };
    list(conn)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "Could not read the saved decision.".into())
}

pub(crate) fn prune(conn: &Connection, retention_days: u32) -> Result<(), String> {
    ensure_schema(conn)?;
    conn.execute("DELETE FROM work_review_decisions WHERE datetime(updated_at) < datetime('now','localtime',?1)",
        [format!("-{} days",retention_days.clamp(1,365))]).map_err(|e| e.to_string())?;
    Ok(())
}

fn connection() -> Result<Connection, String> {
    let path = crate::paths::db_path()?;
    connection_at(&path)
}

fn connection_at(path: &Path) -> Result<Connection, String> {
    let settings = crate::privacy::load_privacy_settings(path)?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    prune(&conn, settings.retention_days)?;
    Ok(conn)
}

#[tauri::command]
pub fn get_work_review_decisions() -> Result<Vec<ReviewDecision>, String> {
    list(&connection()?)
}

#[tauri::command]
pub fn save_work_review_decision(input: DecisionInput) -> Result<ReviewDecision, String> {
    save(&connection()?, input)
}

#[tauri::command]
pub fn delete_work_review_decision(id: String) -> Result<(), String> {
    connection()?
        .execute("DELETE FROM work_review_decisions WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> DecisionInput {
        DecisionInput {
            id: None,
            kind: "change".into(),
            text: "Protect a writing block".into(),
            period_start: "2026-10-01".into(),
            period_end: "2026-10-07".into(),
            review_date: "2026-10-14".into(),
            status: "planned".into(),
            note: String::new(),
        }
    }
    #[test]
    fn persists_edits_and_results_without_changing_origin() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(file.path()).unwrap();
        let saved = save(&conn, input()).unwrap();
        drop(conn);
        let conn = Connection::open(file.path()).unwrap();
        assert_eq!(list(&conn).unwrap()[0].text, saved.text);
        let mut edit = input();
        edit.id = Some(saved.id.clone());
        edit.status = "tried".into();
        edit.note = "Useful; keep Tuesdays free. 東京".into();
        edit.period_start = "2026-09-01".into();
        let updated = save(&conn, edit).unwrap();
        assert_eq!(updated.period_start, saved.period_start);
        assert_eq!(updated.created_at, saved.created_at);
        assert_eq!(updated.note, "Useful; keep Tuesdays free. 東京");
        assert_eq!(list(&conn).unwrap().len(), 1);
    }
    #[test]
    fn rejects_invalid_or_deleted_choices_and_preserves_existing_data() {
        let conn = Connection::open_in_memory().unwrap();
        let saved = save(&conn, input()).unwrap();
        for mutate in [0, 1, 2, 3] {
            let mut bad = input();
            match mutate {
                0 => bad.review_date = "2026-02-30".into(),
                1 => bad.text = " ".into(),
                2 => bad.note = "x".repeat(1001),
                _ => bad.status = "approved".into(),
            };
            assert!(save(&conn, bad).is_err());
        }
        let mut missing = input();
        missing.id = Some("deleted".into());
        assert!(save(&conn, missing).is_err());
        assert_eq!(list(&conn).unwrap()[0].id, saved.id);
    }
    #[test]
    fn keeps_all_choices_available_for_review_and_personal_export() {
        let conn = Connection::open_in_memory().unwrap();
        let first = save(&conn, input()).unwrap();
        for _ in 0..105 {
            save(&conn, input()).unwrap();
        }
        assert_eq!(list(&conn).unwrap().len(), 106);
        let mut edit = input();
        edit.id = Some(first.id.clone());
        edit.status = "tried".into();
        assert_eq!(save(&conn, edit).unwrap().id, first.id);
        assert_eq!(list(&conn).unwrap().len(), 106);
    }
    #[test]
    fn retention_and_local_erasure_remove_private_decisions() {
        let conn = Connection::open_in_memory().unwrap();
        save(&conn, input()).unwrap();
        conn.execute(
            "UPDATE work_review_decisions SET updated_at=datetime('now','-40 days')",
            [],
        )
        .unwrap();
        prune(&conn, 30).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        save(&conn, input()).unwrap();
        conn.execute_batch("CREATE TABLE reports(id INTEGER); CREATE TABLE privacy_events(id INTEGER); CREATE TABLE config(key TEXT,value TEXT);").unwrap();
        crate::privacy::erase_local_database(&conn).unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }
}
