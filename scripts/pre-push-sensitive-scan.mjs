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
  // fine-grained PAT는 gh[pousr]_ 접두사가 아니라 github_pat_ 접두사를 쓴다 — 별도 패턴 필요.
  { name: "GitHub fine-grained PAT", re: /github_pat_[A-Za-z0-9_]{50,}/g },
  { name: "Slack token", re: /xox[baprs]-[A-Za-z0-9-]{10,}/g },
  { name: "Google OAuth client secret", re: /GOCSPX-[A-Za-z0-9_-]{20,}/g },
  { name: "Google API key", re: /AIza[0-9A-Za-z_-]{35}/g },
  // sk-ant- 뒤에는 하이픈이 섞여 나오므로 아래 "Generic API key literal"(하이픈 불허)로는
  // 안 잡힌다 — Anthropic 키 전용 패턴을 별도로 둔다.
  { name: "Anthropic API key", re: /sk-ant-[A-Za-z0-9_-]{20,}/g },
  { name: "Generic API key literal", re: /sk-[A-Za-z0-9]{20,}/g },
  { name: "PEM private key block", re: /-----BEGIN (RSA |OPENSSH |EC |DSA )?PRIVATE KEY-----/g },
  // minisign/rsign 개인키 파일 첫 줄 형식 탐지(업데이터 서명키가 이 형식이라 가장 중요).
  // pragma: allowlist-secret — 이 정규식 리터럴이 자기 자신과 자기참조 매치되는 걸 막는 마커(실제 키 아님, 지우면 이 파일 push마다 자기차단됨).
  { name: "minisign/rsign secret key", re: /untrusted comment:.*secret key/gi },
  // PKCS#12(.pfx/.p12)를 base64로 인코딩해 커밋하는 사고 패턴. DER 인코딩된 X.509/PKCS
  // 구조는 거의 항상 base64로 "MII"로 시작한다. 200자 이상으로 문턱을 높게 잡아 우연한
  // 짧은 base64 조각과 구분한다 — pnpm-lock.yaml/Cargo.lock은 getPushedDiff()에서 이미
  // diff 대상에서 제외되므로 그쪽의 긴 해시 문자열과는 애초에 부딪히지 않는다.
  { name: "PKCS#12/X.509 base64 blob", re: /MII[A-Za-z0-9+/]{200,}/g },
];

// git add -f로 .gitignore를 뚫고 시크릿 파일 자체를 추가하는 경우 — 내용 패턴과 무관하게
// 파일명만으로 차단한다. "diff --git a/X b/Y" 헤더는 텍스트/바이너리 diff 모두에 항상
// 나오므로(바이너리는 "+++ b/..." 헤더가 없고 "Binary files ... differ"로 대체된다),
// 그쪽이 아니라 이 헤더에서 새 경로(Y)를 뽑는다.
const SECRET_FILENAME_HEADER = /^diff --git a\/.+ b\/(.+)$/;
const SECRET_FILENAME_RE = /(^|\/)(\.env(\..+)?|\.dev\.vars|[^/]+\.(pfx|p12|key|pem|jks))$/;
// .env.example/.env.sample/.env.template은 값이 없는 문서용 템플릿이라는 흔한 관례다 —
// 이것까지 막으면 정상적인 온보딩 문서 작업이 매번 걸린다.
const SECRET_FILENAME_EXEMPT_RE = /\.env\.(example|sample|template)$/;

function findSecretFilenames(allLines) {
  const findings = [];
  for (let i = 0; i < allLines.length; i++) {
    const m = allLines[i].match(SECRET_FILENAME_HEADER);
    if (!m) continue;
    const path = m[1];
    if (SECRET_FILENAME_EXEMPT_RE.test(path)) continue;
    if (!SECRET_FILENAME_RE.test(path)) continue;

    // 파일 삭제는 노출이 아니라 오히려 정리이므로 대상에서 뺀다 — 다음 "diff --git"
    // 전까지 구간에서 "deleted file mode"가 보이면 이 블록은 삭제다.
    let isDeleted = false;
    for (let j = i + 1; j < allLines.length && !allLines[j].startsWith("diff --git "); j++) {
      if (allLines[j].startsWith("deleted file mode")) {
        isDeleted = true;
        break;
      }
    }
    if (isDeleted) continue;

    findings.push({ name: "시크릿으로 의심되는 파일명", line: `+++ b/${path}` });
  }
  return findings;
}

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
  const allLines = diff.split("\n");

  // 파일명 자체가 시크릿인 경우(내용 패턴 매치 여부와 무관) — 먼저 검사한다.
  const findings = findSecretFilenames(allLines);

  // 추가된 줄(+로 시작, 파일 헤더 +++ 제외)만 검사한다 — 삭제되는 줄에 있던 값은
  // push 이후 저장소에 안 남으니 대상이 아니다.
  const addedLines = allLines.filter((line) => line.startsWith("+") && !line.startsWith("+++"));

  // 멀티라인 문자열 리터럴(예: PEM 블록)은 시작줄 자체에 트레일링 주석을 못 달 수
  // 있으니, "직전 2줄 이내"에 허용 표시가 있으면 그 매치는 건너뛴다.
  const ALLOWLIST_MARKER = "pragma: allowlist-secret";
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
