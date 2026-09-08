// 프리로드 — `contextIsolation:true` + `sandbox:true` 아래에서 렌더러가 쓸 수 있는
// 유일한 통로다. `nodeIntegration:false`라 렌더러는 `require`도 `node:*`도 못 쓴다 —
// 이 파일만 `electron`을 import하고, `contextBridge.exposeInMainWorld`로 딱 두 함수만
// (목록/상세 조회) 노출한다. 렌더러가 임의 IPC 채널을 직접 부르지 못하게 채널 이름을
// 렌더러 쪽 코드에 노출하지 않는다(`ipcContract.ts`의 `IPC_CHANNEL`은 이 파일 안에서만
// 소비된다).

import { contextBridge, ipcRenderer } from 'electron';
import { IPC_CHANNEL, type MalgnRendererApi, type ProjectDetailResult, type ProjectListItem } from './ipcContract.js';

const api: MalgnRendererApi = {
  listProjects: () => ipcRenderer.invoke(IPC_CHANNEL.listProjects) as Promise<readonly ProjectListItem[]>,
  getProjectDetail: (projectPath: string) => ipcRenderer.invoke(IPC_CHANNEL.getProjectDetail, projectPath) as Promise<ProjectDetailResult>,
};

contextBridge.exposeInMainWorld('malgn', api);
