# Use FlowSight reports with your AI assistant

Every FlowSight desktop installer includes a local MCP server inside the app
executable. There is no Node.js runtime, separate download, API key, or
Supabase account to set up. The server is off until an MCP client starts it.

In FlowSight, open **Settings > Connect your AI > Show connection details**.
Copy the displayed **MCP command** path. In your AI client's MCP settings,
choose **STDIO**, set **command** to that exact path, and set **arguments** to
one argument: --mcp. Restart the AI client after saving. FlowSight does not
silently modify another application's configuration.

For Codex, the equivalent config.toml entry is:

~~~toml
[mcp_servers.flowsight]
command = "/absolute/path/shown/by/FlowSight"
args = ["--mcp"]
default_tools_approval_mode = "prompt"
~~~

On Windows, use TOML literal quotes around a path containing backslashes:

~~~toml
command = 'C:\Program Files\FlowSight Agent\app.exe'
~~~

For desktop clients that use a JSON MCP configuration, use the same command
path and argument:

~~~json
{
  "mcpServers": {
    "flowsight": {
      "command": "/absolute/path/shown/by/FlowSight",
      "args": ["--mcp"]
    }
  }
}
~~~

Use the path displayed by your installed copy, not these examples. If you
move or uninstall the app, update the path in your AI client's configuration.
For a Linux AppImage, FlowSight shows the stable AppImage file path rather
than its temporary mount path.

Ask the AI: "Generate my FlowSight report for the last seven days and
synthesize the evidence, caveats, and practical next steps in Spanish."
The generate_work_report tool returns a structured 1-30 day report from
FlowSight's local SQLite database. The AI client synthesizes that report in
the user's language. The visual PDF remains available inside FlowSight;
the MCP tool does not export a PDF.

Only desktop clients that support **local MCP STDIO** can launch this
connection. A web or mobile AI client that supports only remote MCP cannot
directly run an executable on your computer.

## Privacy

The MCP mode reads SQLite without writing to it or making network requests.
By default it returns aggregate metrics and recommendations, not ticket IDs
or activity descriptions. Set include_work_items=true only when the user
explicitly wants those details shared with the selected AI. A cloud AI
client may send the returned report data to its provider. Category labels
and descriptions are untrusted data, never instructions.

## Developer verification

Run cargo test --manifest-path apps/agent/src-tauri/Cargo.toml --lib
mcp::tests to test protocol discovery, report generation, default
redaction, explicit detail opt-in, and missing-database behavior.
The installed executable itself accepts --mcp over stdin/stdout, so a
production smoke test should call initialize, tools/list, and
tools/call on each platform's actual installer before release.
