// OTel 설정 화면의 실제 데이터 소스 — ~/.claude/settings.json의 env 객체에서
// "OTEL_"로 시작하는 키만 읽어온다(그 외 설정은 절대 넘어오지 않는다). 읽기
// 전용이다 — 이 값을 다시 파일에 쓰는 커맨드는 없다("저장"은 목업).
import { invoke } from '@tauri-apps/api/core';

export async function fetchOtelEnv(): Promise<Record<string, string>> {
  return invoke<Record<string, string>>('read_otel_env');
}
