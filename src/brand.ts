import logoMarkUrl from './assets/logo-mark.png';
import { el } from './dom';

export function brandMark(): HTMLImageElement {
  const img = el('img', { className: 'sidebar-brand-mark' });
  img.src = logoMarkUrl;
  img.alt = '맑은에이전트';
  return img;
}
