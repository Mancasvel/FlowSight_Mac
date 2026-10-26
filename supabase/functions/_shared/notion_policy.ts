export const NOTION_API_VERSION = "2026-03-11";
export const PRO_PLAN_IDS = new Set(["pro", "individual", "individual_pro"]);

export type Entitlements = {
  plan?: string | null;
  status?: string | null;
  features?: { integrations?: boolean } | null;
};

export type ProGateResult =
  | { allowed: true }
  | { allowed: false; status: 403; code: "notion_requires_pro"; error: string };

export function requireNotionPro(entitlements: Entitlements | null | undefined): ProGateResult {
  const allowed = entitlements?.status === "active" &&
    PRO_PLAN_IDS.has(entitlements?.plan ?? "") &&
    entitlements?.features?.integrations === true;
  return allowed
    ? { allowed: true }
    : {
      allowed: false,
      status: 403,
      code: "notion_requires_pro",
      error: "Notion publishing requires an active FlowSight Pro plan.",
    };
}

export type RichText = {
  type: "text";
  text: { content: string };
  annotations?: { bold?: boolean };
};

export type NotionBlock = Record<string, unknown>;

export type CanonicalReport = {
  periodStart: string;
  periodEnd: string;
  policyVersion: string;
  title: string;
  markdown: string;
  blocks: NotionBlock[];
};

type FocusSemantics = Record<string, unknown>;
type LocalReport = Record<string, unknown> & { focus_semantics?: FocusSemantics };

function finiteMetric(source: FocusSemantics, key: string, nullable = false): number | null {
  const value = source[key];
  if (nullable && value == null) return null;
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new Error(`Canonical focus_semantics.${key} is missing or invalid.`);
  }
  return value;
}

function stringMetric(source: FocusSemantics, key: string): string {
  const value = source[key];
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`Canonical focus_semantics.${key} is missing or invalid.`);
  }
  return value;
}

function periodValue(report: LocalReport, key: "period_start" | "period_end"): string {
  const value = report[key];
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    throw new Error(`Canonical local report ${key} is missing or invalid.`);
  }
  return value;
}

function minutes(seconds: number | null): string {
  if (seconds == null) return "Not observed";
  return `${Math.round(seconds / 6) / 10} min`;
}

function percent(value: number | null): string {
  return `${Math.round((value ?? 0) * 10) / 10}%`;
}

function numberText(value: number | null): string {
  return String(Math.round((value ?? 0) * 10) / 10);
}

function text(content: string, bold = false): RichText {
  return {
    type: "text",
    text: { content: content.slice(0, 2000) },
    ...(bold ? { annotations: { bold: true } } : {}),
  };
}

function paragraph(content: string): NotionBlock {
  return { object: "block", type: "paragraph", paragraph: { rich_text: [text(content)] } };
}

function heading(level: 1 | 2, content: string): NotionBlock {
  const type = `heading_${level}`;
  return { object: "block", type, [type]: { rich_text: [text(content)] } };
}

function bullet(content: string): NotionBlock {
  return {
    object: "block",
    type: "bulleted_list_item",
    bulleted_list_item: { rich_text: [text(content)] },
  };
}

function tableRow(cells: string[], header = false): NotionBlock {
  return {
    object: "block",
    type: "table_row",
    table_row: { cells: cells.map((cell) => [text(cell, header)]) },
  };
}

function escapeMarkdown(value: string): string {
  return value.replaceAll("|", "\\|").replaceAll("\n", " ");
}

