// U-1/F-1(architecture.md §3.6.2·tech-stack.md §5.3) — `UpdateTransport` DI 구현체.
// "정적 HTTPS 호스팅" 원문 그대로: 매니페스트 = `https://<authority>/manifest.json`,
// 아티팩트 = 매니페스트가 지시한 URL. **이 파일은 서명 검증을 하지 않는다** —
// `runUpdateFlow`(src/update/updateFlow.ts)가 이 transport가 가져온 원문을
// `verifyUpdateManifest`/`assertArtifactMatchesVerifiedManifest`에 넘겨 검증한다(U-1
// 원문: "TLS는 검증이 아니다"). 이 파일의 책임은 순수하게 "authority에서 바이트를
// 가져오는 것"뿐이다.
//
// [최소 방어 — https: 스킴 강제] F-1 "정적 HTTPS 호스팅" 요구를 아티팩트 URL에도
// 적용한다. 매니페스트는 아직 서명 검증 전 원문(`unknown`)이라 `artifactUrl`이
// 신뢰할 수 없는 값일 수 있지만(서명 검증은 `runUpdateFlow`가 이후에 한다), 스킴만은
// transport 단계에서 확인해 `http:`·`file:` 등으로의 우회를 막는다. 이 이상의 제약
// (예: 매니페스트와 같은 host만 허용)은 아티팩트가 별도 CDN에 있을 수 있다는
// 정당한 배포 구성을 막을 수 있어 이 슬라이스에서 규칙으로 확정하지 않는다 — F-1
// 원문이 요구하지 않는 것을 이 파일이 새로 만들지 않는다.

import type { UpdateTransport } from '../../update/updateFlow.js';

export class UpdateArtifactSchemeRejectedError extends Error {
  constructor(actualUrl: string) {
    super(`업데이트 아티팩트 URL은 https:만 허용됩니다: ${actualUrl}`);
    this.name = 'UpdateArtifactSchemeRejectedError';
  }
}

export class UpdateTransportHttpError extends Error {
  constructor(url: string, status: number) {
    super(`업데이트 채널 요청 실패: ${url} → HTTP ${status}`);
    this.name = 'UpdateTransportHttpError';
  }
}

/** Node 18+ 내장 `fetch`만 쓴다(tech-stack.md §3 "의존하지 않기로 한 것들 — HTTP
 * 클라이언트"). 새 런타임 의존성을 들이지 않는다. */
export class HttpUpdateTransport implements UpdateTransport {
  async fetchManifest(authority: string): Promise<unknown> {
    const url = `https://${authority}/manifest.json`;
    const response = await fetch(url);
    if (!response.ok) {
      throw new UpdateTransportHttpError(url, response.status);
    }
    return response.json();
  }

  async fetchArtifact(artifactUrl: string): Promise<Buffer> {
    const parsed = new URL(artifactUrl);
    if (parsed.protocol !== 'https:') {
      throw new UpdateArtifactSchemeRejectedError(artifactUrl);
    }
    const response = await fetch(artifactUrl);
    if (!response.ok) {
      throw new UpdateTransportHttpError(artifactUrl, response.status);
    }
    const arrayBuffer = await response.arrayBuffer();
    return Buffer.from(arrayBuffer);
  }
}
