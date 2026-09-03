// 진입점. activate/deactivate만. 로직 없음 (architecture.md §1.1).
//
// §2.2가 정의하는 실제 활성화 시퀀스(호환 게이트 → 정책 로드 → 킬 스위치 확인 →
// provider detect() → plan() → 상태 표출)는 W2(정책 로더)·W4(킬 스위치)·W5(저널)가
// 갖춰진 뒤에야 의미 있게 조립된다. 이번 슬라이스(W1)는 그 조립에 쓰일 core(reconciler
// engine)·providers(균일 인터페이스 + 레지스트리) 계약만 제공하므로, activate()는 확장이
// VS Code에 로드되고 살아있음을 보여주는 것 이상을 하지 않는다.
import * as vscode from 'vscode';

let statusBarItem: vscode.StatusBarItem | undefined;

export function activate(_context: vscode.ExtensionContext): void {
  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  statusBarItem.text = '$(sync~spin) Malgn';
  statusBarItem.show();
  // §2.2 ①~⑦의 나머지 단계는 이후 작업 단위(W2~W5)가 갖춰진 뒤 여기서 조립된다.
  // 이 시점에는 절대 apply()를 호출하지 않는다 — 그것이 불변량이다.
}

export function deactivate(): void {
  statusBarItem?.dispose();
  statusBarItem = undefined;
}