export function formatCanonicalNotionReport(localReport: LocalReport): CanonicalReport {
  const focus = localReport.focus_semantics;
  if (!focus || typeof focus !== "object" || Array.isArray(focus)) {
    throw new Error("Canonical local_report.focus_semantics is required; category totals are not a fallback.");
  }

  const periodStart = periodValue(localReport, "period_start");
  const periodEnd = periodValue(localReport, "period_end");
  const policyVersion = stringMetric(focus, "policy_version");
  const constructLabel = stringMetric(focus, "construct_label");
  const proxyDisclaimer = stringMetric(focus, "proxy_disclaimer");
  const focusedThreshold = finiteMetric(focus, "focused_threshold_seconds")!;
  const deepThreshold = finiteMetric(focus, "deep_threshold_seconds")!;
  const extendedThreshold = finiteMetric(focus, "extended_threshold_seconds")!;
  const deepSeconds = finiteMetric(focus, "deep_focus_seconds")!;
  const deepSessions = finiteMetric(focus, "deep_focus_sessions")!;
  const longestSeconds = finiteMetric(focus, "longest_focus_seconds")!;
  const fragmentation = finiteMetric(focus, "fragmentation_pct")!;
  const explicitThemeCoverage = finiteMetric(focus, "explicit_theme_coverage_pct")!;
  const themeSwitches = finiteMetric(focus, "theme_switches")!;
  const themeSwitchRate = finiteMetric(
    focus,
    "explicit_theme_switches_per_labelled_focus_hour",
  )!;
  const resumeEvents = finiteMetric(focus, "resume_events")!;
  const averageResumeSeconds = finiteMetric(focus, "average_resume_seconds", true);
  const browsingEvents = finiteMetric(focus, "distraction_events")!;
  const browsingSeconds = finiteMetric(focus, "distraction_seconds")!;
  const contextSeconds = finiteMetric(focus, "context_work_seconds")!;

  const categoryMix = Array.isArray(focus.context_category_mix)
    ? focus.context_category_mix.flatMap((item) => {
      if (!item || typeof item !== "object" || Array.isArray(item)) return [];
      const category = (item as Record<string, unknown>).category;
      const seconds = (item as Record<string, unknown>).seconds;
      return typeof category === "string" && typeof seconds === "number" && seconds >= 0
        ? [{ category, seconds }]
        : [];
    })
    : [];

  const title = `FlowSight report · ${periodStart} to ${periodEnd}`;
  const summary = `${minutes(deepSeconds)} in ${numberText(deepSessions)} Deep block(s); ` +
    `the longest observed block was ${minutes(longestSeconds)}. ` +
    `${minutes(contextSeconds)} of valuable context work remains visible outside the sustained-work construct.`;

  const metrics: Array<[string, string]> = [
    ["Deep minutes", minutes(deepSeconds)],
    ["Deep blocks", numberText(deepSessions)],
    ["Longest observed block", minutes(longestSeconds)],
    ["Fragmentation", percent(fragmentation)],
    ["Explicit theme coverage", percent(explicitThemeCoverage)],
    ["Observed explicit theme switches", numberText(themeSwitches)],
    ["Explicit theme switches / labelled focus hour", numberText(themeSwitchRate)],
    ["Observed resume episodes", numberText(resumeEvents)],
    ["Average observed resume time", minutes(averageResumeSeconds)],
    ["Sustained non-work browsing episodes", numberText(browsingEvents)],
    ["Sustained non-work browsing time", minutes(browsingSeconds)],
    ["Context work", minutes(contextSeconds)],
  ];

  const tierCopy = `Focused ≥ ${minutes(focusedThreshold)} · Deep ≥ ${minutes(deepThreshold)} · ` +
    `Extended ≥ ${minutes(extendedThreshold)}. These are transparent product reference tiers, not biological thresholds.`;
  const categoryBullets = categoryMix.length > 0
    ? categoryMix.map(({ category, seconds }) => `${category}: ${minutes(seconds)}`)
    : ["No context-work category mix was observed for this period."];

  const blocks: NotionBlock[] = [
    heading(1, title),
    paragraph(summary),
    heading(2, "Canonical sustained-work metrics"),
    {
      object: "block",
      type: "table",
      table: {
        table_width: 2,
        has_column_header: true,
        has_row_header: false,
        children: [tableRow(["Metric", "Value"], true), ...metrics.map((row) => tableRow(row))],
      },
    },
    heading(2, "Focused / Deep / Extended"),
    paragraph(tierCopy),
    heading(2, "Context work"),
    ...categoryBullets.map(bullet),
    heading(2, "Measurement notes"),
    paragraph(`${constructLabel}. ${proxyDisclaimer}`),
    paragraph(
      `Explicit theme coverage was ${percent(explicitThemeCoverage)}; unlabelled task changes may not be observed. ` +
      `Planning, meetings, communication, administration, sales, research, design, documentation and other context work are retained as evidence and are not labelled unproductive.`,
    ),
    paragraph(`Focus semantics policy: ${policyVersion}`),
  ];

  const markdownMetrics = metrics
    .map(([metric, value]) => `| ${escapeMarkdown(metric)} | ${escapeMarkdown(value)} |`)
    .join("\n");
  const markdown = [
    `# ${title}`,
    "",
    summary,
    "",
    "## Canonical sustained-work metrics",
    "",
    "| Metric | Value |",
    "| --- | --- |",
    markdownMetrics,
    "",
    "## Focused / Deep / Extended",
    "",
    tierCopy,
    "",
    "## Context work",
    "",
    ...categoryBullets.map((line) => `- ${escapeMarkdown(line)}`),
    "",
    "## Measurement notes",
    "",
    `${constructLabel}. ${proxyDisclaimer}`,
    "",
    `Explicit theme coverage was ${percent(explicitThemeCoverage)}; unlabelled task changes may not be observed. Planning, meetings, communication, administration, sales, research, design, documentation and other context work are retained as evidence and are not labelled unproductive.`,
    "",
    `Focus semantics policy: ${policyVersion}`,
  ].join("\n");

  return { periodStart, periodEnd, policyVersion, title, markdown, blocks };
}

export async function sha256Hex(value: string): Promise<string> {
  const bytes = new TextEncoder().encode(value);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function publicationKey(input: {
  userId: string;
  destinationId: string;
  reportMode: "period_page" | "live_page";
  periodStart: string;
  periodEnd: string;
  policyVersion: string;
}): Promise<string> {
  const period = input.reportMode === "live_page"
    ? "live"
    : `${input.periodStart}:${input.periodEnd}`;
  return sha256Hex(`${input.userId}:${input.destinationId}:${input.reportMode}:${period}`);
}
