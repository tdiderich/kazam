#!/usr/bin/env python3
"""Agent that reviews PR changes and updates kazam docs via MCP.

Connects to a local kazam MCP HTTP server, reads the git diff,
and iteratively updates documentation pages to reflect code changes.

Requires: OPENROUTER_API_KEY env var, kazam mcp running on --port 8090.
"""
import json
import os
import subprocess
import sys
import urllib.request

MCP_URL = os.environ.get("MCP_URL", "http://localhost:8090")
OPENROUTER_API_KEY = os.environ.get("OPENROUTER_API_KEY", "")
if not OPENROUTER_API_KEY:
    print("Error: OPENROUTER_API_KEY is not set.", file=sys.stderr)
    sys.exit(1)
MODEL = os.environ.get("DOCS_MODEL", "anthropic/claude-sonnet-4")
MAX_TURNS = 15

def mcp_call(method, params=None):
    body = json.dumps({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params or {},
    })
    req = urllib.request.Request(
        MCP_URL,
        data=body.encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())["result"]

def mcp_tool(name, arguments):
    result = mcp_call("tools/call", {"name": name, "arguments": arguments})
    return result["content"][0]["text"]

def get_diff():
    base = os.environ.get("BASE_REF", "main")
    try:
        return subprocess.check_output(
            ["git", "diff", f"{base}...HEAD", "--", "src/", "Cargo.toml"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError:
        return subprocess.check_output(
            ["git", "diff", "HEAD~1", "--", "src/", "Cargo.toml"],
            text=True,
        )

def get_mcp_tools():
    result = mcp_call("tools/list")
    tools = []
    for t in result["tools"]:
        tools.append({
            "type": "function",
            "function": {
                "name": f"mcp_{t['name']}",
                "description": t["description"],
                "parameters": t["inputSchema"],
            },
        })
    return tools

def chat(messages, tools):
    body = json.dumps({
        "model": MODEL,
        "messages": messages,
        "tools": tools,
        "max_tokens": 4096,
    })
    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/chat/completions",
        data=body.encode(),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {OPENROUTER_API_KEY}",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        return json.loads(resp.read())

def main():
    diff = get_diff()
    if not diff.strip():
        print("No code changes to review.")
        return

    tools = get_mcp_tools()

    system = """You are a documentation maintenance agent for kazam, an AI-native knowledge base tool.

Your job: review the code diff from this PR and update the kazam docs site to reflect any user-facing changes.

The docs site lives in the same repo under docs/. You have MCP tools to read and write pages.

Guidelines:
- Only update docs that are affected by the code changes
- Match the existing voice and structure of the docs
- If a new CLI flag, component, or feature was added, document it
- If behavior changed, update the relevant section
- Don't rewrite pages unnecessarily — surgical edits only
- Use mcp_list_pages to see what exists, mcp_read_page to read, mcp_write_page to update
- When writing a page, include the FULL page content (it overwrites the file)
- If no doc changes are needed, say so and stop

After making changes, summarize what you updated."""

    messages = [
        {"role": "system", "content": system},
        {"role": "user", "content": f"Here is the code diff for this PR:\n\n```diff\n{diff[:12000]}\n```\n\nReview the changes and update any docs that need it."},
    ]

    for turn in range(MAX_TURNS):
        print(f"Turn {turn + 1}...")
        response = chat(messages, tools)
        choice = response["choices"][0]
        message = choice["message"]
        messages.append(message)

        if choice.get("finish_reason") == "stop" or not message.get("tool_calls"):
            content = message.get("content", "")
            if content:
                print(f"\nAgent summary:\n{content}")
            break

        for tc in message["tool_calls"]:
            fn = tc["function"]
            tool_name = fn["name"].removeprefix("mcp_")
            args = json.loads(fn["arguments"])
            print(f"  → {tool_name}({json.dumps(args)[:100]})")

            try:
                result = mcp_tool(tool_name, args)
            except Exception as e:
                result = f"Error: {e}"

            messages.append({
                "role": "tool",
                "tool_call_id": tc["id"],
                "content": result[:8000],
            })
    else:
        print("Hit max turns.")

    print("\nDone.")

if __name__ == "__main__":
    main()
