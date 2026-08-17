//! Vision output parsing — pure logic, heavily unit-tested. Used by `agent::capture_context_snapshot` path.

pub(crate) const ALLOWED_CATEGORIES: &str = "Coding, Debugging, CodeReview, Testing, Documentation, Design, \
Planning, Meeting, Communication, Research, Learning, DevOps, Database, Sales, Admin, Browsing, Idle, General";

const CATEGORY_MAP: &[(&str, &str)] = &[
    ("coding", "Coding"),
    ("debugging", "Debugging"),
    ("codereview", "CodeReview"),
    ("testing", "Testing"),
    ("documentation", "Documentation"),
    ("design", "Design"),
    ("planning", "Planning"),
    ("meeting", "Meeting"),
    ("communication", "Communication"),
    ("research", "Research"),
    ("learning", "Learning"),
    ("devops", "DevOps"),
    ("database", "Database"),
    ("sales", "Sales"),
    ("admin", "Admin"),
    ("browsing", "Browsing"),
    ("idle", "Idle"),
    ("general", "General"),
];

/// Full pipeline: structured description + resolved category label.
/// Category is never empty/whitespace — unknown or missing always becomes "General".
pub(crate) fn parse_analysis(raw: &str) -> (String, String) {
    let lower = raw.to_lowercase();

    let category = extract_category_from_field(&lower)
        .unwrap_or_else(|| infer_category_from_content(&lower));
    let category = resolve_persisted_category(&category);

    let description = build_structured_description(raw);

    (description, category)
}

/// SQLite / emit gate: never persist a blank category.
pub(crate) fn resolve_persisted_category(category: &str) -> String {
    let trimmed = category.trim();
    if trimmed.is_empty() {
        "General".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Strip to a single lowercase alnum token so "Code Review", "code_review", "CodeReview" → `codereview`.
fn normalize_category_value(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn lookup_category(norm: &str) -> Option<&'static str> {
    CATEGORY_MAP
        .iter()
        .find(|(key, _)| *key == norm)
        .map(|(_, label)| *label)
}

/// Match a known category from the start of `s`, preferring longer phrases ("code review" over "code").
fn match_category_prefix(s: &str) -> Option<&'static str> {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    if let Some(label) = lookup_category(&normalize_category_value(s)) {
        return Some(label);
    }
    for n in (1..=words.len().min(3)).rev() {
        let chunk = words[..n].join(" ");
        if let Some(label) = lookup_category(&normalize_category_value(&chunk)) {
            return Some(label);
        }
    }
    None
}

/// Extract category from an explicit "CATEGORY: Xyz" field in the model output.
/// Handles a dedicated last line, `category :` with spaces, and an inline field
/// after newline-flattening (e.g. "... CATEGORY: Planning VISIBLE CONTENT: ...").
fn extract_category_from_field(lower: &str) -> Option<String> {
    let mut after_colon: Option<&str> = None;
    for (i, _) in lower.rmatch_indices("category") {
        let rest = lower[i + "category".len()..].trim_start();
        if let Some(stripped) = rest.strip_prefix(':') {
            after_colon = Some(stripped.trim_start());
            break;
        }
    }
    let after = after_colon?;
    let first_line = after.lines().next()?.trim();
    if first_line.is_empty() {
        return None;
    }
    match_category_prefix(first_line).map(str::to_string)
}

/// Fallback: infer category from keywords in the full content.
fn infer_category_from_content(lower: &str) -> String {
    if lower.contains("debugger") || lower.contains("breakpoint") {
        "Debugging"
    } else if lower.contains("pull request")
        || lower.contains("reviewing code")
        || lower.contains("code review")
    {
        "CodeReview"
    } else if lower.contains("running tests")
        || lower.contains("test results")
        || lower.contains("test suite")
    {
        "Testing"
    } else if lower.contains("microsoft excel")
        || lower.contains("google sheets")
        || lower.contains("libreoffice calc")
        || lower.contains("spreadsheet")
        || lower.contains(".xlsx")
        || lower.contains(".xls")
    {
        // Spreadsheets are often miscategorized as "Coding" when the prompt mentions a generic "editor".
        "Admin"
    } else if lower.contains("writing code")
        || lower.contains("visual studio code")
        || lower.contains("vs code")
        || lower.contains("vscode")
        || lower.contains("intellij")
        || lower.contains("pycharm")
        || lower.contains("webstorm")
        || lower.contains("rider")
        || lower.contains("xcode")
        || lower.contains("android studio")
        || lower.contains("neovim")
    {
        "Coding"
    } else if lower.contains("writing docs") || lower.contains("readme") {
        "Documentation"
    } else if lower.contains("figma") || lower.contains("sketch") || lower.contains("design tool") {
        "Design"
    } else if lower.contains("jira") || lower.contains("trello") || lower.contains("backlog") {
        "Planning"
    } else if lower.contains("zoom")
        || lower.contains("google meet")
        || lower.contains("teams meeting")
    {
        "Meeting"
    } else if lower.contains("slack") || lower.contains("discord") || lower.contains("email") {
        "Communication"
    } else if lower.contains("stackoverflow")
        || lower.contains("searching")
        || lower.contains("google search")
    {
        "Research"
    } else if lower.contains("tutorial") || lower.contains("course") || lower.contains("learning") {
        "Learning"
    } else if lower.contains("docker")
        || lower.contains("kubernetes")
        || lower.contains("pipeline")
        || lower.contains("ci/cd")
    {
        "DevOps"
    } else if lower.contains("sql") || lower.contains("database") || lower.contains("supabase") {
        "Database"
    } else if lower.contains("crm") || lower.contains("hubspot") {
        "Sales"
    } else if lower.contains("settings") || lower.contains("configuration") {
        "Admin"
    } else if lower.contains("browser")
        || lower.contains("chrome")
        || lower.contains("firefox")
        || lower.contains("linkedin")
        || lower.contains("github.com")
    {
        "Browsing"
    } else if lower.contains("idle") || lower.contains("no activity") || lower.contains("lock screen") {
        "Idle"
    } else {
        "General"
    }
    .to_string()
}

