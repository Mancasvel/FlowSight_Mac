//! Vision output parsing — pure logic shared by the background capture paths.

use crate::focus_semantics::{canonical_category_label, canonicalize_category};

/// Full pipeline: structured description + resolved category label.
/// Category is never empty/whitespace — unknown or missing always becomes "General".
pub(crate) fn parse_analysis(raw: &str) -> (String, String) {
    let lower = raw.to_lowercase();

    let category =
        extract_category_from_field(&lower).unwrap_or_else(|| infer_category_from_content(&lower));
    // Local VL models often label IDEs / GitHub / terminals as Browsing or General.
    // Prefer clear engineering signals over a weak model label.
    let category = correct_misclassified_category(&category, &lower);
    let category = resolve_persisted_category(&category);

    let description = build_structured_description(raw);

    (description, category)
}

/// Cost-sensitive correction from trusted OS metadata. The local VLM still
/// describes the content, but a small screenshot model cannot overrule an
/// unambiguous executable/title prior. Browser alone is intentionally not a
/// category: its title/content decides Research, engineering, or Browsing.
pub(crate) fn correct_category_with_window(
    category: &str,
    app_name: Option<&str>,
    window_title: Option<&str>,
) -> String {
    let app = app_name.unwrap_or_default().to_ascii_lowercase();
    let title = window_title.unwrap_or_default().to_ascii_lowercase();
    let combined = format!("{app} {title}");

    if combined.contains("lock screen") || combined.contains("windows logon") {
        return "Idle".into();
    }
    if weak_activity_category(category) && eclipse_work_window(app_name, window_title) {
        return infer_engineering_subcategory(&combined);
    }
    if combined.contains("zoom")
        || combined.contains("google meet")
        || (combined.contains("teams")
            && (combined.contains("meeting") || combined.contains("call")))
    {
        return "Meeting".into();
    }
    if combined.contains("slack")
        || combined.contains("discord")
        || combined.contains("outlook")
        || combined.contains("mail")
        || combined.contains("teams")
    {
        return "Communication".into();
    }
    if looks_like_research_material(&combined) {
        return "Research".into();
    }
    if combined.contains("salesforce")
        || combined.contains("hubspot")
        || combined.contains("pipedrive")
        || combined.contains("dynamics 365 sales")
    {
        return "Sales".into();
    }
    if combined.contains("jira")
        || combined.contains("trello")
        || combined.contains("asana")
        || combined.contains("monday.com")
    {
        return "Planning".into();
    }
    if combined.contains("figma") || combined.contains("sketch") || combined.contains("adobe xd") {
        return "Design".into();
    }
    if combined.contains("excel")
        || combined.contains("libreoffice calc")
        || combined.contains("google sheets")
    {
        return if [
            "expense",
            "invoice",
            "timesheet",
            "roster",
            "contact list",
            "data entry",
        ]
        .iter()
        .any(|hint| title.contains(hint))
        {
            "Admin".into()
        } else if matches!(category, "Coding" | "General" | "Browsing") {
            "Analysis".into()
        } else {
            resolve_persisted_category(category)
        };
    }
    if combined.contains("power bi")
        || combined.contains("tableau")
        || combined.contains("looker studio")
    {
        return "Analysis".into();
    }
    if combined.contains("winword")
        || combined.contains("microsoft word")
        || combined.contains("libreoffice writer")
        || combined.contains("google docs")
        || combined.contains("scrivener")
    {
        return if ["procedure", "manual", "knowledge base", "documentation"]
            .iter()
            .any(|hint| title.contains(hint))
        {
            "Documentation".into()
        } else if matches!(category, "Coding" | "General" | "Browsing") {
            "Writing".into()
        } else {
            resolve_persisted_category(category)
        };
    }
    if combined.contains("notion")
        && [
            "knowledge base",
            "wiki",
            "documentation",
            "procedure",
            "manual",
        ]
        .iter()
        .any(|hint| title.contains(hint))
    {
        return "Documentation".into();
    }
    if combined.contains("powerpoint")
        || combined.contains("google slides")
        || combined.contains("keynote")
    {
        return if matches!(category, "Coding" | "General" | "Browsing") {
            "Design".into()
        } else {
            resolve_persisted_category(category)
        };
    }
    if combined.contains("quickbooks")
        || combined.contains("sage accounting")
        || combined.contains("workday")
    {
        return "Admin".into();
    }
    if combined.contains("github actions") || combined.contains("pipeline") {
        return "DevOps".into();
    }
    if combined.contains("pull request") || combined.contains("merge request") {
        return "CodeReview".into();
    }
    if combined.contains("github") || combined.contains("gitlab") {
        return if title.contains("docs")
            || title.contains("documentation")
            || title.contains("wiki")
        {
            "Research".into()
        } else if ["issues", "project board", "milestone", "roadmap"]
            .iter()
            .any(|hint| title.contains(hint))
        {
            "Planning".into()
        } else if title.contains("discussion") {
            "Communication".into()
        } else {
            "Coding".into()
        };
    }
    if combined.contains("stackoverflow")
        || combined.contains("developer.mozilla")
        || combined.contains(" docs")
        || combined.contains("documentation")
    {
        return "Research".into();
    }
    if combined.contains("sales navigator") {
        return "Sales".into();
    }
    if combined.contains("linkedin")
        && ["messaging", "messages", "inbox"]
            .iter()
            .any(|hint| title.contains(hint))
    {
        return "Communication".into();
    }
    if (combined.contains("youtube")
        && ["course", "tutorial", "lecture", "training", "webinar"]
            .iter()
            .any(|hint| title.contains(hint)))
        || (combined.contains("linkedin learning")
            && ["course", "lesson", "training"]
                .iter()
                .any(|hint| title.contains(hint)))
    {
        return "Learning".into();
    }
    let social_or_entertainment = combined.contains("linkedin")
        || combined.contains("reddit")
        || combined.contains("youtube")
        || combined.contains("netflix")
        || combined.contains("instagram");
    if social_or_entertainment {
        let evidence_signal = [
            "research",
            "evidence",
            "case study",
            "industry report",
            "professional community",
        ]
        .iter()
        .any(|hint| title.contains(hint));
        if category == "Research" && evidence_signal {
            return "Research".into();
        }
        return "Browsing".into();
    }
    const IDE_APPS: &[&str] = &[
        "cursor",
        "visual studio code",
        "code.exe",
        "intellij",
        "pycharm",
        "webstorm",
        "rider",
        "xcode",
        "android studio",
        "neovim",
        "sublime text",
        "zed",
    ];
    let title_looks_like_work = [
        ".rs",
        ".ts",
        ".tsx",
        ".js",
        ".py",
        ".java",
        ".go",
        ".cpp",
        "terminal",
        "project",
        "repository",
        "debug",
        "test",
    ]
    .iter()
    .any(|hint| title.contains(hint));
    if IDE_APPS.iter().any(|hint| combined.contains(hint))
        && title_looks_like_work
        && matches!(
            category,
            "Browsing" | "General" | "Idle" | "Admin" | "Communication"
        )
    {
        return infer_engineering_subcategory(&combined);
    }
    resolve_persisted_category(category)
}

