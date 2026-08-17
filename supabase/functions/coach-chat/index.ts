import { createClient } from "https://esm.sh/@supabase/supabase-js@2.49.1";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
};

const DEFAULT_AZURE_ENDPOINT =
  "https://france-flow.services.ai.azure.com/api/projects/france/openai/v1";
const DEFAULT_AZURE_DEPLOYMENT = "Mistral-Large-3";
const MAX_MESSAGE_LEN = 500;
const MAX_HISTORY = 12;

const COACH_SYSTEM_PROMPT = `You are FlowSight, a senior team productivity and cognitive-health advisor (engineering-manager + agile-coach level). Privacy-first.
Answer ONLY about focus, flow state, meetings, context switching, sprint planning, team activity, and uploaded documents when provided.
Use ONLY the team stats and documents in the user message — never invent metrics, names, or policies.

Before your visible answer, reason inside <thinking>...</thinking> tags (3–6 bullet notes: which metrics you checked, what patterns you see, what you will recommend and why).
Then write the user-facing reply inside <answer>...</answer> tags only.

For greetings or one-line questions: reply in 1–2 sentences.

For substantive questions, write like a trusted industry expert — detailed, practical, and step-by-step.

STRICT FORMAT (follow exactly — the UI renders Markdown structure, not bold walls of text):
1) # One-line thesis title (the bottom line).
2) One short opening paragraph in normal text. Bold only 3–8 words for the key takeaway — never wrap the whole paragraph in **.
3) Blank line, then ## What the Data Shows (or Spanish equivalent).
4) Bullet list: one metric per line, each line starting with "- " on its own line. Cite metrics inline.
5) Blank line, then ## What to Do Now (or ## Recommended Actions).
6) Numbered action plan: each step on its own line starting with "1. ", "2. ", "3. ", etc. (4–6 steps). Under each step, sub-bullets on separate lines starting with "- ".
7) Optional ## Next Steps with 2–3 bullets.

Rules:
- Use # and ## for all section titles. Never use **bold** as a section heading.
- CRITICAL: each numbered step MUST be on its own line ("1. ..." then newline then "2. ..."). Never write "1. ... 2. ..." on the same line.
- Each sub-bullet MUST be on its own line starting with "- ".
- Use **bold** at most once per step (short label only). Never bold entire steps or paragraphs.
- Put a blank line between every section and before every list.
Avoid vague advice. Sound authoritative but humane.

If data is missing, say what is unavailable and suggest tracking more activity in FlowSight.`;

type HistoryMessage = { role: "user" | "assistant"; content: string };

type ParsedCoachResponse = {
  answer: string;
  reasoning: string | null;
};

const PLAN_LIMITS: Record<string, number> = {
  individual: 150,
  individual_pro: 150,
  team: 250,
  teams_pro: 250,
  teams_simple: 50,
  enterprise: 500,
};

function periodStart(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;
}

function resolvePromptLimit(plan: string | null | undefined): number {
  if (!plan) return 0;
  return PLAN_LIMITS[plan] ?? (plan === "individual" ? 150 : plan === "team" ? 250 : 0);
}

function stripThinkingTags(text: string): string {
  return text
    .replace(/<thinking>[\s\S]*?<\/thinking>/gi, "")
    .replace(/<\/?answer>/gi, "")
    .trim();
}

function parseCoachResponse(raw: string): ParsedCoachResponse {
  const trimmed = raw.trim();
  if (!trimmed) return { answer: "", reasoning: null };

  const thinkingMatch = trimmed.match(/<thinking>([\s\S]*?)<\/thinking>/i);
  const answerMatch = trimmed.match(/<answer>([\s\S]*?)<\/answer>/i);

  if (thinkingMatch || answerMatch) {
    const reasoning = thinkingMatch?.[1]?.trim() || null;
    const answer = answerMatch?.[1]?.trim() || stripThinkingTags(trimmed);
    return { answer: answer || trimmed, reasoning };
  }

  return { answer: trimmed, reasoning: null };
}

async function getPromptUsed(
  serviceClient: ReturnType<typeof createClient>,
  userId: string,
  teamId: string | null,
): Promise<number | null> {
  if (!teamId) return 0;
  const period = periodStart();
  const { data, error } = await serviceClient
    .from("prompt_usage")
    .select("user_prompt_count")
    .eq("user_id", userId)
    .eq("team_id", teamId)
    .eq("period_start", period)
    .maybeSingle();

  if (error) {
    if (error.code === "42P01" || error.message?.includes("does not exist")) return null;
    throw error;
  }
  return data?.user_prompt_count ?? 0;
}

async function incrementPromptUsed(
  serviceClient: ReturnType<typeof createClient>,
  userId: string,
  teamId: string | null,
  used: number,
): Promise<void> {
  if (!teamId) return;
  const period = periodStart();
  const { error } = await serviceClient.from("prompt_usage").upsert(
    {
      user_id: userId,
      team_id: teamId,
      period_start: period,
      user_prompt_count: used + 1,
    },
    { onConflict: "user_id,team_id,period_start" },
  );
  if (error && error.code !== "42P01" && !error.message?.includes("does not exist")) {
    throw error;
  }
}

