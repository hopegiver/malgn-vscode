// 메인 창(대시보드) — architecture.md §2.8 "렌더러 표면 강제 설정"의 실제 구현이자
// **창 생성 지점을 이 파일 하나로 고정**하는 지점이다(`src/architecture-tests/
// rendererWindowSecurityInvariant.test.ts`가 `BrowserWindow` 생성자 호출이 저장소
// 전체에서 정확히 여기 한 곳에만 등장함을 grep으로 강제한다 — §2.8 "창 생성 지점이
// 하나임을 강제하고 그 지점의 옵션 객체를 검사한다"). 고정값: `nodeIntegration:false` ·
// `contextIsolation:true` · `sandbox:true` · `webSecurity:true` · `will-navigate` 차단 ·
// `setWindowOpenHandler`는 deny 기본.
//
// [apply()를 호출하지 않는다] 이 파일은 창을 열 뿐이다 — "창을 여는 것은 apply가
// 아니다"(작업 지시). `Provider.apply`를 import하지 않는다.
//
// [싱글턴] 트레이 메뉴를 여러 번 클릭해도 창을 중복 생성하지 않는다 — 이미 열려
// 있으면 포커스만 준다.

import { BrowserWindow } from 'electron';

export interface CreateMainWindowDeps {
  readonly preloadPath: string;
  readonly indexHtmlPath: string;
}

let mainWindowSingleton: BrowserWindow | null = null;

export function openMainWindow(deps: CreateMainWindowDeps): BrowserWindow {
  if (mainWindowSingleton && !mainWindowSingleton.isDestroyed()) {
    mainWindowSingleton.show();
    mainWindowSingleton.focus();
    return mainWindowSingleton;
  }

  const win = new BrowserWindow({
    width: 1120,
    height: 760,
    minWidth: 720,
    minHeight: 480,
    title: 'Malgn',
    show: false,
    webPreferences: {
      preload: deps.preloadPath,
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
      webSecurity: true,
    },
  });

  // §2.8 "will-navigate 차단" — 렌더러가 로컬 index.html 밖으로 내비게이션하지 못한다.
  win.webContents.on('will-navigate', (event) => {
    event.preventDefault();
  });
  // §2.8 "setWindowOpenHandler는 deny 기본" — 새 창/팝업을 막는다(외부 링크는 렌더러가
  // 텍스트로만 보여줄 뿐 클릭 가능한 임의 팝업 표면을 만들지 않는다).
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));

  win.once('ready-to-show', () => win.show());
  win.on('closed', () => {
    mainWindowSingleton = null;
  });

  void win.loadFile(deps.indexHtmlPath);

  mainWindowSingleton = win;
  return win;
}

/** 테스트 전용 — 싱글턴을 초기화한다(실제 런타임에서는 쓰지 않는다). */
export function __resetMainWindowSingletonForTests(): void {
  mainWindowSingleton = null;
}
