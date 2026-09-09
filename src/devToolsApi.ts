// "개발 환경" 화면의 실제 데이터 소스. Rust 커맨드 check_dev_tools()가
// `claude --version` 등 고정된 바이너리명·인자만 읽기 전용으로 실행해 설치 여부와
// 버전을 반환한다(src-tauri/src/lib.rs 주석 참고). 설치/업데이트 버튼은 이 값을
// 건드리지 않는 순수 목업이다.
import { invoke } from '@tauri-apps/api/core';

export interface DevToolStatus {
  readonly id: string;
  readonly name: string;
  readonly installed: boolean;
  readonly version: string | null;
}

export async function fetchDevTools(): Promise<DevToolStatus[]> {
  return invoke<DevToolStatus[]>('check_dev_tools');
}