async function callAzureCoach(
  system: string,
  userContent: string,
  history: HistoryMessage[],
): Promise<ParsedCoachResponse> {
  const apiKey = Deno.env.get("AZURE_OPENAI_API_KEY");
  if (!apiKey) {
    throw new Error(
      "AZURE_OPENAI_API_KEY is not configured in Supabase Edge Function secrets",
    );
  }

  const base = (Deno.env.get("AZURE_OPENAI_ENDPOINT") ?? DEFAULT_AZURE_ENDPOINT).replace(/\/$/, "");
  const deployment =
    Deno.env.get("AZURE_OPENAI_DEPLOYMENT") ??
    Deno.env.get("AZURE_OPENAI_MODEL") ??
    DEFAULT_AZURE_DEPLOYMENT;

  const messages = [
    { role: "system", content: system },
    ...history.slice(-MAX_HISTORY).map((m) => ({ role: m.role, content: m.content })),
    { role: "user", content: userContent },
  ];

  const response = await fetch(`${base}/chat/completions`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      model: deployment,
      messages,
      temperature: 0.35,
      max_tokens: 1400,
    }),
  });

  if (!response.ok) {
    const errText = await response.text();
    throw new Error(`Azure OpenAI error (${response.status}): ${errText.slice(0, 300)}`);
  }

  const json = await response.json();
  const raw = json?.choices?.[0]?.message?.content;
  if (!raw || typeof raw !== "string") {
    throw new Error("Azure OpenAI returned empty content");
  }

  return parseCoachResponse(raw.trim());
}

function buildHistoryBlock(history: HistoryMessage[]): string {
  if (!history.length) return "";
  const lines = history
    .slice(-8)
    .map((m) => `${m.role === "user" ? "User" : "Coach"}: ${m.content}`)
    .join("\n");
  return `Recent conversation:\n${lines}`;
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }

  const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
  const supabaseAnonKey = Deno.env.get("SUPABASE_ANON_KEY")!;
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
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

  const userId = userData.user.id;

  try {
    const { data: entitlements, error: entError } = await userClient.rpc("get_user_entitlements");
    if (entError) {
      return new Response(JSON.stringify({ error: entError.message }), {
        status: 400,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    const plan = (entitlements?.plan as string | undefined) ?? null;
    const limit = resolvePromptLimit(plan);
    const teamIds = Array.isArray(entitlements?.team_ids) ? entitlements.team_ids : [];
    const defaultTeamId = teamIds[0] ?? null;

    const serviceClient = serviceKey
      ? createClient(supabaseUrl, serviceKey)
      : null;

    if (req.method === "GET") {
      const url = new URL(req.url);
      const teamId = url.searchParams.get("teamId") ?? defaultTeamId;
      const allowed = Boolean(entitlements?.features?.cloud_ai) && limit > 0;
      let used = 0;

      if (serviceClient && teamId) {
        const counted = await getPromptUsed(serviceClient, userId, teamId);
        if (counted !== null) used = counted;
      }

      return new Response(
        JSON.stringify({
          usage: {
            used,
            limit,
            remaining: Math.max(0, limit - used),
            planId: plan ?? "free",
            allowed: allowed && (limit === 0 || used < limit),
          },
        }),
        { headers: { ...corsHeaders, "Content-Type": "application/json" } },
      );
    }

    if (req.method !== "POST") {
      return new Response(JSON.stringify({ error: "Method not allowed" }), {
        status: 405,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    if (!entitlements?.features?.cloud_ai || limit <= 0) {
      return new Response(
        JSON.stringify({
          error: "Upgrade to Pro to unlock your AI coach.",
          usage: { used: 0, limit: 0, remaining: 0, planId: plan ?? "free", allowed: false },
        }),
        { status: 403, headers: { ...corsHeaders, "Content-Type": "application/json" } },
      );
    }

    const body = await req.json().catch(() => ({}));
    const message = String(body.message ?? "").trim();
    const teamId = (body.team_id as string | undefined) ?? defaultTeamId;
    const history = (Array.isArray(body.history) ? body.history : []) as HistoryMessage[];
    const localContext = body.local_context ?? null;

    if (!message || message.length > MAX_MESSAGE_LEN) {
      return new Response(JSON.stringify({ error: "Invalid message" }), {
        status: 400,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    let used = 0;
    if (serviceClient && teamId) {
      const counted = await getPromptUsed(serviceClient, userId, teamId);
      if (counted !== null) {
        used = counted;
        if (used >= limit) {
          return new Response(
            JSON.stringify({
              error: "Monthly coach limit reached. Resets on the 1st.",
              usage: { used, limit, remaining: 0, planId: plan ?? "free", allowed: false },
            }),
            { status: 429, headers: { ...corsHeaders, "Content-Type": "application/json" } },
          );
        }
      }
    }

    const contextBlock = localContext
      ? `Activity context (last 7 days):\n${JSON.stringify(localContext, null, 2)}`
      : "Activity context: no local stats provided.";
    const historyBlock = buildHistoryBlock(history);
    const userPrompt = [contextBlock, historyBlock, `Question: ${message}`].filter(Boolean).join("\n\n");

    const coachResult = await callAzureCoach(COACH_SYSTEM_PROMPT, userPrompt, history);
    const reply = coachResult.answer.trim();
    if (!reply) {
      return new Response(JSON.stringify({ error: "AI coach returned an empty response." }), {
        status: 503,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }

    if (serviceClient && teamId) {
      await incrementPromptUsed(serviceClient, userId, teamId, used);
      used += 1;
    }

    return new Response(
      JSON.stringify({
        reply,
        reasoning: coachResult.reasoning,
        usage: {
          used,
          limit,
          remaining: Math.max(0, limit - used),
          planId: plan ?? "free",
          allowed: used < limit,
        },
      }),
      { headers: { ...corsHeaders, "Content-Type": "application/json" } },
    );
  } catch (error) {
    return new Response(JSON.stringify({ error: String(error) }), {
      status: 500,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }
});
