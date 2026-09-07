// otel provider detect() — architecture.md §4.3 detect 표의 앞 두 단계(①헬퍼 실행
// 가능? ②OS 지원?)와 §4.2 O-4(`~/.claude/settings.json`의 `env`+`otelHeadersHelper`)
// 읽기를 구현한다. 부작용 0(읽기 전용 fs만), 절대 throw하지 않는다(PR-8).
//
// [정직 표기 — 이 슬라이스가 채우지 못한 신호] §4.3 표의 ③(헬퍼를 실제로 실행해
// Authorization을 꺼내는지)·④(OTLP 최소 payload POST로 수집기 도달성)·⑤(그 응답으로
// 인증 통과 여부)는 이 슬라이스에서 구현하지 않는다 — ③은 macOS Keychain 접근을
// 트리거해 최초 1회 사람의 허가 다이얼로그를 띄울 수 있는 부작용이라 "detect는 부작용
// 0"이라는 이 provider군 전체의 불변량과 충돌하고, ④·⑤는 실제 네트워크 왕복(OTLP
// protobuf 인코딩)이 필요해 이번 슬라이스의 범위를 넘는다. 헬퍼는 "경로가 설정돼 있고
// 실행 권한 비트가 있는지"(존재 확인)까지만 확인한다 — 반환문에 명시할 설계 범위 축소.

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import type { DetectContext, Observed } from '../../providers/types.js';
import { buildDefaultOtelEnv, OTEL_RESOURCE_ATTRIBUTES_KEY } from './desiredEnv.js';
import { claudeSettingsPath, readClaudeSettings, type PathExecutable, type ReadTextFile } from './settingsFile.js';
import {
  MV_OTEL_HELPER_MISSING,
  MV_OTEL_HELPER_UNSUPPORTED_OS,
  MV_OTEL_DRIFT,
  MV_OTEL_NO_TARGET_CONFIGURED,
  MV_OTEL_OK,
  MV_OTEL_SETTINGS_UNREADABLE,
} from './errors.js';

export type { PathExecutable };

export interface OtelDetectDeps {
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
  readonly pathExecutable: PathExecutable;
  readonly platform: NodeJS.Platform;
}

export interface OtelObservedDetail {
  readonly platformSupported: boolean;
  readonly settingsFileFound: boolean;
  readonly headersHelperConfigured: boolean;
  readonly headersHelperPath: string | null;
  readonly headersHelperExecutable: boolean | null;
  /** 요약용 — 값이 아니라 **키 이름만**(사람이 읽는 상태 메시지·로그용). 실제 3-way
   * 병합과 Change.before/after 생성은 `plan()`이 아래 `observedEnv`(원문 스냅샷)로
   * 다시 계산한다 — `plan()`은 네트워크·fs를 호출하지 않는 순수 함수(PR-2)라 필요한
   * 사실은 전부 `Observed`를 통해서만 전달받을 수 있기 때문이다. */
  readonly missingKeys: readonly string[];
  readonly mismatchedKeys: readonly string[];
  readonly resourceAttributesConfigured: boolean;
  /** `plan()`이 실제 정책 desired(사이트면 기본값과 다를 수 있다)와 다시 비교하기
   * 위한 원문 스냅샷. `settings.json`에 어차피 평문으로 존재하는 값이라(이 provider가
   * 새로 만드는 노출 표면이 아니다) 여기 담는다 — 로그 싱크(§6.2 마스킹)로 나갈 때는
   * 별도 필터를 거쳐야 한다(이 필드 자체가 그 필터는 아니다, 정직 표기). */
  readonly observedEnv: Readonly<Record<string, string>>;
}

function nowIso(): string {
  return new Date().toISOString();
}

function diffKeys(observedEnv: Readonly<Record<string, string>>, desiredEnv: Readonly<Record<string, string>>) {
  const missingKeys: string[] = [];
  const mismatchedKeys: string[] = [];
  for (const [key, value] of Object.entries(desiredEnv)) {
    if (!(key in observedEnv)) {
      missingKeys.push(key);
    } else if (observedEnv[key] !== value) {
      mismatchedKeys.push(key);
    }
  }
  return { missingKeys, mismatchedKeys };
}