/// Override weak/wrong labels when the text clearly shows software-engineering work.
fn correct_misclassified_category(category: &str, lower: &str) -> String {
    let weak = weak_activity_category(category);
    if weak && looks_like_engineering_work(lower) {
        return infer_engineering_subcategory(lower);
    }
    if matches!(category, "Coding" | "Browsing" | "General") && looks_like_research_material(lower)
    {
        return "Research".into();
    }
    category.to_string()
}

fn looks_like_research_material(lower: &str) -> bool {
    [
        "google scholar",
        "semantic scholar",
        "pubmed",
        "jstor",
        "arxiv",
        "researchgate",
        "web of science",
        "scopus",
        "peer-reviewed",
        "peer reviewed",
        "journal article",
        "scholarly article",
        "literature review",
        "market research",
        "customer research",
        "research repository",
        "dovetail",
    ]
    .iter()
    .any(|hint| lower.contains(hint))
}

fn looks_like_engineering_work(lower: &str) -> bool {
    if communication_mentions_another_tool(lower) {
        return false;
    }
    if looks_like_eclipse_work(lower) {
        return true;
    }
    const IDE_HINTS: &[&str] = &[
        "cursor",
        "visual studio code",
        "vs code",
        "vscode",
        "intellij",
        "pycharm",
        "webstorm",
        "rider",
        "xcode",
        "android studio",
        "neovim",
        "vim ",
        "sublime text",
        "zed ",
        "warp",
        "iterm",
        "terminal.app",
        "windows terminal",
    ];
    if IDE_HINTS.iter().any(|h| lower.contains(h)) {
        return true;
    }
    if lower.contains("github")
        || lower.contains("gitlab")
        || lower.contains("bitbucket")
        || lower.contains("pull request")
        || lower.contains("code review")
        || lower.contains("ci/cd")
        || lower.contains("github actions")
        || lower.contains("docker")
        || lower.contains("kubernetes")
        || lower.contains("pipeline")
        || (lower.contains("workflow")
            && (lower.contains("build") || lower.contains("release") || lower.contains("pipeline")))
    {
        return true;
    }
    if (lower.contains("terminal") || lower.contains("shell"))
        && (lower.contains("git ")
            || lower.contains("cargo")
            || lower.contains("npm ")
            || lower.contains("pnpm")
            || lower.contains("rustc")
            || lower.contains("docker")
            || lower.contains("kubectl"))
    {
        return true;
    }
    lower.contains("writing code")
        || lower.contains("editing code")
        || lower.contains("source code")
        || lower.contains(".rs ")
        || lower.contains(".ts ")
        || lower.contains(".tsx")
        || lower.contains(".py ")
}

