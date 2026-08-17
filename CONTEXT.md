# FlowSight.AI — contexto de módulos

Documento de arquitectura del **agente de escritorio** (Tauri) y límites funcionales relevantes para evitar deuda técnica al añadir features. No sustituye al código fuente; enlaza responsabilidades con rutas concretas.

## Monorepo

- Raíz: `pnpm` workspace; la app principal está en `apps/agent/` (Vite + Tauri 2).
- Backend Rust: `apps/agent/src-tauri/`.
- UI renderer: `apps/agent/src/renderer/` (`index.html` + JS vanilla; invoca comandos Tauri).
- **Catálogo de apps (research)**: crate `tools/popular-apps-catalog/` genera `data/popular-apps-catalog.txt` con `cargo run --manifest-path tools/popular-apps-catalog/Cargo.toml` (fuentes en `tools/popular-apps-catalog/assets/`, sin Python).

## Capas del backend Rust (`apps/agent/src-tauri/src/`)

| Módulo | Rol |
|--------|-----|
| `lib.rs` | Arranque Tauri, registro de comandos `invoke_handler`, plugin de logs a disco/consola. |
| `main.rs` | Punto de entrada del binario (delega en `lib::run`). |
| `agent.rs` | Estado del agente, SQLite local (`reports`, `config`), captura de pantalla, **llamada HTTP** al servidor local tipo OpenAI para visión, guardado de actividades, arranque de hilos de sync. |
| `agent_pure.rs` | Parseo determinista del texto devuelto por el modelo: categoría + descripción estructurada. **Sin efectos secundarios**; cubierto por tests. |
| `sync.rs` | Sincronización periódica con Supabase: lectura de informes no sincronizados, **resumen por LLM** local, subida `work_sessions` / `activity_reports`, sesión JWT y refresh. |
| `sync_pure.rs` | Helpers puros: SQL de batch pendiente, JWT `exp`, truncado/clamp del texto enviado al modelo de resumen. |
| `sync_env.rs` | URLs y claves públicas de Supabase (entorno / build). |
| `entitlements.rs` | Licencias/tiers: caché local, RPC `get_user_entitlements`, gates de sync/integraciones/cloud AI. |
| `auth.rs` | OAuth / sesión local expuesta al frontend (requiere licencia para integraciones). |
| `jira.rs` | Integración Jira (OAuth, tareas). |
| `linear.rs` | Integración Linear. |
| `oauth_env.rs` | Config OAuth por proveedor. |
| `context.rs` | Contexto de ventana activa (`active_win_pos_rs`) y utilidades Git opcionales (`get_git_context`). |
| `paths.rs` | Rutas de datos de la app (DB, logs, temporales). |
| `vision_model.rs` | IDs de modelo y nombres de archivos GGUF/MMPROJ embebidos. |
| `llama_port.rs` | Puerto y URL del `llama-server` gestionado. |
| `llama_windows_job.rs` | Agrupación de proceso en Windows para limpieza al cerrar. |
| `screenshot_disk.rs` | Escritura opcional de captura cifrada (DPAPI) para depuración. |

## Flujo principal de captura y clasificación

1. **Captura**: `capture_screen` en `agent.rs` → redimensionado (~960×540) → PNG base64.
2. **Visión**: `analyze_image_with_vision` → POST a `/v1/chat/completions` local, mensaje multimodal (texto + imagen).
3. **Post-proceso**: `parse_analysis` en `agent_pure.rs` → `(description, category)` que la UI persiste vía `save_activity`.

La categoría **no es un booleano “es código”**: es una etiqueta entre muchas (`Coding`, `Admin`, etc.). La heurística de respaldo y el parseo de `CATEGORY:` deben alinearse con el prompt de visión para minimizar falsos `Coding`.

## Flujo de sync y resumen batch

1. `perform_sync` lee filas `synced = 0` **ordenadas de más antigua a más nueva** hasta el límite de batch.
2. Construye un texto tipo lista `- [Categoría] descripción` por fila.
3. Aplica límites de tamaño (por línea y total) y llama a `summarize_with_vision_model` para un párrafo único.
4. Sube el resumen y marca filas como sincronizadas.

Variables útiles: `FLOWSIGHT_SUMMARY_MAX_CHARS`, `FLOWSIGHT_SUMMARY_MAX_LINE_CHARS`, `FLOWSIGHT_SYNC_BATCH_LIMIT` (ver comentarios en `sync.rs`).

## Frontend (`apps/agent/src/renderer/`)

- Arranque **free-first**: la app principal (Today/Summary) funciona sin login; captura y LLM local siempre en localhost.
- Panel **Activate cloud features** (Profile o overlay): login Supabase + licencia Individual o Team.
- Comunicación con Rust solo vía `invoke(...)` con los comandos declarados en `lib.rs`.

## Modelo de tiers (Free / Individual / Team)

| Tier | Internet | Features |
|------|----------|----------|
| **Free** | Solo localhost (`llama-server`) | Captura, SQLite, Summary local |
| **Individual** | Supabase + integraciones | Sync cloud cada 10 min, Work Insights (local + OpenRouter MiMo v2.5 Pro), Jira/Linear/Google |
| **Team** | Supabase + integraciones | Igual que Individual pero varios miembros; PM gestiona invitaciones fuera del agente |

- Esquema y RLS: `supabase/migrations/20260521000000_tiers_and_entitlements.sql`
- RPC principal: `get_user_entitlements()` — fuente de verdad para agente y políticas RLS.
- Tablas nuevas: `cloud_insights`; `teams.is_personal` para cuentas Individual.
- Licencias existentes: `public.licenses` + RPC `claim_license` (código FS-XXXX-XXXX); no hay tabla `subscriptions`.
- Edge Function on-demand: `supabase/functions/generate-insights/` → escribe en `cloud_insights`.

## Dependencias conceptuales

- **SQLite local** (`agent.rs`): fuente de verdad de capturas hasta sync; esquema evolutivo con `ALTER` tolerantes a fallo.
- **Servidor LLM local**: requerido para visión y para el resumen de sync; puerto coordinado por `llama_port.rs`.
- **Supabase**: destino cloud para sesiones de trabajo agregadas y licencias; no para filas crudas de `reports` completas. Sync/integraciones bloqueados sin licencia activa (`entitlements.rs` + RLS).

## Al modificar comportamiento de IA

- Cualquier cambio al **template** del prompt en `analyze_image_with_vision` debería revisarse junto con **`agent_pure.rs`** (lista de categorías válidas y heurísticas).
- Tras subir `max_tokens` o alargar prompts, revisar `docs/` existentes sobre presupuesto de contexto (si aplica) y el truncado en `sync_pure.rs`.
