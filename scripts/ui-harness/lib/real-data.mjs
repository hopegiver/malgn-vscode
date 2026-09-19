// 로컬 파일시스템에서 실제 데이터를 읽어 하네스 픽스처의 현실성을 높인다.
// 파일이 없거나 읽기에 실패하면 그 사실을 콘솔에 기록하고 현실적인 더미로
// 대체한다(지시서 요구사항 — "파일이 없으면 현실적인 더미로 대체하되 그
// 사실을 기록하라").
import { readFile, readdir, stat } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';

const HOME = homedir();
export const notes = [];

function note(msg) {
  notes.push(msg);
}

async function readJson(p, { quiet = false } = {}) {
  try {
    const raw = await readFile(p, 'utf-8');
    return JSON.parse(raw);
  } catch (err) {
    // ENOENT + quiet: 있을 수도 없을 수도 있는 파일(예: 프로젝트별
    // .claude/autonomy.json)을 순회 조회할 때 "없음"은 정상 상태다 — 매번
    // 로그를 남기면 30개 이상 프로젝트를 훑는 동안 진짜 이상 신호(JSON 파싱
    // 깨짐 등)가 노이즈에 묻힌다.
    if (!(quiet && err.code === 'ENOENT')) note(`읽기 실패(더미로 대체): ${p} — ${err.message}`);
    return null;
  }
}

// ---------------- 설치된 플러그인 (~/.claude/plugins/installed_plugins.json) ----------------
async function scanEntryDir(dir, kind) {
  try {
    const names = await readdir(dir, { withFileTypes: true });
    const items = [];
    for (const n of names) {
      if (kind === 'knowledge') {
        // knowledge는 파일/디렉터리가 섞여 있을 수 있다 — 이름만 항목화한다.
        if (n.name.startsWith('.')) continue;
        items.push({ id: n.name, name: n.name.replace(/\.md$/, ''), description: '' });
        continue;
      }
      if (kind === 'agents' && n.isFile() && n.name.endsWith('.md')) {
        const desc = await extractFrontmatterDescription(path.join(dir, n.name));
        items.push({ id: n.name.replace(/\.md$/, ''), name: n.name.replace(/\.md$/, ''), description: desc });
      } else if (kind === 'skills' && n.isDirectory()) {
        const skillMd = path.join(dir, n.name, 'SKILL.md');
        const desc = await extractFrontmatterDescription(skillMd);
        items.push({ id: n.name, name: n.name, description: desc });
      }
    }
    return items;
  } catch {
    return [];
  }
}

async function extractFrontmatterDescription(mdPath) {
  try {
    const raw = await readFile(mdPath, 'utf-8');
    const m = raw.match(/^---\n([\s\S]*?)\n---/);
    if (!m) return '';
    const descMatch = m[1].match(/^description:\s*(.+)$/m);
    return descMatch ? descMatch[1].trim().slice(0, 140) : '';
  } catch {
    return '';
  }
}

export async function loadInstalledPlugins() {
  const p = path.join(HOME, '.claude', 'plugins', 'installed_plugins.json');
  const data = await readJson(p);
  if (!data?.plugins) {
    note('installed_plugins.json 없음 — 더미 플러그인 1개로 대체');
    return [
      {
        id: 'dummy-plugin@dummy-marketplace',
        name: 'dummy-plugin',
        displayName: '더미 플러그인',
        version: '0.0.1',
        description: '실 파일을 찾지 못해 생성한 더미 픽스처',
        installPath: '/nonexistent',
        agents: [],
        skills: [],
        knowledge: [],
      },
    ];
  }
  const result = [];
  for (const [pluginId, installs] of Object.entries(data.plugins)) {
    const userInstall = installs.find((i) => i.scope === 'user');
    if (!userInstall) continue; // catalogApi.ts 주석: scope "user"만 조회 대상
    const [name] = pluginId.split('@');
    const agents = await scanEntryDir(path.join(userInstall.installPath, 'agents'), 'agents');
    const skills = await scanEntryDir(path.join(userInstall.installPath, 'skills'), 'skills');
    const knowledge = await scanEntryDir(path.join(userInstall.installPath, 'knowledge'), 'knowledge');
    result.push({
      id: pluginId,
      name,
      displayName: null,
      version: userInstall.version,
      description: `${name} 플러그인(실제 로컬 설치 데이터에서 읽음)`,
      installPath: userInstall.installPath,
      agents,
      skills,
      knowledge,
    });
  }
  if (result.length === 0) note('installed_plugins.json에 user scope 항목 없음 — 빈 배열 반환(정상 "빈 상태" 케이스)');
  return result;
}