fn weak_activity_category(category: &str) -> bool {
    matches!(
        category,
        "Browsing" | "General" | "Idle" | "Admin" | "Communication"
    )
}

fn eclipse_application(app: Option<&str>) -> bool {
    let app = app
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(
        app.trim_end_matches(".exe").trim_end_matches(".app"),
        "eclipse" | "eclipsec" | "eclipse ide" | "eclipse-ide" | "org.eclipse.platform"
    )
}

/// Eclipse must be the foreground application, with an identifiable source
/// file/workspace. A mention of an eclipse in a browser or a generic welcome
/// window is not evidence of programming.
pub(crate) fn eclipse_work_window(app: Option<&str>, title: Option<&str>) -> bool {
    if !eclipse_application(app) {
        return false;
    }
    let title = title.unwrap_or_default().to_ascii_lowercase();
    [
        ".java",
        ".kt",
        ".scala",
        ".c -",
        ".cpp",
        ".h -",
        ".py",
        ".rs",
        "workspace",
        "project",
        "source",
        "debugger",
        "breakpoint",
        "running tests",
        "test results",
        "test suite",
        "junit",
    ]
    .iter()
    .any(|hint| title.contains(hint))
}

fn looks_like_eclipse_work(lower: &str) -> bool {
    if communication_mentions_another_tool(lower) {
        return false;
    }
    let names_eclipse = lower
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| word == "eclipse");
    names_eclipse
        && [
            "eclipse-workspace",
            ".java",
            "java development environment",
            "code editor",
            "source code",
            "source file",
            "writing code",
            "editing code",
            "debugger",
            "breakpoint",
            "running tests",
            "test results",
            "test suite",
            "junit",
        ]
        .iter()
        .any(|hint| lower.contains(hint))
}

fn communication_mentions_another_tool(lower: &str) -> bool {
    [
        "messages about",
        "conversation about",
        "message about",
        "discussing eclipse",
    ]
    .iter()
    .any(|hint| lower.contains(hint))
}

/// Apply the same Eclipse correction when reading historic observations.
/// Keep the stored evidence intact and do not reinterpret an app mentioned
/// in another foreground application as the user's primary work.
pub(crate) fn resolve_activity_category(
    category: &str,
    description: &str,
    app: Option<&str>,
    title: Option<&str>,
) -> String {
    let category = resolve_persisted_category(category);
    if !weak_activity_category(&category) {
        return category;
    }
    let description = description.to_lowercase();
    if eclipse_work_window(app, title)
        || ((eclipse_application(app) || app.is_none()) && looks_like_eclipse_work(&description))
    {
        return infer_engineering_subcategory(&format!(
            "{} {description}",
            title.unwrap_or_default().to_lowercase()
        ));
    }
    category
}

