import { corsHeaders, authenticate, errorResponse, isResponse, jsonResponse } from "../_shared/http.ts";
import { decryptTokenBundle, NotionApiClient } from "../_shared/notion_api.ts";
import {
  formatCanonicalNotionReport,
  publicationKey,
  requireNotionPro,
} from "../_shared/notion_policy.ts";
import {
  coordinatePublication,
  type PublicationRecord,
  type PublicationRepository,
} from "../_shared/notion_publish.ts";

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });
  if (req.method !== "POST") return errorResponse("Method not allowed.", 405, "method_not_allowed");

  const auth = await authenticate(req);
  if (isResponse(auth)) return auth;

  // The server-side entitlement check deliberately runs before reading the
  // report body, stored Notion token, destination, or calling Notion.
  const { data: entitlements, error: entitlementError } = await auth.userClient.rpc(
    "get_user_entitlements",
  );
  if (entitlementError) return errorResponse("Could not verify your plan.", 503, "entitlement_check_failed");
  const gate = requireNotionPro(entitlements);
  if (!gate.allowed) return errorResponse(gate.error, gate.status, gate.code);

  const body = await req.json().catch(() => ({})) as Record<string, unknown>;
  const localReport = body.local_report;
  if (!localReport || typeof localReport !== "object" || Array.isArray(localReport)) {
    return errorResponse("A canonical local report is required.", 400, "canonical_report_required");
  }

  let report;
  try {
    report = formatCanonicalNotionReport(localReport as Record<string, unknown>);
  } catch (error) {
    return errorResponse(
      error instanceof Error ? error.message : "Canonical report is invalid.",
      422,
      "invalid_focus_semantics",
    );
  }

  const requestedDestinationId = typeof body.destination_id === "string" ? body.destination_id : null;
  let destinationQuery = auth.serviceClient.from("notion_destinations")
    .select("id,notion_object_id,destination_type,title_property,report_mode")
    .eq("user_id", auth.userId);
  destinationQuery = requestedDestinationId
    ? destinationQuery.eq("id", requestedDestinationId)
    : destinationQuery.eq("is_default", true);
  const { data: destination, error: destinationError } = await destinationQuery.maybeSingle();
  if (destinationError || !destination) {
    return errorResponse("Choose a Notion destination before publishing.", 409, "destination_required");
  }

  const { data: connection, error: connectionError } = await auth.serviceClient
    .from("notion_connections")
    .select("token_ciphertext,token_iv,encryption_version")
    .eq("user_id", auth.userId).maybeSingle();
  const encryptionKey = Deno.env.get("NOTION_TOKEN_ENCRYPTION_KEY");
  if (connectionError || !connection || !encryptionKey) {
    return errorResponse("Connect Notion before publishing.", 409, "notion_not_connected");
  }

  let notion: NotionApiClient;
  try {
    const token = await decryptTokenBundle(connection, encryptionKey);
    notion = new NotionApiClient(token.access_token);
  } catch {
    return errorResponse("The Notion connection must be renewed.", 409, "notion_reconnect_required");
  }

  const reportMode = destination.report_mode as "period_page" | "live_page";
  const requestId = await publicationKey({
    userId: auth.userId,
    destinationId: destination.id,
    reportMode,
    periodStart: report.periodStart,
    periodEnd: report.periodEnd,
    policyVersion: report.policyVersion,
  });

  const repository: PublicationRepository = {
    async claim() {
      const now = new Date().toISOString();
      const { data: inserted, error } = await auth.serviceClient.from("notion_publications").insert({
        user_id: auth.userId,
        destination_id: destination.id,
        report_mode: reportMode,
        period_start: report.periodStart,
        period_end: report.periodEnd,
        policy_version: report.policyVersion,
        idempotency_key: requestId,
        status: "pending",
        updated_at: now,
      }).select("id,status,notion_page_id,notion_page_url").single();
      if (!error && inserted) return { created: true, record: inserted as PublicationRecord };
      if (error?.code !== "23505") throw new Error("Could not reserve the publication.");

      const { data: existing, error: existingError } = await auth.serviceClient
        .from("notion_publications")
        .select("id,status,notion_page_id,notion_page_url")
        .eq("user_id", auth.userId).eq("idempotency_key", requestId).single();
      if (existingError || !existing) throw new Error("Could not load the existing publication.");
      if (existing.status === "failed") {
        const { data: retry, error: retryError } = await auth.serviceClient
          .from("notion_publications")
          .update({ status: "pending", failure_code: null, updated_at: now })
          .eq("id", existing.id).eq("status", "failed")
          .select("id,status,notion_page_id,notion_page_url").single();
        if (!retryError && retry) return { created: true, record: retry as PublicationRecord };
      }
      return { created: false, record: existing as PublicationRecord };
    },
    async markPublished(id, pageId, pageUrl) {
      const { error } = await auth.serviceClient.from("notion_publications").update({
        status: "published",
        notion_page_id: pageId,
        notion_page_url: pageUrl,
        failure_code: null,
        published_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      }).eq("id", id);
      if (error) throw new Error("Could not record the completed publication.");
    },
    async markFailed(id, failureCode) {
      await auth.serviceClient.from("notion_publications").update({
        status: "failed",
        failure_code: failureCode,
        updated_at: new Date().toISOString(),
      }).eq("id", id);
    },
  };

  try {
    const result = await coordinatePublication({
      reportMode,
      requestId,
      report,
      repository,
      publisher: {
        createPage: (formatted, idempotencyId) => notion.createChildPage({
          destinationType: destination.destination_type,
          destinationObjectId: destination.notion_object_id,
          titleProperty: destination.title_property,
          title: formatted.title,
          blocks: formatted.blocks,
          requestId: idempotencyId,
        }),
        replacePage: (pageId, formatted, idempotencyId) =>
          notion.replacePageMarkdown(pageId, formatted.markdown, idempotencyId),
      },
    });
    return jsonResponse({
      publication: {
        id: result.publicationId,
        status: result.status,
        notion_page_id: result.notionPageId,
        notion_page_url: result.notionPageUrl,
        period_start: report.periodStart,
        period_end: report.periodEnd,
        policy_version: report.policyVersion,
      },
    }, result.status === "in_progress" ? 202 : 200);
  } catch (error) {
    const code = error && typeof error === "object" && "code" in error &&
        typeof (error as { code?: unknown }).code === "string"
      ? (error as { code: string }).code
      : "notion_publish_failed";
    return errorResponse("The report could not be published to Notion.", 502, code);
  }
});
