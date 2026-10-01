//! Local weekly PDF schedule. The renderer supplies the same PDF bytes used by
//! the manual Work report button; this module owns the destination and prevents
//! a second automatic save in the same week.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const SCHEDULE_FILE: &str = "weekly-report-schedule.json";
const MAX_REPORT_BYTES: usize = 20 * 1024 * 1024;
static SCHEDULE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReportSchedule {
    pub enabled: bool,
    /// Sunday = 0, Monday = 1, … Saturday = 6, matching JavaScript Date.
    pub weekday: u8,
    pub time: String,
    pub folder: String,
    pub revision: u64,
    pub last_generated_date: Option<String>,
    pub last_saved_path: Option<String>,
}

impl Default for WeeklyReportSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            weekday: 5,
            time: "17:00".into(),
            folder: String::new(),
            revision: 0,
            last_generated_date: None,
            last_saved_path: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReportScheduleInput {
    pub enabled: bool,
    pub weekday: u8,
    pub time: String,
    pub folder: String,
}

fn schedule_path() -> Result<PathBuf, String> {
    Ok(crate::paths::app_data_dir()?.join(SCHEDULE_FILE))
}

fn read_schedule(path: &Path) -> Result<WeeklyReportSchedule, String> {
    if !path.exists() {
        return Ok(WeeklyReportSchedule::default());
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Could not read weekly report settings: {error}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Weekly report settings are invalid: {error}"))
}

fn write_schedule(path: &Path, schedule: &WeeklyReportSchedule) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(schedule).map_err(|error| error.to_string())?;
    let temp_path = path.with_file_name(format!("{SCHEDULE_FILE}.{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, bytes)
        .map_err(|error| format!("Could not save weekly report settings: {error}"))?;
    if let Err(error) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("Could not save weekly report settings: {error}"));
    }
    Ok(())
}

fn validate_time(time: &str) -> Result<NaiveTime, String> {
    let bytes = time.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes[0..2].iter().all(u8::is_ascii_digit)
        || !bytes[3..5].iter().all(u8::is_ascii_digit)
    {
        return Err("Choose a valid time in HH:MM format.".into());
    }
    NaiveTime::parse_from_str(time, "%H:%M")
        .map_err(|_| "Choose a valid time in HH:MM format.".into())
}

fn validate_folder(folder: &str) -> Result<String, String> {
    let path = std::fs::canonicalize(folder)
        .map_err(|_| "Choose an existing folder for weekly reports.".to_string())?;
    if !path.is_dir() {
        return Err("Choose a folder, not a file, for weekly reports.".into());
    }
    Ok(path.to_string_lossy().to_string())
}

fn validate_pdf_bytes(bytes: &[u8]) -> Result<(), String> {
    if !bytes.starts_with(b"%PDF-") || bytes.len() > MAX_REPORT_BYTES {
        return Err("The generated report is missing a PDF header or exceeds 20 MB.".into());
    }
    Ok(())
}

fn is_due(schedule: &WeeklyReportSchedule, now: NaiveDateTime) -> bool {
    if !schedule.enabled || schedule.weekday != now.weekday().num_days_from_sunday() as u8 {
        return false;
    }
    let Ok(time) = validate_time(&schedule.time) else {
        return false;
    };
    let already_generated_this_week = schedule
        .last_generated_date
        .as_deref()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .is_some_and(|date| date.iso_week() == now.date().iso_week());
    now.time() >= time && !already_generated_this_week
}

fn can_save_started_run(
    schedule: &WeeklyReportSchedule,
    started_at: NaiveDateTime,
    elapsed: ChronoDuration,
) -> bool {
    elapsed >= ChronoDuration::zero()
        && elapsed <= ChronoDuration::hours(24)
        && is_due(schedule, started_at)
}