fn strip_markdown(s: &str) -> String {
    s.replace("####", "")
        .replace("###", "")
        .replace("##", "")
        .replace("**", "")
        .trim()
        .to_string()
}

/// Byte index of a `category` token whose next non-whitespace char is `:`.
fn find_category_field_index(lower: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("category") {
        let i = search_from + rel;
        let rest = lower[i + "category".len()..].trim_start();
        if rest.starts_with(':') {
            return Some(i);
        }
        search_from = i + "category".len();
    }
    None
}

fn end_of_nth_word(s: &str, n: usize) -> usize {
    let mut consumed = 0usize;
    let mut seen = 0usize;
    for word in s.split_whitespace() {
        if let Some(rel) = s[consumed..].find(word) {
            consumed += rel + word.len();
            seen += 1;
            if seen == n {
                return consumed;
            }
        }
    }
    s.len()
}

/// Drop `CATEGORY: <value>` (known label, or the next token if unknown) from a line.
fn remove_category_field(line: &str) -> String {
    let mut current = line.to_string();
    loop {
        let lower = current.to_lowercase();
        let Some(idx) = find_category_field_index(&lower) else {
            break;
        };
        let before = current[..idx].trim_end();
        let after_name = &current[idx + "category".len()..];
        let trimmed = after_name.trim_start();
        let Some(after_colon) = trimmed.strip_prefix(':') else {
            break;
        };
        let after_colon = after_colon.trim_start();
        let skip = if match_category_prefix(after_colon).is_some() {
            let words: Vec<&str> = after_colon.split_whitespace().collect();
            let mut n = 1usize;
            for try_n in (1..=words.len().min(3)).rev() {
                let chunk = words[..try_n].join(" ");
                if lookup_category(&normalize_category_value(&chunk)).is_some() {
                    n = try_n;
                    break;
                }
            }
            end_of_nth_word(after_colon, n)
        } else if after_colon.split_whitespace().next().is_some() {
            end_of_nth_word(after_colon, 1)
        } else {
            0
        };
        let rest = after_colon[skip.min(after_colon.len())..].trim_start();
        current = if before.is_empty() {
            rest.to_string()
        } else if rest.is_empty() {
            before.to_string()
        } else {
            format!("{before} {rest}")
        };
    }
    current
}

