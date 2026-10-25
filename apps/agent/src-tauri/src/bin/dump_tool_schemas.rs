#[path = "../local_agent/registry.rs"]
#[allow(dead_code)]
mod registry;

fn main() {
    let definitions = registry::enabled_specs()
        .iter()
        .map(registry::model_definition)
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::json!({
            "tools": definitions,
            "routeTool": registry::family_router_definition(),
            "routePrompt": registry::TOOL_FAMILY_PROMPT,
        })
    );
}