// ---------------- 마켓플레이스 (~/.claude/plugins/known_marketplaces.json) ----------------
export async function loadKnownMarketplaces() {
  const p = path.join(HOME, '.claude', 'plugins', 'known_marketplaces.json');
  const data = await readJson(p);
  if (!data) {
    note('known_marketplaces.json 없음 — 더미 마켓플레이스 1개로 대체');
    return [{ id: 'dummy-marketplace', repo: 'example/dummy', lastUpdated: new Date().toISOString() }];
  }
  return Object.entries(data).map(([id, v]) => ({
    id,
    repo: v?.source?.repo ?? null,
    lastUpdated: v?.lastUpdated ?? null,
  }));
}

// ---------------- 전역 카탈로그 (~/.claude/agents, ~/.claude/skills) ----------------
export async function loadGlobalCatalog() {
  const agents = await scanEntryDir(path.join(HOME, '.claude', 'agents'), 'agents');
  const skills = await scanEntryDir(path.join(HOME, '.claude', 'skills'), 'skills');
  if (agents.length === 0 && skills.length === 0) note('~/.claude/agents, ~/.claude/skills 둘 다 비어있음(정상 "빈 상태")');
  return { agents: agents.map((a) => ({ ...a, path: '', status: 'valid' })), skills: skills.map((s) => ({ ...s, path: '', status: 'valid' })) };
}

// ---------------- 워크스페이스 프로젝트 (~/workspace/*/CLAUDE.md) ----------------
export async function loadWorkspaceProjects() {
  const wsDir = path.join(HOME, 'workspace');
  let entries;
  try {
    entries = await readdir(wsDir, { withFileTypes: true });
  } catch (err) {
    note(`~/workspace 읽기 실패(더미로 대체): ${err.message}`);
    return [
      { name: 'dummy-project', path: '/nonexistent/dummy-project', hasStatus: false, archiveStatus: 'unknown', sections: null, updatedAt: Date.now() },
    ];
  }
  const projects = [];
  for (const e of entries) {
    if (!e.isDirectory()) continue;
    const dir = path.join(wsDir, e.name);
    const claudeMd = path.join(dir, 'CLAUDE.md');
    try {
      const st = await stat(claudeMd);
      const statusMd = path.join(dir, 'STATUS.md');
      let hasStatus = false;
      let updatedAt = st.mtimeMs;
      try {
        const sst = await stat(statusMd);
        hasStatus = true;
        updatedAt = Math.max(updatedAt, sst.mtimeMs);
      } catch {
        /* STATUS.md 없음 — hasStatus false 유지 */
      }
      projects.push({ name: e.name, path: dir, hasStatus, archiveStatus: 'unknown', sections: null, updatedAt: Math.round(updatedAt) });
    } catch {
      /* CLAUDE.md 없는 폴더는 프로젝트로 인식되지 않는다(Rust와 동일 규칙) */
    }
  }
  if (projects.length === 0) note('~/workspace 아래 CLAUDE.md 있는 폴더 없음 — 빈 배열(정상 "빈 상태")');
  projects.sort((a, b) => b.updatedAt - a.updatedAt);
  return projects;
}

// ---------------- 전역 설정 (~/.claude/malgn-agent.json) ----------------
export async function loadMalgnAgentConfig() {
  const p = path.join(HOME, '.claude', 'malgn-agent.json');
  const data = await readJson(p);
  const limits = { minInterval: 1, maxInterval: 1440, minTimeout: 1, maxTimeout: 480, maxConcurrency: 10, startupGraceMinutes: 3, missedRunGraceMinutes: 10 };
  if (!data) {
    note('~/.claude/malgn-agent.json 없음 — 더미 설정으로 대체');
    return {
      ok: true,
      error: null,
      configPath: p,
      fileExists: false,
      workspaces: [path.join(HOME, 'workspace')],
      warnings: [],
      autonomy: { concurrency: 3, defaultTimeout: 60 },
      logs: { retentionDays: 30 },
      limits,
    };
  }
  return {
    ok: true,
    error: null,
    configPath: p,
    fileExists: true,
    workspaces: data.workspaces ?? [],
    warnings: [],
    autonomy: data.autonomy ?? { concurrency: 3, defaultTimeout: 60 },
    logs: data.logs ?? { retentionDays: 30 },
    limits,
  };
}