#[tauri::command]
pub fn get_weekly_report_schedule() -> Result<WeeklyReportSchedule, String> {
    let _guard = SCHEDULE_LOCK.lock().map_err(|error| error.to_string())?;
    read_schedule(&schedule_path()?)
}

pub fn clear_weekly_report_schedule() -> Result<(), String> {
    let _guard = SCHEDULE_LOCK.lock().map_err(|error| error.to_string())?;
    let path = schedule_path()?;
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("Could not erase weekly report settings: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn save_weekly_report_schedule(
    schedule: WeeklyReportScheduleInput,
) -> Result<WeeklyReportSchedule, String> {
    if schedule.weekday > 6 {
        return Err("Choose a day of the week.".into());
    }
    validate_time(&schedule.time)?;
    let folder = if schedule.enabled {
        validate_folder(&schedule.folder)?
    } else {
        schedule.folder.trim().to_string()
    };
    let _guard = SCHEDULE_LOCK.lock().map_err(|error| error.to_string())?;
    let path = schedule_path()?;
    let mut current = read_schedule(&path)?;
    if current.enabled == schedule.enabled
        && current.weekday == schedule.weekday
        && current.time == schedule.time
        && current.folder == folder
    {
        return Ok(current);
    }
    current.enabled = schedule.enabled;
    current.weekday = schedule.weekday;
    current.time = schedule.time;
    current.folder = folder;
    current.revision = current.revision.saturating_add(1);
    write_schedule(&path, &current)?;
    Ok(current)
}

#[tauri::command]
pub fn save_scheduled_report_pdf(
    revision: u64,
    started_at: String,
    filename: String,
    bytes: Vec<u8>,
) -> Result<String, String> {
    validate_pdf_bytes(&bytes)?;
    let _guard = SCHEDULE_LOCK.lock().map_err(|error| error.to_string())?;
    let path = schedule_path()?;
    save_due_report(
        &path,
        revision,
        &started_at,
        &filename,
        &bytes,
        Local::now(),
    )
}

fn save_due_report(
    path: &Path,
    revision: u64,
    started_at: &str,
    filename: &str,
    bytes: &[u8],
    now: DateTime<Local>,
) -> Result<String, String> {
    let mut schedule = read_schedule(path)?;
    if schedule.revision != revision {
        return Err(
            "The weekly report settings changed during generation. Please try again.".into(),
        );
    }
    let started_at = DateTime::parse_from_rfc3339(started_at)
        .map_err(|_| "Invalid weekly report start time.".to_string())?
        .with_timezone(&Local);
    if !can_save_started_run(
        &schedule,
        started_at.naive_local(),
        now.signed_duration_since(started_at),
    ) {
        return Err(
            "The weekly report was not due when it started or was already saved this week.".into(),
        );
    }
    let folder = validate_folder(&schedule.folder)?;
    let saved_path = crate::paths::save_pdf_to_directory(Path::new(&folder), filename, bytes)?;
    schedule.last_generated_date = Some(started_at.format("%Y-%m-%d").to_string());
    schedule.last_saved_path = Some(saved_path.clone());
    if let Err(error) = write_schedule(path, &schedule) {
        let _ = std::fs::remove_file(&saved_path);
        return Err(error);
    }
    Ok(saved_path)
}

/// A backend pulse keeps the hidden WebView's schedule check alive while the
/// main window sits in the tray. The renderer also checks on startup and wake.
pub fn start_check_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        if app.emit("weekly-report-check", ()).is_err() {
            break;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn weekly_schedule_runs_once_after_selected_local_time() {
        let mut schedule = WeeklyReportSchedule {
            enabled: true,
            ..Default::default()
        };
        assert!(!is_due(&schedule, at(2026, 10, 2, 16, 59)));
        assert!(is_due(&schedule, at(2026, 10, 2, 17, 0)));
        assert!(!is_due(&schedule, at(2026, 10, 3, 17, 0)));
        schedule.last_generated_date = Some("2026-10-02".into());
        assert!(!is_due(&schedule, at(2026, 10, 2, 18, 0)));
        assert!(is_due(&schedule, at(2026, 10, 9, 17, 0)));
    }

    #[test]
    fn changing_schedule_does_not_generate_twice_in_the_same_week() {
        let schedule = WeeklyReportSchedule {
            enabled: true,
            weekday: 6,
            last_generated_date: Some("2026-10-02".into()),
            ..Default::default()
        };
        assert!(!is_due(&schedule, at(2026, 10, 3, 17, 0)));
    }

    #[test]
    fn a_late_report_can_finish_after_midnight() {
        let schedule = WeeklyReportSchedule {
            enabled: true,
            weekday: 5,
            time: "23:59".into(),
            ..Default::default()
        };
        let started = at(2026, 10, 2, 23, 59);
        assert!(can_save_started_run(
            &schedule,
            started,
            ChronoDuration::minutes(20)
        ));
        assert!(!can_save_started_run(
            &schedule,
            started,
            ChronoDuration::hours(25)
        ));
    }

    #[test]
    fn settings_can_be_replaced_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SCHEDULE_FILE);
        let mut schedule = WeeklyReportSchedule::default();
        write_schedule(&path, &schedule).unwrap();
        schedule.enabled = true;
        schedule.revision = 1;
        write_schedule(&path, &schedule).unwrap();
        assert_eq!(read_schedule(&path).unwrap(), schedule);
    }

    #[test]
    fn invalid_times_do_not_become_due() {
        assert!(validate_time("09:30").is_ok());
        assert!(validate_time("9:30").is_err());
        assert!(validate_time("25:00").is_err());
        let schedule = WeeklyReportSchedule {
            enabled: true,
            time: "99:99".into(),
            ..Default::default()
        };
        assert!(!is_due(&schedule, at(2026, 10, 2, 17, 0)));
    }

    #[test]
    fn scheduled_report_rejects_non_pdf_or_oversized_content() {
        assert!(validate_pdf_bytes(b"not a pdf").is_err());
        assert!(validate_pdf_bytes(b"%PDF-1.7\ncontent").is_ok());
        let mut oversized = vec![0; MAX_REPORT_BYTES + 1];
        oversized[..5].copy_from_slice(b"%PDF-");
        assert!(validate_pdf_bytes(&oversized).is_err());
    }

    #[test]
    fn due_report_saves_in_chosen_folder_once_and_records_its_path() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("chosen-reports");
        std::fs::create_dir(&destination).unwrap();
        let schedule_path = directory.path().join(SCHEDULE_FILE);
        let schedule = WeeklyReportSchedule {
            enabled: true,
            weekday: 5,
            time: "17:00".into(),
            folder: destination.to_string_lossy().to_string(),
            revision: 1,
            ..Default::default()
        };
        write_schedule(&schedule_path, &schedule).unwrap();
        let now = Local
            .from_local_datetime(&at(2026, 10, 2, 17, 0))
            .single()
            .unwrap();
        let bytes = b"%PDF-1.7\nweekly report";
        let saved = save_due_report(
            &schedule_path,
            1,
            &now.to_rfc3339(),
            "FlowSight_Weekly.pdf",
            bytes,
            now,
        )
        .unwrap();
        assert!(Path::new(&saved).starts_with(std::fs::canonicalize(&destination).unwrap()));
        assert_eq!(std::fs::read(&saved).unwrap(), bytes);
        let updated = read_schedule(&schedule_path).unwrap();
        assert_eq!(updated.last_saved_path.as_deref(), Some(saved.as_str()));
        assert_eq!(updated.last_generated_date.as_deref(), Some("2026-10-02"));
        assert!(save_due_report(
            &schedule_path,
            1,
            &now.to_rfc3339(),
            "FlowSight_Weekly.pdf",
            bytes,
            now,
        )
        .is_err());
    }
}
