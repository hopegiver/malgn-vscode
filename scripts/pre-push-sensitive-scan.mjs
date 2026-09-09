#!/usr/bin/env node
// pre-push 훅 정본 로직 — .githooks/pre-push가 이 스크립트를 실행한다.
//
// 이 저장소는 public이다. 이전(Electron 시절) 버전은 compat/sensitive-classes.json +
// scripts/lib/sensitiveScan.mjs라는 별도 인프라를 뒀지만, 2026-09-08 Tauri 전환으로
// 그 인프라 전체가 삭제됐다. 이 스크립트는 그 자리를 대신하는 최소 버전이다 — 알려진
// 시크릿 패턴(클라우드 API 키, GitHub/Slack 토큰, Google OAuth client secret, 개인키
// 블록 등)을 push될 커밋 범위의 diff에서 찾는다. 새 패턴이 필요해지면 PATTERNS 배열에
// 추가한다.

import { execFileSync } from "node:child_process";

const PATTERNS = [
  { name: "AWS Access Key", re: /AKIA[0-9A-Z]{16}/g },
  { name: "GitHub token", re: /gh[pousr]_[A-Za-z0-9]{36,}/g },
  { name: "Slack token", re: /xox[baprs]-[A-Za-z0-9-]{10,}/g },
  { name: "Google OAuth client secret", re: /GOCSPX-[A-Za-z0-9_-]{20,}/g },
  { name: "Generic API key literal", re: /sk-[A-Za-z0-9]{20,}/g },
  { name: "PEM private key block", re: /-----BEGIN (RSA |OPENSSH |EC |DSA )?PRIVATE KEY-----/g },
];

function getPushedDiff() {
  // 로컬 main이 origin/main보다 앞선 커밋들의 diff만 본다. origin에 아직 없으면(신규
  // 브랜치 등) 최근 커밋 하나로 폴백한다 — 매번 전체 히스토리를 훑을 필요는 없다.
  try {
    execFileSync("git", ["rev-parse", "--verify", "origin/main"], { stdio: "ignore" });
    return execFileSync("git", ["diff", "origin/main...HEAD", "--", ".", ":(exclude)*.lock", ":(exclude)pnpm-lock.yaml"], {
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 64,
    });
  } catch {
    return execFileSync("git", ["show", "HEAD", "--", ".", ":(exclude)*.lock", ":(exclude)pnpm-lock.yaml"], {
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 64,
    });
  }
}

function main() {
  const diff = getPushedDiff();
  // 추가된 줄(+로 시작, 파일 헤더 +++ 제외)만 검사한다 — 삭제되는 줄에 있던 값은
  // push 이후 저장소에 안 남으니 대상이 아니다.
  const addedLines = diff
    .split("\n")
    .filter((line) => line.startsWith("+") && !line.startsWith("+++"));

  // 멀티라인 문자열 리터럴(예: PEM 블록)은 시작줄 자체에 트레일링 주석을 못 달 수
  // 있으니, "직전 2줄 이내"에 허용 표시가 있으면 그 매치는 건너뛴다.
  const ALLOWLIST_MARKER = "pragma: allowlist-secret";
  const findings = [];
  for (let i = 0; i < addedLines.length; i++) {
    const line = addedLines[i];
    const nearby = addedLines.slice(Math.max(0, i - 2), i + 1).join("\n");
    if (nearby.includes(ALLOWLIST_MARKER)) continue;
    for (const { name, re } of PATTERNS) {
      re.lastIndex = 0;
      if (re.test(line)) {
        findings.push({ name, line: line.slice(0, 200) });
      }
    }
  }

  if (findings.length > 0) {
    console.error("\n🚫 pre-push 차단 — push될 변경분에서 시크릿으로 보이는 패턴을 발견했다:\n");
    for (const f of findings) {
      console.error(`  [${f.name}] ${f.line}`);
    }
    console.error(
      "\n이 저장소는 public이다. 실수라면 커밋을 정리한 뒤 다시 push하고, 오탐이면 " +
        "scripts/pre-push-sensitive-scan.mjs의 PATTERNS를 조정하라.\n",
    );
    process.exit(1);
  }

  process.exit(0);
}

main();
