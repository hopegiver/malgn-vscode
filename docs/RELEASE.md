# 릴리스·롤백 운영 가이드

대상: `main` 브랜치 관리자(현재 1인 솔로 워크플로) + IT 담당자(Windows 루트 CA 배포 절차만 해당).
범위: `.github/workflows/tauri-release-build.yml`(설치형 릴리스)이 만드는 GitHub Release를 실제로 배포·롤백하는 절차. 워크플로 자체의 설계 근거(왜 draft인지, 왜 순차 실행인지 등)는 그 파일 상단 주석이 정본이고, 이 문서는 "사람이 무엇을 언제 눌러야 하는가"만 다룬다.

표기 규칙: 각 명령 옆에 검증 상태를 표시한다.
- ✅ 검증됨 — 이 작업 중 실제로 실행해 확인했다.
- 📄 문서 근거(미검증) — macOS 환경에서 실행할 수 없어(Windows 전용 명령·GPO) Microsoft 공식 문서 URL만 확인했다. 실제 동작은 Windows에서 처음 쓸 때 재확인 필요.

---

## 1. 릴리스 절차

1. **버전 범프**: `src-tauri/tauri.conf.json`의 `version`을 올려 커밋한다. `verify-version` 잡이 태그와 이 값을 대조하므로 둘이 다르면 빌드 자체가 막힌다.
2. **CI 초록불 확인**: `main`에 푸시 후 기존 CI(빌드/테스트)가 통과하는지 본다.
3. **태그 push**: `git tag v<X.Y.Z> && git push origin v<X.Y.Z>` — `v*.*.*` 패턴 태그 push가 `tauri-release-build.yml`의 유일한 정상 트리거다.
4. **여기서 자동으로 배포되지 않는다.** 워크플로가 하는 일은 다음까지다:
   - macOS(ad-hoc/무서명 `.app`+`.dmg`)와 Windows(NSIS, 시크릿 등록 시 서명)를 순차 빌드
   - GitHub Release를 **Draft 상태로** 생성하고 두 플랫폼 설치 파일 + `latest.json`(updater 매니페스트)을 자산으로 첨부
   - `verify-release-assets` 잡이 `latest.json`에 macOS·Windows 플랫폼 항목과 서명이 모두 있는지 기계적으로 확인

   Draft는 GitHub의 `releases/latest`에 잡히지 않으므로, 이 시점까지는 **어떤 사용자도 이 버전을 받지 않는다**(신규 다운로드도, 기존 설치본의 자동업데이트도).
5. **자산·서명 확인(사람)**: GitHub Release(초안) 페이지에서
   - Windows 설치 파일 서명 여부를 Actions 잡 요약(Job Summary — `Preflight - Windows 코드서명 시크릿 확인` 스텝)에서 확인
   - `latest.json`이 첨부돼 있고 `verify-release-assets` 잡이 초록불인지 확인
   - 가능하면 파일럿 PC 1~2대에서 설치 파일을 직접 내려받아 실행까지 왕복해본다(특히 Windows 무서명 빌드는 이 단계에서 SmartScreen/게시자 경고가 예상대로 뜨는지 확인).
6. **Publish**: 문제없으면 Release 페이지에서 "Publish release"를 누른다. 이 순간부터 `releases/latest/download/latest.json`이 이 버전을 가리키고, 기존 설치본의 자동업데이트 대상이 된다.

재시도가 필요하면(시크릿을 뒤늦게 등록했거나 일시 실패) 새 태그를 만들지 말고 `workflow_dispatch`에 기존 태그를 입력해 재실행한다.

---

## 2. 롤백 정책

### 2-a. 나쁜 릴리스를 발견하면 — 즉시 노출을 멈춘다

Publish 전(Draft 상태)이면 아무 조치도 필요 없다 — 애초에 아무도 받지 않았다.

**이미 Publish한 뒤** 문제를 발견하면, 신규 다운로드·자동업데이트를 즉시 멈추는 방법은 두 가지다.

```bash
# (a) Draft로 되돌린다 — 자산은 남기고 노출만 끈다. 권장 기본 조치.
gh release edit <tag> --draft=true

# (b) 아예 삭제한다 — 태그까지 지우려면 --cleanup-tag 추가(신중히).
gh release delete <tag>
```
📄 문서 근거(미검증) — 두 명령 모두 `gh release edit --help` / `gh release delete --help`로 문법을 확인했다(이 작업에서 실제 릴리스에 실행하지는 않았다 — 실행 금지가 이번 작업 범위였다). `gh release edit <tag> --draft=false`가 draft→published 전환 예시로 공식 도움말에 있으므로 반대 방향(`--draft=true`)도 동일 문법으로 동작할 것으로 판단한다.

