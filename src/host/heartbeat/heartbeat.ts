// 하트비트 — architecture.md §2.7 "앱은 재수렴 루프마다 앱 데이터 디렉터리에
// heartbeat.json{ts, pid, version}을 갱신한다." PR-8 후반부("죽으면 티가 난다") 요건의
// 신호 생산 지점. 실제 신선도 판정(비침해 감시자)은 `watchdogCheck.ts` — 이 파일과
// 분리해 둔 이유는 하트비트를 "쓰는" 프로세스(이 앱 자신)와 "읽고 판정하는" 프로세스
// (OS 스케줄러가 돌리는 별도 감시자 스크립트, §2.7 "감시자를 앱 자신의 프로세스 안에
// 두지 않는다")가 구조적으로 달라야 하기 때문이다 — 판정 로직을 여기 두면 감시자가
// 이 파일을 재사용할 때 "앱 프로세스 안의 감시자"처럼 보이는 결합이 생긴다.

import { writeFile } from 'node:fs/promises';

export interface HeartbeatContent {
  readonly ts: string; // ISO8601
  readonly pid: number;
  readonly version: string;
}

export const HEARTBEAT_FILE_MODE = 0o600;
export const HEARTBEAT_FILE_NAME = 'heartbeat.json';

/**
 * 하트비트 파일을 원자적으로 갱신한다(매 재수렴 루프 tick마다 호출). 비밀이 아니지만
 * 앱 데이터 디렉터리 관례(§6.1)를 따라 0600으로 쓴다.
 */
export async function writeHeartbeat(filePath: string, content: HeartbeatContent): Promise<void> {
  await writeFile(filePath, `${JSON.stringify(content)}\n`, { encoding: 'utf8', mode: HEARTBEAT_FILE_MODE });
}
