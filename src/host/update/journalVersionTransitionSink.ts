// U-4② 저널 기록 DI 구현체 — `src/update/versionTransitionRecord.ts`의
// `VersionTransitionSink`를 `core/journal/store.ts`(`JournalStore`)로 구현한다.
//
// [이 파일이 `src/update/**` 밖에 있는 이유] AT-U1(모듈 경계)이 `src/update/**`↔
// `src/core/policy/**`만 막지만, NT-R18의 취지("정책과 업데이트는 서로 다른 신뢰
// 뿌리")를 넓게 지키기 위해 이 구현체도 `src/update/`가 아니라 호스트 어댑터
// (`src/host/`)에 둔다 — 저널은 정책이 아니지만, 업데이트 채널의 DI 구현체는 전부
// composition root(호스트 어댑터)에 모아 "어디서 무엇이 조립되는가"를 한 곳에서
// 추적 가능하게 하는 것이 이 슬라이스의 배치 원칙이다.

import type { JournalStore } from '../../core/journal/store.js';
import type { VersionTransitionRecord, VersionTransitionSink } from '../../update/versionTransitionRecord.js';

export class JournalVersionTransitionSink implements VersionTransitionSink {
  readonly #journal: JournalStore;

  constructor(journal: JournalStore) {
    this.#journal = journal;
  }

  async record(entry: VersionTransitionRecord): Promise<void> {
    await this.#journal.appendVersionTransition({
      ts: entry.ts,
      fromVersion: entry.fromVersion,
      toVersion: entry.toVersion,
      trigger: entry.trigger,
    });
  }
}