**왜 이게 먹히는가**: `releases/latest/download/latest.json`은 GitHub이 "가장 최근의 **published, non-draft, non-prerelease** 릴리스"를 찾아 리다이렉트하는 엔드포인트다. 대상 릴리스를 draft로 되돌리거나 지우면 이 엔드포인트는 그 이전 published 릴리스로 다시 리다이렉트된다(다른 published 릴리스가 없으면 404).

**되돌린 뒤 확인하는 방법**(✅ 검증됨 — 이번 작업 중 실제로 v0.2.3에 대해 실행):
```bash
curl -sSL -D - -o /dev/null https://github.com/hopegiver/malgn-vscode/releases/latest/download/latest.json
```
응답 헤더의 `location:`이 어느 태그의 `latest.json`으로 리다이렉트되는지 보여준다(예: `location: https://github.com/hopegiver/malgn-vscode/releases/download/v0.2.3/latest.json`). draft로 되돌린 태그가 더 이상 여기 나타나지 않으면 성공이다.

### 2-b. Tauri updater는 롤포워드 전용이다 — 구버전 재게시로 못 돌아간다

2-a로 신규 유입은 막아도, **그 버전으로 이미 자동업데이트된 단말은 구버전으로 되돌릴 수 없다.** updater는 원격 `latest.json`의 버전이 현재 설치된 버전보다 **클 때만** 다운로드를 진행하는 단방향 semver 비교로 동작하기 때문이다 — 구버전을 다시 Publish해도 "더 낮은 버전"으로 보여 무시된다.

즉 나쁜 릴리스가 이미 퍼진 뒤의 유일한 자동업데이트 경로는 **버그를 고친 코드에 새 버전 번호를 매겨 1절 절차대로 다시 내보내는 것**(forward-fix)이다. 릴리스 노트에 이전 버전이 왜 회수됐는지 한 줄 남겨둔다.

### 2-c. 급할 때 — 사용자 수동 복구

forward-fix가 나올 때까지 기다릴 수 없을 만큼 급하면(예: 신버전이 아예 기동 자체가 안 되는 사고), 사용자가 직접 이전 버전 설치 파일로 재설치한다.

