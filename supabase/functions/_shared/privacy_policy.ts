export const CURRENT_PRIVACY_NOTICE_VERSION = "2026-08-23";

export type PrivacyExportSource = {
  table: string;
  column: string;
  select?: string;
};

export const PRIVACY_EXPORT_SOURCES: PrivacyExportSource[] = [
  {
    table: "profiles",
    column: "id",
    select:
      "id,display_name,avatar_url,role,jira_cloud_id,last_seen_at,created_at",
  },
  { table: "licenses", column: "owner_id" },
  { table: "teams", column: "owner_id" },
  { table: "team_members", column: "user_id" },
  { table: "work_sessions", column: "user_id" },
  { table: "activity_reports", column: "user_id" },
  { table: "mobile_activity_events", column: "user_id" },
  { table: "devices", column: "user_id" },
  { table: "cloud_insights", column: "user_id" },
  { table: "prompt_usage", column: "user_id" },
  { table: "privacy_preferences", column: "user_id" },
  {
    table: "notion_oauth_states",
    column: "user_id",
    select: "id,user_id,expires_at,consumed_at,created_at",
  },
  {
    table: "notion_connections",
    column: "user_id",
    select:
      "user_id,workspace_id,workspace_name,workspace_icon,bot_id,connected_at,updated_at",
  },
  { table: "notion_destinations", column: "user_id" },
  { table: "notion_publications", column: "user_id" },
];

const SECURITY_FIELDS: Record<string, string[]> = {
  profiles: ["jira_tokens"],
  notion_oauth_states: ["state_hash"],
  notion_connections: ["token_ciphertext", "token_iv", "encryption_version"],
};

export function assertSafePrivacyExportProjections(
  sources: PrivacyExportSource[] = PRIVACY_EXPORT_SOURCES,
): void {
  for (const [table, forbiddenFields] of Object.entries(SECURITY_FIELDS)) {
    const source = sources.find((candidate) => candidate.table === table);
    if (!source?.select || source.select.trim() === "*") {
      throw new Error(
        `${table} requires an explicit privacy-export projection.`,
      );
    }
    const selected = new Set(
      source.select.split(",").map((field) => field.trim().toLowerCase()),
    );
    for (const field of forbiddenFields) {
      if (selected.has(field)) {
        throw new Error(
          `${table}.${field} must not be included in a privacy export.`,
        );
      }
    }
  }
}