export async function detectOtel(deps: OtelDetectDeps, _ctx: DetectContext): Promise<Observed> {
  const constants = loadCodeConstants();

  if (deps.platform !== 'darwin') {
    return {
      providerId: 'otel',
      status: 'blocked',
      code: MV_OTEL_HELPER_UNSUPPORTED_OS,
      message: `${deps.platform}는 아직 지원하지 않습니다 — macOS만 자동 프로비저닝합니다(R-2 실기 검증 전)`,
      observedAt: nowIso(),
    };
  }

  const settingsPath = claudeSettingsPath(deps.claudeHomeDir);
  const outcome = await readClaudeSettings(settingsPath, deps.readTextFile);

  if (outcome.kind === 'unreadable') {
    return {
      providerId: 'otel',
      status: 'unknown',
      code: MV_OTEL_SETTINGS_UNREADABLE,
      message: `settings.json을 읽을 수 없습니다: ${outcome.message}`,
      observedAt: nowIso(),
    };
  }

  const snapshot = outcome.kind === 'ok' ? outcome.snapshot : { env: {}, otelHeadersHelper: null };
  const settingsFileFound = outcome.kind === 'ok';

  const desired = buildDefaultOtelEnv(constants);
  if (desired.blocked) {
    return {
      providerId: 'otel',
      status: 'unknown',
      code: MV_OTEL_NO_TARGET_CONFIGURED,
      message: desired.blockedReason ?? '사이트면 코드 상수로 desired env를 만들 수 없습니다',
      observedAt: nowIso(),
    };
  }

  // 조직이 keychain-basic 헤더 헬퍼를 요구하는지는 `allowedKeychainItems`(사이트면)가
  // 비어 있지 않은지로 판단한다 — 정책의 `otel.headersHelper.kind`는 detect()가 접근할
  // 수 있는 입력이 아니다(agent/mcp와 같은 이유: desired는 plan() 단계에서만 주어진다).
  const headerHelperRequired = constants.allowedKeychainItems.length > 0;
  const headersHelperConfigured = snapshot.otelHeadersHelper !== null;

  let headersHelperExecutable: boolean | null = null;
  if (headersHelperConfigured) {
    headersHelperExecutable = await deps.pathExecutable(snapshot.otelHeadersHelper as string);
  }

  if (headerHelperRequired && (!headersHelperConfigured || headersHelperExecutable === false)) {
    return {
      providerId: 'otel',
      status: 'blocked',
      code: MV_OTEL_HELPER_MISSING,
      message: headersHelperConfigured
        ? '헤더 헬퍼가 설정돼 있으나 실행 권한이 없습니다'
        : '헤더 헬퍼(otelHeadersHelper)가 설정돼 있지 않습니다 — Keychain 자격증명 발급은 이 provider 범위 밖입니다',
      detail: {
        platformSupported: true,
        settingsFileFound,
        headersHelperConfigured,
        headersHelperPath: snapshot.otelHeadersHelper,
        headersHelperExecutable,
        missingKeys: [],
        mismatchedKeys: [],
        resourceAttributesConfigured: OTEL_RESOURCE_ATTRIBUTES_KEY in snapshot.env,
        observedEnv: snapshot.env,
      } satisfies OtelObservedDetail,
      observedAt: nowIso(),
    };
  }

  const { missingKeys, mismatchedKeys } = diffKeys(snapshot.env, desired.env);
  const resourceAttributesConfigured = OTEL_RESOURCE_ATTRIBUTES_KEY in snapshot.env;

  const detail: OtelObservedDetail = {
    platformSupported: true,
    settingsFileFound,
    headersHelperConfigured,
    headersHelperPath: snapshot.otelHeadersHelper,
    headersHelperExecutable,
    missingKeys,
    mismatchedKeys,
    resourceAttributesConfigured,
    observedEnv: snapshot.env,
  };

  if (missingKeys.length === 0 && mismatchedKeys.length === 0 && resourceAttributesConfigured) {
    return { providerId: 'otel', status: 'ok', code: MV_OTEL_OK, message: 'OTel 설정이 desired 상태와 일치합니다', detail, observedAt: nowIso() };
  }

  return {
    providerId: 'otel',
    status: 'drift',
    code: MV_OTEL_DRIFT,
    message: `OTel 설정이 desired 상태와 다릅니다(누락 ${missingKeys.length}건, 불일치 ${mismatchedKeys.length}건)`,
    detail,
    observedAt: nowIso(),
  };
}