fn build_structured_description(raw: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut boilerplate: Vec<String> = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let without_category = remove_category_field(trimmed);
        let clean = strip_markdown(&without_category);
        if clean.is_empty() {
            continue;
        }

        let upper = clean.to_uppercase();
        if upper.starts_with("APP:") || upper.starts_with("WINDOW TITLE:") {
            boilerplate.push(clean);
        } else {
            parts.push(clean);
        }
    }

    if parts.is_empty() {
        parts = boilerplate;
    }

    if parts.is_empty() {
        return "No analysis available".to_string();
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefers_explicit_category_field() {
        let raw = "APP: X\nCATEGORY: debugging\n";
        let (_desc, cat) = parse_analysis(raw);
        assert_eq!(cat, "Debugging");
    }

    #[test]
    fn parse_category_field_accepts_multiword_code_review() {
        let raw = "APP: Gh\nCATEGORY: Code Review\n";
        let (_desc, cat) = parse_analysis(raw);
        assert_eq!(cat, "CodeReview");
    }

    #[test]
    fn parse_infers_debugging() {
        let raw = "VISIBLE: using the debugger and a breakpoint";
        let (_d, c) = parse_analysis(raw);
        assert_eq!(c, "Debugging");
    }

    #[test]
    fn parse_infers_each_major_bucket() {
        let cases = [
            ("code review here", "CodeReview"),
            ("test suite green", "Testing"),
            ("writing code in editor", "Coding"),
            ("excel spreadsheet open", "Admin"),
            ("readme writing docs", "Documentation"),
            ("figma open", "Design"),
            ("jira board", "Planning"),
            ("zoom call", "Meeting"),
            ("slack message", "Communication"),
            ("stackoverflow page", "Research"),
            ("tutorial course", "Learning"),
            ("docker pipeline", "DevOps"),
            ("sql database supabase", "Database"),
            ("crm hubspot", "Sales"),
            ("settings configuration", "Admin"),
            ("chrome browser linkedin", "Browsing"),
            ("idle lock screen", "Idle"),
        ];
        for (text, expected) in cases {
            let (_, c) = parse_analysis(text);
            assert_eq!(c, expected, "text={text:?}");
        }
    }

    #[test]
    fn parse_generic_editor_word_not_auto_coding() {
        let raw = "VISIBLE: drafting text in a generic editor window";
        let (_, c) = parse_analysis(raw);
        assert_ne!(c, "Coding");
    }

    #[test]
    fn structured_description_strips_category_line() {
        let raw = "APP: VS\nCATEGORY: Coding\nVISIBLE: ok";
        let (d, c) = parse_analysis(raw);
        assert!(!d.to_uppercase().contains("CATEGORY:"));
        assert!(d.contains("VISIBLE:"));
        assert_eq!(c, "Coding");
    }

    #[test]
    fn empty_lines_yield_no_analysis_available() {
        let (d, c) = parse_analysis("   \n  \n");
        assert_eq!(d, "No analysis available");
        assert_eq!(c, "General");
    }

    #[test]
    fn category_field_maps_general_token() {
        let (_, c) = parse_analysis("noise\nCATEGORY: general\n");
        assert_eq!(c, "General");
    }

    #[test]
    fn markdown_stripped_in_description() {
        let raw = "### APP: Test\n**VISIBLE**: x";
        let (d, _) = parse_analysis(raw);
        assert!(!d.contains("###"));
        assert!(!d.contains("**"));
    }

    #[test]
    fn empty_or_whitespace_category_becomes_general() {
        assert_eq!(resolve_persisted_category(""), "General");
        assert_eq!(resolve_persisted_category("   "), "General");
        assert_eq!(resolve_persisted_category("\n\t"), "General");
        assert_eq!(resolve_persisted_category("Planning"), "Planning");
    }

    #[test]
    fn category_at_end_is_parsed_and_stripped() {
        let raw = "Reviewing the sprint board and moving tickets.\nCATEGORY: Planning";
        let (d, c) = parse_analysis(raw);
        assert_eq!(c, "Planning");
        assert!(!d.to_uppercase().contains("CATEGORY"));
        assert!(d.contains("sprint board"));
    }

    #[test]
    fn missing_category_infers_then_falls_back_to_general() {
        let (_, inferred) = parse_analysis("looking at the jira backlog");
        assert_eq!(inferred, "Planning");

        let (_, unknown) = parse_analysis("moved a couple of windows around");
        assert_eq!(unknown, "General");
        assert!(!unknown.trim().is_empty());
    }

    #[test]
    fn category_inline_in_flattened_body_is_parsed_and_stripped() {
        let raw = "APP: VS WINDOW TITLE: foo VISIBLE CONTENT: editing a file CATEGORY: Planning NEXT STEP: commit";
        let (d, c) = parse_analysis(raw);
        assert_eq!(c, "Planning");
        assert!(!d.to_uppercase().contains("CATEGORY:"));
        assert!(d.to_uppercase().contains("VISIBLE CONTENT:"));
    }

    #[test]
    fn blank_category_field_falls_back_to_infer() {
        let raw = "VISIBLE: using the debugger\nCATEGORY:   \n";
        let (_, c) = parse_analysis(raw);
        assert_eq!(c, "Debugging");
    }

    #[test]
    fn category_with_space_before_colon() {
        let raw = "Editing tests.\nCATEGORY : Testing";
        let (d, c) = parse_analysis(raw);
        assert_eq!(c, "Testing");
        assert!(!d.to_uppercase().contains("CATEGORY"));
    }

    #[test]
    fn category_codereview_in_body_stripped() {
        let raw = "APP: GitHub\nWINDOW TITLE: PR\nCURRENT ACTION: reviewing a pull request\nCATEGORY: CodeReview";
        let (d, c) = parse_analysis(raw);
        assert_eq!(c, "CodeReview");
        assert!(!d.to_uppercase().contains("CATEGORY:"));
        assert!(!d.to_uppercase().starts_with("APP:"));
        assert!(d.to_uppercase().contains("CURRENT ACTION:"));
    }
}
