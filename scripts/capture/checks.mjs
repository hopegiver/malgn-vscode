// 화면별 콘텐츠 검증 공용 헬퍼 — 스크린샷을 "찍기만" 하고 끝내지 않기 위해,
// 매 캡처마다 실제로 기대한 콘텐츠가 렌더됐는지 셀렉터/텍스트로 확인한다.
export async function bodyText(page) {
  try {
    return await page.locator('#app').innerText();
  } catch {
    return '';
  }
}

export async function count(page, selector) {
  try {
    return await page.locator(selector).count();
  } catch {
    return 0;
  }
}

export async function hasAlert(page) {
  return (await count(page, '.alert')) > 0;
}

const LOADING_MARKERS = ['불러오는 중', '확인 중'];

export async function hasLoadingText(page) {
  const text = await bodyText(page);
  return LOADING_MARKERS.some((m) => text.includes(m));
}

export async function hasSkeleton(page) {
  return (await count(page, '.skeleton-card')) > 0 || (await count(page, '.skeleton-grid')) > 0;
}

// 공통 판정: ok가 아니면 note에 실패 사유를 남긴다.
export function verdict(ok, note) {
  return { ok, note };
}