fn infer_engineering_subcategory(lower: &str) -> String {
    if lower.contains("debugger") || lower.contains("breakpoint") {
        "Debugging".into()
    } else if lower.contains("pull request")
        || lower.contains("code review")
        || lower.contains("reviewing code")
        || (lower.contains("github") && lower.contains(" pull"))
    {
        "CodeReview".into()
    } else if lower.contains("test suite")
        || lower.contains("running tests")
        || lower.contains("test results")
        || lower.contains("junit")
    {
        "Testing".into()
    } else if lower.contains("docker")
        || lower.contains("kubernetes")
        || lower.contains("pipeline")
        || lower.contains("ci/cd")
        || lower.contains("github actions")
        || lower.contains("workflow")
    {
        "DevOps".into()
    } else {
        "Coding".into()
    }
}

/// SQLite / emit gate: canonicalize every known category and never persist a
/// blank one. Unknown labels remain inspectable instead of being silently
/// rewritten as productive or distracting work.
pub(crate) fn resolve_persisted_category(category: &str) -> String {
    canonicalize_category(category)
}

/// Match a known category from the start of `s`, preferring longer phrases ("code review" over "code").
fn match_category_prefix(s: &str) -> Option<&'static str> {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    if let Some(label) = canonical_category_label(s) {
        return Some(label);
    }
    for n in (1..=words.len().min(3)).rev() {
        let chunk = words[..n].join(" ");
        if let Some(label) = canonical_category_label(&chunk) {
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
    } else if (lower.contains("microsoft word")
        || lower.contains("google docs")
        || lower.contains("libreoffice writer"))
        && (lower.contains("writing") || lower.contains("drafting") || lower.contains("editing"))
    {
        "Writing"
    } else if lower.contains("microsoft excel")
        || lower.contains("google sheets")
        || lower.contains("libreoffice calc")
        || lower.contains("spreadsheet")
        || lower.contains(".xlsx")
        || lower.contains(".xls")
    {
        if lower.contains("analysis")
            || lower.contains("analyzing")
            || lower.contains("formula")
            || lower.contains("pivot")
            || lower.contains("forecast")
            || lower.contains("chart")
        {
            "Analysis"
        } else {
            "Admin"
        }
    } else if looks_like_engineering_work(lower) {
        // Cursor / VS Code / GitHub / terminals / CI — before generic "browser" heuristics.
        return infer_engineering_subcategory(lower);
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
        || lower.contains("developer docs")
        || lower.contains("mdn ")
        || looks_like_research_material(lower)
        || lower.contains("searching")
        || lower.contains("google search")
    {
        "Research"
    } else if lower.contains("tutorial") || lower.contains("course") || lower.contains("learning") {
        "Learning"
    } else if lower.contains("sql") || lower.contains("database") || lower.contains("supabase") {
        "Database"
    } else if lower.contains("crm") || lower.contains("hubspot") {
        "Sales"
    } else if lower.contains("settings") || lower.contains("configuration") {
        "Admin"
    } else if lower.contains("linkedin")
        || lower.contains("twitter")
        || lower.contains("instagram")
        || lower.contains("youtube")
        || lower.contains("netflix")
        || lower.contains("reddit")
        || ((lower.contains("browser")
            || lower.contains("chrome")
            || lower.contains("firefox")
            || lower.contains("safari"))
            && !looks_like_engineering_work(lower))
    {
        // Consumer / social browsing only — never treat GitHub/IDE work as Browsing.
        "Browsing"
    } else if lower.contains("idle")
        || lower.contains("no activity")
        || lower.contains("lock screen")
    {
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
                if canonical_category_label(&chunk).is_some() {
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
        assert_eq!(resolve_persisted_category("code review"), "CodeReview");
        assert_eq!(resolve_persisted_category("RESEARCH"), "Research");
        assert_eq!(
            resolve_persisted_category("Custom workflow"),
            "Custom workflow"
        );
    }

    #[test]
    fn parser_accepts_every_category_from_the_canonical_prompt_registry() {
        let prompt = crate::focus_semantics::allowed_categories_prompt();
        for expected in prompt.split(", ") {
            let raw = format!("CURRENT ACTION: labelled fixture\nCATEGORY: {expected}");
            let (_, actual) = parse_analysis(&raw);
            assert_eq!(actual, expected, "{expected}");
        }
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
    fn cursor_ide_not_browsing_even_if_model_says_so() {
        let raw = "APP: Cursor\nCURRENT ACTION: editing rust in the IDE\nCATEGORY: Browsing";
        let (_, c) = parse_analysis(raw);
        assert_eq!(c, "Coding");
    }

    #[test]
    fn github_engineering_page_not_browsing() {
        let raw = "APP: Safari\nVISIBLE CONTENT: GitHub release page for FlowSight Agent\nCATEGORY: Browsing";
        let (_, c) = parse_analysis(raw);
        assert_eq!(c, "Coding");
    }

    #[test]
    fn github_actions_is_devops() {
        let raw = "VISIBLE: GitHub Actions workflow pipeline running\nCATEGORY: General";
        let (_, c) = parse_analysis(raw);
        assert_eq!(c, "DevOps");
    }

    #[test]
    fn linkedin_still_browsing() {
        let raw = "APP: Chrome\nVISIBLE: linkedin feed\nCATEGORY: Browsing";
        let (_, c) = parse_analysis(raw);
        assert_eq!(c, "Browsing");
    }

    #[test]
    fn trusted_window_metadata_corrects_costly_false_positives() {
        assert_eq!(
            correct_category_with_window("Browsing", Some("Code.exe"), Some("main.rs - VS Code")),
            "Coding"
        );
        assert_eq!(
            correct_category_with_window("Coding", Some("Slack.exe"), Some("project channel")),
            "Communication"
        );
        assert_eq!(
            correct_category_with_window("Coding", Some("EXCEL.EXE"), Some("budget.xlsx")),
            "Analysis"
        );
        assert_eq!(
            correct_category_with_window("General", Some("Chrome"), Some("GitHub Actions")),
            "DevOps"
        );
    }

    #[test]
    fn eclipse_source_work_is_corrected_without_treating_every_eclipse_as_coding() {
        for app in [
            "eclipse",
            "Eclipse.exe",
            "Eclipse IDE",
            "Eclipse.app",
            "org.eclipse.platform",
            "C:\\tools\\eclipse\\eclipse.exe",
            "/opt/eclipse/eclipse",
        ] {
            assert_eq!(
                correct_category_with_window(
                    "Browsing",
                    Some(app),
                    Some("workspace - EmailService.java - Eclipse IDE"),
                ),
                "Coding",
                "{app}",
            );
        }
        let (_, category) = parse_analysis(
            "APP: Eclipse\nCURRENT ACTION: editing a Java source file in the code editor\nCATEGORY: Browsing",
        );
        assert_eq!(category, "Coding");
        assert_eq!(
            correct_category_with_window(
                "General",
                Some("Eclipse"),
                Some("debugger breakpoint - project")
            ),
            "Debugging",
        );
        assert_eq!(
            correct_category_with_window(
                "General",
                Some("Eclipse"),
                Some("JUnit test results - project")
            ),
            "Testing",
        );
        let (_, astronomy) = parse_analysis(
            "APP: Chrome\nCURRENT ACTION: reading news about a solar eclipse\nCATEGORY: Browsing",
        );
        assert_eq!(astronomy, "Browsing");
        let (_, welcome) = parse_analysis(
            "APP: Eclipse IDE\nCURRENT ACTION: viewing a generic welcome window\nCATEGORY: General",
        );
        assert_eq!(welcome, "General");
        let (_, discussion) = parse_analysis(
            "APP: Slack\nCURRENT ACTION: messages about Eclipse source code\nCATEGORY: Communication",
        );
        assert_eq!(discussion, "Communication");
        assert_eq!(
            correct_category_with_window(
                "Browsing",
                Some("Eclipse"),
                Some("embedded browser - news")
            ),
            "Browsing",
        );
    }

    #[test]
    fn historic_eclipse_labels_use_foreground_evidence_and_preserve_other_activities() {
        assert_eq!(
            resolve_activity_category(
                "Browsing",
                "editing controls",
                Some("Eclipse.exe"),
                Some("workspace - Main.java - Eclipse IDE"),
            ),
            "Coding",
        );
        assert_eq!(
            resolve_activity_category(
                "General",
                "CURRENT ACTION: using debugger breakpoints in Eclipse",
                Some("Eclipse"),
                Some("Problem Occurred"),
            ),
            "Debugging",
        );
        for category in ["Research", "Learning", "CodeReview", "Custom workflow"] {
            assert_eq!(
                resolve_activity_category(
                    category,
                    "Eclipse code editor",
                    Some("Eclipse"),
                    Some("workspace - Main.java - Eclipse IDE"),
                ),
                category,
            );
        }
        assert_eq!(
            resolve_activity_category(
                "Communication",
                "messages discussing Eclipse IDE source code",
                Some("Slack.exe"),
                Some("Team chat"),
            ),
            "Communication",
        );
        assert_eq!(
            resolve_activity_category(
                "General",
                "welcome screen",
                Some("Eclipse IDE"),
                Some("Welcome")
            ),
            "General",
        );
    }

    #[test]
    fn labelled_policy_fixture_reports_precision_recall_and_confusion() {
        #[derive(serde::Deserialize)]
        struct Case {
            id: String,
            raw: String,
            app: String,
            title: String,
            expected: String,
        }

        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../testdata/activity_classification_cases.json"
        ))
        .expect("valid labelled classification fixture");
        assert!(cases.len() >= 40, "classification eval needs a useful N");

        let mut correct = 0usize;
        let mut focus_true_positive = 0usize;
        let mut focus_false_positive = 0usize;
        let mut focus_false_negative = 0usize;
        let mut failures = Vec::new();
        let mut per_class = std::collections::BTreeMap::<String, (usize, usize, usize)>::new();

        for case in &cases {
            let (_, model_category) = parse_analysis(&case.raw);
            let predicted =
                correct_category_with_window(&model_category, Some(&case.app), Some(&case.title));
            let exact = predicted == case.expected;
            correct += usize::from(exact);
            if !exact {
                failures.push(format!(
                    "{}: expected {}, got {}",
                    case.id, case.expected, predicted
                ));
            }

            let expected_focus = crate::focus_semantics::focus_role(&case.expected)
                == crate::focus_semantics::FocusRole::Eligible;
            let predicted_focus = crate::focus_semantics::focus_role(&predicted)
                == crate::focus_semantics::FocusRole::Eligible;
            focus_true_positive += usize::from(expected_focus && predicted_focus);
            focus_false_positive += usize::from(!expected_focus && predicted_focus);
            focus_false_negative += usize::from(expected_focus && !predicted_focus);

            per_class.entry(case.expected.clone()).or_default().2 += 1;
            per_class.entry(predicted.clone()).or_default().1 += 1;
            if exact {
                per_class.entry(case.expected.clone()).or_default().0 += 1;
            }
        }

        let accuracy = correct as f64 / cases.len() as f64;
        let focus_precision =
            focus_true_positive as f64 / (focus_true_positive + focus_false_positive).max(1) as f64;
        let focus_recall =
            focus_true_positive as f64 / (focus_true_positive + focus_false_negative).max(1) as f64;
        let critical_non_focus = [
            "Planning",
            "Meeting",
            "Communication",
            "Sales",
            "Admin",
            "Browsing",
            "Idle",
            "General",
        ];
        for category in critical_non_focus {
            let (true_positive, predicted_count, expected_count) =
                per_class.get(category).copied().unwrap_or_default();
            let precision = true_positive as f64 / predicted_count.max(1) as f64;
            let recall = true_positive as f64 / expected_count.max(1) as f64;
            assert!(
                precision >= 0.90 && recall >= 0.90,
                "{category}: precision={precision:.3}, recall={recall:.3}"
            );
        }
        for category in crate::focus_semantics::allowed_categories_prompt().split(", ") {
            let expected_count = per_class.get(category).copied().unwrap_or_default().2;
            assert!(
                expected_count > 0,
                "classification fixture has no expected case for {category}"
            );
        }

        eprintln!(
            "classification_eval n={} accuracy={:.3} focus_precision={:.3} focus_recall={:.3}",
            cases.len(),
            accuracy,
            focus_precision,
            focus_recall
        );
        assert!(
            accuracy >= 0.95,
            "accuracy={accuracy:.3}; failures={failures:?}"
        );
        assert!(
            focus_precision >= 0.98,
            "focus precision={focus_precision:.3}"
        );
        assert!(focus_recall >= 0.95, "focus recall={focus_recall:.3}");
        assert_eq!(focus_false_positive, 0, "failures={failures:?}");
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