// ---------------- 자율업무 (프로젝트별 .claude/autonomy.json 실물) ----------------
// list_workspace_projects가 스캔하는 것과 같은 폴더들을 훑어 실제
// .claude/autonomy.json이 있는 프로젝트만 그룹으로 합친다 — Rust의
// autonomy_list()가 하는 일과 동일한 절차를 프론트 스크립트로 재현한 것이다.
export async function loadAutonomyGroups(projects) {
  const groups = [];
  for (const proj of projects) {
    const p = path.join(proj.path, '.claude', 'autonomy.json');
    const data = await readJson(p, { quiet: true });
    if (!data?.tasks || data.tasks.length === 0) continue;
    groups.push({
      projectPath: proj.path,
      projectName: proj.name,
      tasks: data.tasks.map((t) => ({
        id: t.id,
        name: t.name,
        prompt: t.prompt,
        subagent: t.subagent ?? null,
        interval: t.interval ?? 60,
        scheduleMode: t.scheduleMode ?? 'interval',
        atTime: t.atTime ?? null,
        days: t.days ?? [],
        hourlyMinute: t.hourlyMinute ?? null,
        cron: t.cron ?? null,
        enabled: t.enabled ?? false,
        timeout: t.timeout ?? null,
      })),
    });
  }
  if (groups.length === 0) note('실제 프로젝트 .claude/autonomy.json에 등록된 task 없음 — 빈 배열(정상 "빈 상태")');
  return groups;
}

// ---------------- 세션 (~/.claude/projects/**/*.jsonl 첫 user 메시지로 제목 유도) ----------------
// Rust의 정본 데이터 소스(registry+jsonl)를 완전히 재현하지는 못한다 — 제목만
// 실제 jsonl에서 최선 노력으로 추출하고, 나머지 필드(running/시각 등)는
// 합성값이다(그 사실을 note에 남긴다).
async function extractFirstUserText(jsonlPath) {
  try {
    const raw = await readFile(jsonlPath, 'utf-8');
    const lines = raw.split('\n').slice(0, 300);
    for (const line of lines) {
      if (!line.trim()) continue;
      let obj;
      try {
        obj = JSON.parse(line);
      } catch {
        continue;
      }
      if (obj.type === 'user' && obj.message?.content) {
        const c = obj.message.content;
        if (typeof c === 'string' && c.trim()) return c.trim().slice(0, 80);
        if (Array.isArray(c)) {
          const textPart = c.find((p) => p.type === 'text' && p.text?.trim());
          if (textPart) return textPart.text.trim().slice(0, 80);
        }
      }
    }
  } catch {
    /* 무시 — 폴백 제목 사용 */
  }
  return null;
}

export async function loadSessionSamples(maxFiles = 6) {
  const projectsDir = path.join(HOME, '.claude', 'projects');
  let projDirs;
  try {
    projDirs = await readdir(projectsDir, { withFileTypes: true });
  } catch (err) {
    note(`~/.claude/projects 읽기 실패(더미로 대체): ${err.message}`);
    return [];
  }
  const samples = [];
  outer: for (const pd of projDirs) {
    if (!pd.isDirectory()) continue;
    const full = path.join(projectsDir, pd.name);
    let files;
    try {
      files = (await readdir(full)).filter((f) => f.endsWith('.jsonl'));
    } catch {
      continue;
    }
    for (const f of files) {
      const jsonlPath = path.join(full, f);
      const st = await stat(jsonlPath).catch(() => null);
      if (!st) continue;
      const title = await extractFirstUserText(jsonlPath);
      samples.push({
        sessionId: f.replace(/\.jsonl$/, ''),
        cwd: pd.name.startsWith('-') ? pd.name.slice(1).replace(/-/g, '/') : pd.name,
        title,
        updatedAt: Math.round(st.mtimeMs),
        startedAt: Math.round(st.mtimeMs) - 1000 * 60 * 5,
      });
      if (samples.length >= maxFiles) break outer;
    }
  }
  if (samples.length === 0) note('~/.claude/projects 아래 jsonl 없음 — 세션 샘플 0개(빈 상태로 처리)');
  else note(`세션 제목은 실제 jsonl 첫 user 메시지에서 추출(성공 ${samples.filter((s) => s.title).length}/${samples.length}건), running/버전 등은 합성값`);
  return samples;
}
