import { createClient } from "https://esm.sh/@supabase/supabase-js@2.49.1";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
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

  const prompt = `You are a productivity analyst for a solo developer (Individual plan). 
Generate a PM-style work report in JSON only (no markdown fences).

Use ONLY facts from the DATA below. Do not invent tasks or tools.

Return this JSON shape:
{
  "executive_summary": "2-3 sentences",
  "focus_analysis": "paragraph about deep work / coding focus",
  "distraction_patterns": "paragraph about distractions or context switching",
  "week_trend": "compare daily totals if available",
  "productivity_score": 0-100 integer,
  "recommendations": ["action 1", "action 2", "action 3"],
  "highlights": ["bullet 1", "bullet 2", "bullet 3"]
}

DATA:
${JSON.stringify(payload, null, 2)}`;

  const response = await fetch("https://openrouter.ai/api/v1/chat/completions", {
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
  });

  if (!response.ok) {
    const errText = await response.text();
    throw new Error(`OpenRouter error (${response.status}): ${errText}`);
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
    const authHeader = req.headers.get("Authorization");
    if (!authHeader) {
      return new Response(JSON.stringify({ error: "Missing Authorization header" }), {
        status: 401,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    const userClient = createClient(supabaseUrl, supabaseAnonKey, {
      global: { headers: { Authorization: authHeader } },
    });

    const { data: userData, error: userError } = await userClient.auth.getUser();
    if (userError || !userData.user) {
      return new Response(JSON.stringify({ error: "Unauthorized" }), {
        status: 401,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    const { data: entitlements, error: entError } = await userClient.rpc("get_user_entitlements");
    if (entError) {
      return new Response(JSON.stringify({ error: entError.message }), {
        status: 400,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    if (!entitlements?.features?.cloud_ai) {
      return new Response(JSON.stringify({ error: "Cloud AI requires an active license" }), {
        status: 403,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    const body = await req.json().catch(() => ({}));
    const periodDays = Math.min(Math.max(Number(body.period_days) || 7, 1), 30);
    const teamId = body.team_id as string | undefined;
    const localReport = body.local_report as Record<string, unknown> | undefined;
    const plan = (body.plan as string | undefined) ?? entitlements?.plan ?? null;

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
      return new Response(JSON.stringify({ error: reportsError.message }), {
        status: 400,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
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
        total_hours: localReport?.total_hours ?? roundHours(cloudStats.totalSeconds),
        activity_count: localReport?.activity_count ?? cloudStats.activityCount,
        focus_hours: localReport?.focus_hours ?? null,
        distraction_events: localReport?.distraction_events ?? null,
        top_categories: localReport?.category_breakdown ?? cloudStats.topCategories,
        daily_totals: localReport?.daily_totals ?? [],
        data_sources: ["local_sqlite", cloudRows.length > 0 ? "cloud_sync" : null].filter(Boolean),
        model,
        generated_at: new Date().toISOString(),
      };
    } else {
      const summary = cloudRows.length === 0 && !localReport
        ? `No synced activity found in the last ${periodDays} days.`
        : `Over the last ${periodDays} days: ${roundHours(cloudStats.totalSeconds)}h logged across ${cloudStats.activityCount} synced activities.`;

      content = {
        summary,
        period_days: periodDays,
        total_hours: roundHours(cloudStats.totalSeconds),
        activity_count: cloudStats.activityCount,
        top_categories: cloudStats.topCategories,
        generated_at: new Date().toISOString(),
      };
    }

    const resolvedTeamId =
      teamId ??
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
      return new Response(JSON.stringify({ error: insertError.message }), {
        status: 400,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    return new Response(JSON.stringify({ insight: inserted }), {
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  } catch (error) {
    return new Response(JSON.stringify({ error: String(error) }), {
      status: 500,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }
});
