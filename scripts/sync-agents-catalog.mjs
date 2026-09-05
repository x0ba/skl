#!/usr/bin/env node
/**
 * Regenerate `crates/cli/data/agents-catalog.json` from vercel-labs/skills
 * `src/agents.ts` (Supported Agents).
 *
 * Usage:
 *   node scripts/sync-agents-catalog.mjs
 *   AGENTS_TS=./agents.ts node scripts/sync-agents-catalog.mjs
 *   AGENTS_TS_URL=https://raw.githubusercontent.com/vercel-labs/skills/main/src/agents.ts \
 *     node scripts/sync-agents-catalog.mjs
 *
 * Path templates:
 *   {home}        — user home
 *   {xdg_config}  — XDG config home (default `$HOME/.config`)
 *
 * Dynamic globals in agents.ts are baked to their default paths:
 *   openclaw      → {home}/.openclaw/skills  (also probes .clawdbot / .moltbot)
 *   claude-code   → {home}/.claude/skills    (CLAUDE_CONFIG_DIR)
 *   codex         → {home}/.codex/skills     (CODEX_HOME)
 *   mistral-vibe  → {home}/.vibe/skills      (VIBE_HOME)
 *   hermes-agent  → {home}/.hermes/skills    (HERMES_HOME)
 *   autohand-code → {home}/.autohand/skills  (AUTOHAND_HOME)
 *   grok          → {home}/.grok/skills      (GROK_HOME)
 *
 * Agents with `globalSkillsDir: undefined` omit `global_skills_dir`.
 * Engineering may hand-maintain the JSON as long as id + skillsDir +
 * globalSkillsDir stay in parity with agents.ts.
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "crates/cli/data/agents-catalog.json");
const DEFAULT_URL =
  "https://raw.githubusercontent.com/vercel-labs/skills/main/src/agents.ts";

const HOME_VARS = {
  claudeHome: "{home}/.claude",
  codexHome: "{home}/.codex",
  vibeHome: "{home}/.vibe",
  hermesHome: "{home}/.hermes",
  autohandHome: "{home}/.autohand",
  grokHome: "{home}/.grok",
};

function joinToTemplate(expr) {
  const trimmed = expr.replace(/\s+/g, " ").trim();
  if (trimmed === "undefined") return null;
  if (trimmed.startsWith("getOpenClawGlobalSkillsDir")) {
    return "{home}/.openclaw/skills";
  }

  const joinMatch = trimmed.match(/^join\((.*)\)$/);
  if (!joinMatch) {
    throw new Error(`unsupported globalSkillsDir expr: ${expr}`);
  }

  const parts = splitArgs(joinMatch[1]);
  if (parts.length < 2) {
    throw new Error(`join() needs a root + segment: ${expr}`);
  }

  const root = parts[0];
  const segs = parts.slice(1).map(unquote);
  let prefix;
  if (root === "home") {
    prefix = "{home}";
  } else if (root === "configHome") {
    prefix = "{xdg_config}";
  } else if (HOME_VARS[root]) {
    prefix = HOME_VARS[root];
  } else {
    throw new Error(`unknown join root \`${root}\` in ${expr}`);
  }
  return [prefix, ...segs].join("/");
}

function splitArgs(inner) {
  const out = [];
  let cur = "";
  let quote = null;
  for (const ch of inner) {
    if (quote) {
      cur += ch;
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"') {
      quote = ch;
      cur += ch;
      continue;
    }
    if (ch === ",") {
      out.push(cur.trim());
      cur = "";
      continue;
    }
    cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

function extractExpr(block, key) {
  const idx = block.indexOf(`${key}:`);
  if (idx < 0) return null;
  let i = idx + key.length + 1;
  while (i < block.length && /\s/.test(block[i])) i++;
  let depth = 0;
  let quote = null;
  let start = i;
  for (; i < block.length; i++) {
    const ch = block[i];
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === '"') {
      quote = ch;
      continue;
    }
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if ((ch === "," || ch === "\n") && depth <= 0) {
      return block.slice(start, i).trim();
    }
  }
  return block.slice(start).trim();
}

/** Project dests must stay inside the selected project (no abs / `..` / drive). */
function isSafeProjectRel(rel) {
  if (!rel || rel.length > 512) return false;
  if (
    rel.startsWith("/") ||
    rel.startsWith("\\") ||
    rel.includes("\\") ||
    rel.includes(":")
  ) {
    return false;
  }
  return rel.split("/").every((part) => part && part !== "." && part !== "..");
}

function unquote(value) {
  const v = value.trim();
  const m = v.match(/^['"](.*)['"]$/);
  if (!m) {
    throw new Error(`expected string literal, got ${value}`);
  }
  return m[1];
}

function parseAgents(src) {
  const record = src.match(
    /export const agents: Record<AgentType, AgentConfig> = \{([\s\S]*)\n\};/
  );
  if (!record) {
    throw new Error("could not find `export const agents` record");
  }

  const body = record[1];
  const entries = [];
  const blockRe =
    /\n\s*['"]?([A-Za-z0-9_-]+)['"]?:\s*\{([\s\S]*?)\n\s*\},/g;
  let match;
  while ((match = blockRe.exec(body))) {
    const id = match[1];
    const block = match[2];
    const display =
      block.match(/displayName:\s*['"]([^'"]+)['"]/)?.[1] ?? null;
    const skillsDir = block.match(/skillsDir:\s*['"]([^'"]+)['"]/)?.[1];
    if (!skillsDir) {
      throw new Error(`missing skillsDir for ${id}`);
    }
    if (!isSafeProjectRel(skillsDir)) {
      throw new Error(
        `unsafe project skillsDir for ${id}: ${skillsDir} (must be a project-relative path)`
      );
    }
    const globalRaw = extractExpr(block, "globalSkillsDir");
    if (!globalRaw) {
      throw new Error(`missing globalSkillsDir for ${id}`);
    }
    const global = joinToTemplate(globalRaw);
    const entry = {
      id,
      name: display ?? id,
      project_skills_dir: skillsDir,
    };
    if (global) {
      entry.global_skills_dir = global;
    }
    entries.push(entry);
  }
  return entries;
}

async function loadSource() {
  if (process.env.AGENTS_TS) {
    return readFileSync(process.env.AGENTS_TS, "utf8");
  }
  const url = process.env.AGENTS_TS_URL || DEFAULT_URL;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`fetch ${url} failed: ${res.status} ${res.statusText}`);
  }
  return await res.text();
}

const src = await loadSource();
const agents = parseAgents(src);
if (agents.length < 70) {
  throw new Error(`expected ~77 agents, parsed ${agents.length}`);
}

mkdirSync(dirname(OUT), { recursive: true });
const payload = {
  source: "https://github.com/vercel-labs/skills/blob/main/src/agents.ts",
  generated_by: "scripts/sync-agents-catalog.mjs",
  notes: [
    "ids + project_skills_dir + global_skills_dir must stay in parity with agents.ts skillsDir / globalSkillsDir.",
    "openclaw global is baked to {home}/.openclaw/skills (agents.ts also probes ~/.clawdbot and ~/.moltbot).",
    "Env-overridable homes (CLAUDE_CONFIG_DIR, CODEX_HOME, …) use their default ~/.name paths.",
  ],
  agents,
};
writeFileSync(OUT, `${JSON.stringify(payload, null, 2)}\n`);
console.log(`wrote ${agents.length} agents → ${OUT}`);
