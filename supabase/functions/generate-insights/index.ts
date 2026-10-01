import { createClient } from "https://esm.sh/@supabase/supabase-js@2.49.1";
import { CURRENT_PRIVACY_NOTICE_VERSION } from "../_shared/privacy_policy.ts";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers":
    "authorization, x-client-info, apikey, content-type",
};

const OPENROUTER_MODEL_DEFAULT = "xiaomi/mimo-v2.5-pro";

type ActivityRow = {
  category?: string;
  duration_seconds?: number;
  description?: string;
  captured_at?: string;
};

function roundHours(seconds: number) {
  return Math.round((seconds / 3600) * 10) / 10;
}

function aggregateRows(rows: ActivityRow[]) {
  const byCategory: Record<string, number> = {};
  let totalSeconds = 0;
  for (const row of rows) {
    const dur = row.duration_seconds ?? 0;
    totalSeconds += dur;
    const cat = row.category ?? "Other";
    byCategory[cat] = (byCategory[cat] ?? 0) + dur;
  }
  const topCategories = Object.entries(byCategory)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([category, seconds]) => ({ category, hours: roundHours(seconds) }));
  return { totalSeconds, topCategories, activityCount: rows.length };
}

async function callOpenRouterPmReport(payload: {
  periodDays: number;
  periodStart: string;
  periodEnd: string;
  cloudStats: ReturnType<typeof aggregateRows>;
  cloudSamples: ActivityRow[];
  localReport?: Record<string, unknown>;
}) {
  const apiKey = Deno.env.get("OPENROUTER_API_KEY");
  if (!apiKey) {
    throw new Error(
      "OPENROUTER_API_KEY is not configured in Supabase Edge Function secrets",
    );
  }

  const model = Deno.env.get("OPENROUTER_MODEL") ?? OPENROUTER_MODEL_DEFAULT;

  const prompt =
    `You are a privacy-first work-pattern analyst for an individual knowledge worker.
Generate a PM-style work report in JSON only (no markdown fences).

Use ONLY facts from the DATA below. Do not invent tasks or tools.
If DATA.localReport.focus_semantics exists, it is canonical: Deep Focus means observed sustained focus-eligible work without an observed theme change, not subjective flow. Theme continuity is only known from explicit manual labels or tickets; use explicit_theme_coverage_pct and state that unlabelled task switches may be missed. Never derive Deep Focus by summing Coding or another category. Use only focus_semantics.distraction_events/distraction_seconds for distraction claims; raw Browsing rows can include sub-threshold observations. Its context_category_mix retains meetings, planning, communication, administration and sales as useful evidence about coordination, workload and transitions even though they do not count toward Deep Focus. Never call that contextual work distraction. If focus_semantics is absent, state that canonical Deep Focus is unavailable and do not estimate it from categories or cloud totals; distraction episodes are also unavailable and must not be reconstructed from raw Browsing. Tie every recommendation to a supplied metric or state that the signal is insufficient. Do not prescribe universal recovery times or ultradian cycles; describe focus_semantics.deep_threshold_seconds as a transparent product reference, not a biological threshold.

Return this JSON shape:
{
  "executive_summary": "2-3 sentences",
  "focus_analysis": "paragraph about sustained work across any relevant profession",
  "distraction_patterns": "paragraph using only canonical distraction episodes and observed theme switches",
  "week_trend": "compare daily totals if available",
  "measurement_notes": "brief explanation of coverage, uncertainty, and the observable proxy",
  "recommendations": ["action 1", "action 2", "action 3"],
  "highlights": ["bullet 1", "bullet 2", "bullet 3"]
}

DATA:
${JSON.stringify(payload, null, 2)}`;

  const response = await fetch(
    "https://openrouter.ai/api/v1/chat/completions",
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
        "HTTP-Referer": "https://flowsight.site",
        "X-Title": "FlowSight Individual Insights",
      },
      body: JSON.stringify({
        model,
        messages: [{ role: "user", content: prompt }],
        temperature: 0.35,
        response_format: { type: "json_object" },
      }),
    },
  );

  if (!response.ok) {
    throw new Error(`OpenRouter request failed (${response.status})`);
  }

  const json = await response.json();
  const raw = json?.choices?.[0]?.message?.content;
  if (!raw || typeof raw !== "string") {
    throw new Error("OpenRouter returned empty content");
  }

  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new Error("OpenRouter returned non-JSON content");
  }

  return { parsed, model };
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }

  try {
    const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
    const supabaseAnonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
    const serviceRoleKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
    const authHeader = req.headers.get("Authorization");
    if (!authHeader) {
      return new Response(
        JSON.stringify({ error: "Missing Authorization header" }),
        {
          status: 401,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }

    const userClient = createClient(supabaseUrl, supabaseAnonKey, {
      global: { headers: { Authorization: authHeader } },
    });

    const { data: userData, error: userError } = await userClient.auth
      .getUser();
    if (userError || !userData.user) {
      return new Response(JSON.stringify({ error: "Unauthorized" }), {
        status: 401,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    const { data: entitlements, error: entError } = await userClient.rpc(
      "get_user_entitlements",
    );
    if (entError) {
      return new Response(
        JSON.stringify({ error: "Could not verify your plan." }),
        {
          status: 400,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }

    if (!entitlements?.features?.cloud_ai) {
      return new Response(
        JSON.stringify({ error: "Cloud AI requires an active license" }),
        {
          status: 403,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }

    const body = await req.json().catch(() => ({}));
    const periodDays = Math.min(Math.max(Number(body.period_days) || 7, 1), 30);
    const teamId = body.team_id as string | undefined;
    const localReport = body.local_report as
      | Record<string, unknown>
      | undefined;
    const plan = (body.plan as string | undefined) ?? entitlements?.plan ??
      null;

    if (!serviceRoleKey) {
      return new Response(
        JSON.stringify({ error: "Privacy enforcement is unavailable" }),
        {
          status: 503,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }
    const serviceClient = createClient(supabaseUrl, serviceRoleKey, {
      auth: { persistSession: false },
    });
    const { data: privacyPreference, error: privacyError } = await serviceClient
      .from("privacy_preferences")
      .select("notice_version,cloud_sync_enabled,cloud_ai_enabled")
      .eq("user_id", userData.user.id)
      .maybeSingle();
    if (
      privacyError ||
      privacyPreference?.notice_version !== CURRENT_PRIVACY_NOTICE_VERSION ||
      privacyPreference?.cloud_sync_enabled !== true
    ) {
      return new Response(
        JSON.stringify({
          error:
            "Enable cloud activity sync in FlowSight's Privacy & data settings first.",
        }),
        {
          status: 403,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }
    if (plan === "individual" && privacyPreference.cloud_ai_enabled !== true) {
      return new Response(
        JSON.stringify({
          error:
            "Enable cloud AI sharing in FlowSight's Privacy & data settings first.",
        }),
        {
          status: 403,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }

    const periodEnd = new Date();
    const periodStart = new Date();
    periodStart.setDate(periodEnd.getDate() - periodDays);

    const periodStartStr = periodStart.toISOString().slice(0, 10);
    const periodEndStr = periodEnd.toISOString().slice(0, 10);

    let reportsQuery = userClient
      .from("activity_reports")
      .select("category, duration_seconds, description, captured_at")
      .gte("captured_at", periodStart.toISOString())
      .lte("captured_at", periodEnd.toISOString())
      .order("captured_at", { ascending: false })
      .limit(500);

    if (teamId) {
      reportsQuery = reportsQuery.eq("team_id", teamId);
    }

    const { data: cloudReports, error: reportsError } = await reportsQuery;
    if (reportsError) {
      return new Response(
        JSON.stringify({ error: "Could not load activity data." }),
        {
          status: 400,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }

    const cloudRows = cloudReports ?? [];
    const cloudStats = aggregateRows(cloudRows);

    let content: Record<string, unknown>;
    let insightType = "weekly_summary";

    if (plan === "individual") {
      insightType = "pm_individual_report";
      const { parsed, model } = await callOpenRouterPmReport({
        periodDays,
        periodStart: periodStartStr,
        periodEnd: periodEndStr,
        cloudStats,
        cloudSamples: cloudRows.slice(0, 40),
        localReport,
      });

      content = {
        ...parsed,
        period_days: periodDays,
        period_start: periodStartStr,
        period_end: periodEndStr,
        total_hours: localReport?.total_hours ??
          roundHours(cloudStats.totalSeconds),
        activity_count: localReport?.activity_count ?? cloudStats.activityCount,
        deep_focus_hours: localReport?.deep_focus_hours ?? null,
        focus_semantics: localReport?.focus_semantics ?? null,
        distraction_events: localReport?.distraction_events ?? null,
        top_categories: localReport?.category_breakdown ??
          cloudStats.topCategories,
        daily_totals: localReport?.daily_totals ?? [],
        data_sources: [
          localReport ? "local_sqlite" : null,
          cloudRows.length > 0 ? "cloud_sync" : null,
        ].filter(Boolean),
        model,
        generated_at: new Date().toISOString(),
      };
    } else {
      const summary = cloudRows.length === 0 && !localReport
        ? `No synced activity found in the last ${periodDays} days.`
        : `Over the last ${periodDays} days: ${
          roundHours(cloudStats.totalSeconds)
        }h logged across ${cloudStats.activityCount} synced activities.`;

      content = {
        summary,
        period_days: periodDays,
        total_hours: roundHours(cloudStats.totalSeconds),
        activity_count: cloudStats.activityCount,
        deep_focus_hours: localReport?.deep_focus_hours ?? null,
        focus_semantics: localReport?.focus_semantics ?? null,
        distraction_events: localReport?.distraction_events ?? null,
        top_categories: cloudStats.topCategories,
        generated_at: new Date().toISOString(),
      };
    }

    const resolvedTeamId = teamId ??
      (Array.isArray(entitlements.team_ids) && entitlements.team_ids.length > 0
        ? entitlements.team_ids[0]
        : null);

    const { data: inserted, error: insertError } = await userClient
      .from("cloud_insights")
      .insert({
        user_id: userData.user.id,
        team_id: resolvedTeamId,
        period_start: localReport?.period_start ?? periodStartStr,
        period_end: localReport?.period_end ?? periodEndStr,
        insight_type: insightType,
        content,
      })
      .select("*")
      .single();

    if (insertError) {
      return new Response(
        JSON.stringify({ error: "Could not save the generated insight." }),
        {
          status: 400,
          headers: { ...corsHeaders, "Content-Type": "application/json" },
        },
      );
    }

    return new Response(JSON.stringify({ insight: inserted }), {
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  } catch {
    return new Response(
      JSON.stringify({ error: "Insight generation failed." }),
      {
        status: 500,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      },
    );
  }
});