1. GitHub Releases(https://github.com/hopegiver/malgn-vscode/releases)에서 문제없던 마지막 태그를 열어 설치 파일을 내려받는다.
   - Windows: `malgn-agent_<버전>_x64-setup.exe`
   - macOS: `.dmg` 또는 `.app.tar.gz`
2. 기존 앱을 종료하고 새 설치 파일로 설치(Windows NSIS는 덮어쓰기 설치, macOS는 `.dmg`를 열어 `/Applications`에 다시 드래그)한다.
3. **주의**: 이 상태에서 앱을 그대로 두면, `releases/latest`가 다시 문제 버전(아직 draft로 되돌리지 않았다면)을 가리킬 때 자동업데이트가 재실행되어 도로 문제 버전으로 올라갈 수 있다. **2-a로 문제 버전을 먼저 draft/삭제 처리한 뒤에** 사용자 수동 복구를 안내하는 순서를 지킨다. 순서가 바뀌면 수동 복구가 무의미해진다.

---

## 3. Windows 코드서명 신뢰 체인 — IT 담당자용 루트 CA 배포

### 3-0. 배경

Windows 설치 파일은 유료 인증서 없이 **사내 자체 루트 CA**로 서명된다(코드 서명은 계속 무료로 유지하기로 결정됨).

서명 체인(보안 점검에서 실측 확인):
```
Malgnsoft malgn-agent Code Signing  ←(발급자)  Malgnsoft Internal Code Signing Root CA
```
타임스탬프는 DigiCert TSA를 사용하지만 이건 "언제 서명했는지"만 증명할 뿐 신뢰 체인과 무관하다.

**이 루트 CA가 45대 PC의 "신뢰할 수 있는 루트 인증 기관" 저장소에 없으면**, 서명 자체는 유효해도 Windows는 발급자를 알 수 없는 인증서로 취급한다 — 설치는 되지만 게시자가 "알 수 없음"으로 뜨고 SmartScreen 경고를 사용자가 직접 넘겨야 한다. **루트 CA 설치는 필수가 아니라 권장이다**(설치 안 해도 설치·실행 자체는 가능, 경고만 매번 감수).

> ⚠️ **보안 주의**: 루트 CA를 PC의 신뢰 저장소에 넣는다는 것은, **그 CA로 서명된 모든 바이너리**를 그 PC가 자동으로 신뢰하게 된다는 뜻이다(이 앱 하나만이 아니다). 사내에서 직접 발급·관리하는 CA이고 PFX·비밀번호는 GitHub Actions secret으로만 존재해 유출 경로가 좁으므로 이번 결정은 수용 가능한 트레이드오프이지만, 이 CA의 개인키가 노출되면 45대 전 PC가 그 키로 서명된 어떤 실행 파일이든 신뢰하게 된다는 점을 IT가 인지하고 있어야 한다.

### 3-1. 루트 CA 공개 인증서 파일을 어디서 구하는가

PFX(개인키 포함)는 시크릿이라 배포할 수 없지만, **루트 CA의 공개 인증서(.cer)는 비밀이 아니다.** 파일을 구하는 방법은 두 가지다.

**우선순위 0 — 가장 쉬운 경로부터 확인**: 이 루트 CA를 처음 발급한 사람/스크립트가 이미 공개 인증서 파일(`.cer`/`.crt`)을 갖고 있을 가능성이 높다. Windows PFX 임포트에 쓰인 `WINDOWS_CERTIFICATE` secret을 최초 등록한 담당자에게 루트 CA 원본 공개 인증서 보유 여부를 먼저 확인하는 것이 가장 빠르다.

**우선순위 1 — 게시된 설치 파일에서 추출 시도 (이번 작업에서 실제로 시도함, 결과: 실패)**

시도한 내용:
```bash
gh release download v0.2.3 --repo hopegiver/malgn-vscode --pattern '*.exe' --dir <scratch>
osslsigncode extract-signature -in malgn-agent_0.2.3_x64-setup.exe -out sig.pkcs7   # brew install osslsigncode
openssl pkcs7 -in sig.pkcs7 -inform DER -print_certs -out certs.pem
```
✅ 검증됨 — 실제로 v0.2.3의 게시된 exe를 내려받아 실행했다.

**결과**: 서명에 포함된 인증서는 4개였다 — 리프 서명서(`Malgnsoft malgn-agent Code Signing`) 1개 + DigiCert 타임스탬프 체인(루트/중간/응답자) 3개. **루트 CA 자신(`Malgnsoft Internal Code Signing Root CA`, 자체서명)은 포함돼 있지 않았다.** Authenticode 서명은 서명자 인증서(+PFX에 함께 담긴 중간 CA가 있다면 그것)만 내장하고, 이미 신뢰되고 있어야 할 루트는 보통 함께 담지 않는다 — 지금 체계는 "루트가 리프를 직접 발급"하는 2단 구조라 중간 CA도 없다. 즉 **이 경로로는 목표 파일(루트 CA .cer)을 얻을 수 없다** — 기술적으로 잘못한 게 아니라 Authenticode의 정상 동작이다.

개인키 혼입 여부 확인(커밋 전 필수 점검이었으나, 애초에 목표 파일을 얻지 못해 커밋 후보 자체가 없다): 추출된 모든 파일(`sig.pkcs7`, `certs.pem`, 개별 cert 4개)에 `PRIVATE KEY` 문자열이 0건임을 `grep`으로 확인했고, 구조적으로도 Authenticode 서명(PKCS7 SignedData)에는 인증서·서명값만 들어가고 개인키가 들어갈 필드 자체가 없다. ✅ 검증됨.

**결론**: 이번 라운드에서 저장소에 커밋을 제안할 루트 CA 파일은 없다. 아래 3-2 절차로 직접 내보내야 한다.

**우선순위 2 — Windows에서 서명된 exe로부터 IT가 직접 내보내기 (우선순위 0·1 모두 안 되면)**

이 절차는 macOS에서 실행할 수 없어 화면을 직접 캡처하지 못했다 — 아래는 Microsoft 공식 문서(Azure Application Gateway "trusted client CA certificate" 가이드)의 절차를 그대로 옮긴 것이다. 📄 문서 근거(미검증), 정본: https://learn.microsoft.com/en-us/azure/application-gateway/mutual-authentication-certificate-management

1. 서명된 설치 파일(`malgn-agent_<버전>_x64-setup.exe`)을 우클릭 → **속성**.
2. **디지털 서명** 탭 → 서명 목록에서 `Malgnsoft malgn-agent Code Signing`을 선택 → **자세히**.
3. **인증서 보기** 클릭.
4. **인증 경로** 탭 → 트리 최상단의 `Malgnsoft Internal Code Signing Root CA`(루트)를 선택 → **인증서 보기**.
5. 새로 열린 루트 인증서 창에서 **자세히** 탭 → **파일에 복사** 클릭 → **인증서 내보내기 마법사** 시작.
6. 마법사에서 **Base-64로 인코딩된 X.509(.CER)** 형식을 선택하고(개인키는 애초에 이 인증서에 없으므로 "개인 키 내보내기" 질문 자체가 나오지 않는다) 저장 경로를 지정 → **마침**.
7. 결과로 생긴 `.cer` 파일이 배포할 루트 CA 공개 인증서다. 이 파일은 비밀이 아니므로 이 저장소에 커밋해도 된다(예: `docs/malgnsoft-internal-code-signing-root-ca.cer`) — 커밋 전 `openssl x509 -in <파일> -noout -subject -issuer`로 subject==issuer(자체서명, 루트가 맞음)인지, `certs.pem`처럼 `grep -c "PRIVATE KEY"`가 0인지 한 번 더 확인한다.

### 3-2. 설치 방법

`.cer` 파일을 확보한 뒤, 다음 두 경로 중 규모에 맞는 쪽을 쓴다. 둘 다 저장소는 **`Root`(신뢰할 수 있는 루트 인증 기관)**다 — `My`(개인)나 `CA`(중간)에 넣으면 신뢰 체인이 완성되지 않는다.

**① 개별 PC(소수 대상, 즉시 적용)**
```cmd
certutil -addstore Root malgn-root-ca.cer
```
📄 문서 근거(미검증), 정본: https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/certutil (`-addstore CertificateStoreName InFile` 문법, `Root`가 신뢰할 수 있는 루트 저장소 이름임을 같은 문서의 `-store` 절에서 확인).
- 로컬 컴퓨터 저장소(모든 사용자 적용)에 넣으므로 **관리자 권한으로 명령 프롬프트를 실행**해야 한다(일반 권한 프롬프트에서는 로컬머신 인증서 저장소 쓰기가 거부된다 — Windows 인증서 저장소 권한 모델의 일반적 동작, 이 명령 자체에 대한 별도 공식 언급은 없어 미검증 표기 유지).
- 관리자 권한 없이 현재 로그인한 사용자에게만 적용하려면 `-user` 옵션을 추가한다(`certutil -addstore -user Root malgn-root-ca.cer`). 45대 전체에 일괄 적용할 거라면 ②(GPO)가 더 적합하다.

**② 45대 일괄 배포(그룹 정책, GPO)**

사내 도메인/AD 환경이면 로그온 시 자동 적용되는 GPO가 개별 PC 작업보다 안전하다.
1. 도메인 컨트롤러(또는 RSAT 설치 PC)에서 **그룹 정책 관리** 스냅인 실행.
2. 대상 GPO(또는 신규 GPO)를 우클릭 → **편집**.
3. 콘솔 트리에서 **컴퓨터 구성 → 정책 → Windows 설정 → 보안 설정 → 공개 키 정책**을 열고, **신뢰할 수 있는 루트 인증 기관**을 우클릭 → **가져오기**.
4. 인증서 가져오기 마법사에서 확보한 `.cer` 파일 경로를 지정하고 **다음**.
5. **인증서 저장소**에서 "모든 인증서를 다음 저장소에 저장"을 선택하고 **신뢰할 수 있는 루트 인증 기관**을 지정 → **다음** → **마침**.
6. 정책을 대상 OU/사이트/도메인에 적용한 뒤, 대상 PC를 재부팅(또는 `gpupdate /force`)해야 반영된다.

📄 문서 근거(미검증), 정본: https://learn.microsoft.com/en-us/windows-server/identity/ad-cs/distribute-certificates-group-policy (콘솔 트리 경로·마법사 단계를 그대로 옮김. 이 문서 원문은 "개인 키 포함 인증서" 배포를 전제로 "개인 키 보호" 단계가 있으나, 루트 CA 공개 인증서만 배포하는 이번 경우 그 단계는 나타나지 않거나 건너뛰게 된다 — 이 차이는 미검증).

### 3-3. 설치하지 않으면 어떻게 되는가

설치는 **필수가 아니라 권장**이다. 설치하지 않아도 실행 자체는 가능하지만:
- 설치 파일 실행 시 게시자가 "알 수 없는 게시자"로 표시되거나, 신뢰되지 않은 인증서 경고가 뜬다.
- Windows SmartScreen("Windows의 PC 보호")이 뜨면 **추가 정보 → 실행** 순서로 직접 넘겨야 실행된다.

다만 한 가지 짚어둘 부분: 루트 CA를 설치해 **인증서 체인 신뢰**가 해결돼도, SmartScreen의 "실행 파일 평판" 경고는 원리상 별개의 클라우드 평판 시스템이라 완전히 사라진다는 보장은 없다(자체 CA는 공인 CA와 달리 Microsoft 평판 데이터베이스에 등록돼 있지 않다) — 이 부분은 이번 작업에서 실기 확인하지 못했다(**미검증**, 다음 배포 시 IT가 직접 실 PC에서 확인 권장). 확실히 달라지는 것은 **게시자 이름이 "Malgnsoft"로 정상 표시되고, "인증서 체인을 신뢰할 수 없음" 류의 강한 차단 경고가 사라진다**는 점이다.

---

## 4. macOS Gatekeeper 우회 안내

macOS 빌드는 계속 미서명(ad-hoc)이다. 첫 실행 시 "확인되지 않은 개발자" 또는 "손상됨" 경고가 뜬다.

아래 안내는 이 저장소의 기존 문구를 그대로 재사용한다 — `.github/workflows/tauri-portable-build.yml`의 `실행방법.txt` 생성 스텝(포터블 zip 동봉)과 `README.md`("macOS 포터블 빌드 실행" 절)에 이미 검증된 문구가 있다.

1. **우선 시도 — 우클릭(또는 control+클릭) → 열기**: Finder에서 앱을 control+클릭(또는 우클릭) 후 **열기**를 선택하면 경고가 뜨지만 **열기** 버튼으로 진행할 수 있다. 한 번 이렇게 열면 이후에는 예외로 등록돼 더블클릭으로도 열린다.
2. **그래도 안 열리면 — 터미널에서 격리 속성 제거**: 이 저장소가 이미 쓰는 문구 그대로 —
   ```bash
   xattr -cr <앱 이름>.app
   ```
   (포터블 zip 배포본은 `실행방법.txt`가 같은 명령을 안내한다: `tauri-portable-build.yml`의 "Zip .app bundle (macOS)" 스텝 참고. 설치형 `.dmg`로 `/Applications`에 설치했다면 `xattr -cr /Applications/malgn-agent.app`.)
3. **macOS 최신 버전(Sequoia 이상)에서 1번 메뉴 자체가 막혀 있는 경우**: **시스템 설정 → 개인정보 보호 및 보안**으로 이동해 보안 섹션에서 이 앱이 차단됐다는 안내와 함께 뜨는 **"그래도 열기"** 버튼을 누른다.
   📄 문서 근거(미검증 — 이번 작업의 macOS는 이미 개발자 신뢰 설정이 돼 있어 실제 차단 화면을 재현하지 못했다), 정본(한국어): https://support.apple.com/ko-kr/guide/mac-help/mh40616/mac

---

## 5. 유지보수 노트

- **`src-tauri/tauri.conf.json`의 `bundle.targets`는 `"all"`이지만, 실제 배포 채널은 NSIS 하나로 확정돼 있다.** Windows 전용 오버라이드 `src-tauri/tauri.windows.conf.json`의 `bundle.targets`가 `["nsis"]`로 명시돼 있어(Tauri 설정은 플랫폼별 파일이 전역 설정을 좁히는 방향으로 병합된다) 실제 릴리스 자산에는 `.msi`가 생기지 않는다. `latest.json`의 `windows-x86_64`와 `windows-x86_64-nsis` 항목이 지금 같은 자산 id를 가리키는 것도 이 때문이다(✅ 검증됨 — 2절에서 실행한 `curl` 결과로 두 키의 자산 URL이 동일함을 확인).
- 이 설정(`targets: ["nsis"]`)을 **지금 바꾸지는 않았다.** 다만 향후 툴체인·설정이 바뀌어 MSI가 다시 섞여 나오게 되면, `latest.json`에서 `windows-x86_64` 항목이 NSIS·MSI 중 어느 자산을 가리키는지 반드시 재확인해야 한다 — MSI는 per-machine(관리자 권한) 설치라 NSIS(현재 방식, 사용자 단위·무권한 설치)와 설치 위치·업데이트 동작이 달라, 두 방식이 뒤섞이면 "매 실행마다 업데이트를 재감지하는" 상태에 빠질 수 있다.
- 릴리스 워크플로 자체의 설계 배경(draft 게이트 이유, 순차 실행 이유, 시크릿 미등록 시 동작 등)은 `.github/workflows/tauri-release-build.yml` 상단 주석이 정본이다 — 이 문서와 내용이 어긋나면 그 주석을 우선한다.
