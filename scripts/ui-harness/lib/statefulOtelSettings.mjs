// 설정 > OTel 화면용 "상태 있는" IPC 스텁.
//
// 왜 필요한가: bridge.mjs의 픽스처는 커맨드마다 고정 응답이다. 그런데 이 화면은 저장 뒤·연동 이벤트 뒤·탭 재진입 때
// `otel_settings_get`을 다시 읽는다. 실제 Rust의 `otel_settings_save`는 쓴 뒤 `otel_settings_get()`을 그대로 돌려주므로
// (src-tauri/src/otel_settings.rs) "저장 응답 = 이후 조회 응답"이다. 고정 픽스처로는 이 관계를 흉내 낼 수 없어서, 저장 응답에
// 새 값을 줘도 곧바로 이어지는 재조회가 옛 값으로 덮어 쓰는 거짓 실패가 났다. 이 스텁은 settings 한 벌을 보관하고
// 저장이 그 한 벌을 바꾸게 해서 둘을 일치시킨다.
//
// 이 파일의 함수는 Playwright `page.addInitScript(fn, arg)`로 문자열 직렬화되어 실행된다 — 바깥 변수를 참조할 수 없고
// 인자(arg)로 받은 값만 쓴다(bridge.mjs와 같은 제약). 반드시 installTauriBridge **뒤에** 주입한다(그 invoke를 감싼다).
//
// arg = { settings: OtelSettings, link?: IngestLinkStatus }
//   - settings: 파일 상태의 시작값. `otel_settings_get`이 이 값을 돌려주고, `otel_settings_save`가 갱신한다.
//   - link: 주면 연동 상태 조회·재시도·재확인(otel_ingest_link_status/otel_link_retry/otel_link_recheck)도 이 값을 돌려준다.
//
// 시나리오가 쓰는 조작·관찰 창구(모두 page.evaluate로 호출):
//   window.__otelStub.setFile(settings)  — 앱 밖에서 파일이 바뀐 것(자동 연동의 서버 권장값 적용 등)을 흉내 낸다.
//   window.__otelStub.setLink(link)      — 연동 상태 응답을 바꾼다.
//   window.__otelStub.saves              — 받은 otel_settings_save 인자 목록(payload 포함).
//   window.__otelStub.calls              — otel_settings_get/otel_settings_save 호출 순서.
export function installStatefulOtelSettings(arg) {
  const clone = (v) => JSON.parse(JSON.stringify(v));
  let file = clone(arg.settings);
  let link = arg.link ? clone(arg.link) : null;
  const saves = [];
  const calls = [];
  window.__otelStub = {
    setFile: (s) => {
      file = clone(s);
    },
    setLink: (l) => {
      link = clone(l);
    },
    saves,
    calls,
  };

  const orig = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
    if (cmd === 'otel_settings_get') {
      window.__invokeLog.push({ cmd, args, at: Date.now() });
      calls.push('otel_settings_get');
      return clone(file);
    }
    if (cmd === 'otel_settings_save') {
      window.__invokeLog.push({ cmd, args, at: Date.now() });
      calls.push('otel_settings_save');
      saves.push(clone(args));
      const payload = args?.payload ?? {};
      // 실제 저장 엔진과 같은 규칙: 값이 있으면 쓰고, 빈 값은 삭제 신호다.
      for (const [key, value] of Object.entries(payload.values ?? {})) {
        if (value === '') delete file.values[key];
        else file.values[key] = value;
      }
      const change = payload.ingestKey;
      if (change?.action === 'set') file.ingestKey = { state: 'set', masked: `mhk_${String(change.key).slice(4, 8)}…` };
      else if (change?.action === 'clear') file.ingestKey = { state: 'unset', masked: null };
      return clone(file);
    }
    if (link && (cmd === 'otel_ingest_link_status' || cmd === 'otel_link_retry' || cmd === 'otel_link_recheck')) {
      window.__invokeLog.push({ cmd, args, at: Date.now() });
      return clone(link);
    }
    return orig(cmd, args);
  };
}
