// ==================== 실행/미리보기 전역 락 ====================
// `EXECUTION_LOCK`(실설치·업데이트)과 `PREVIEW_LOCK`(dry-run 미리보기), 그리고 둘이
// 공유하는 `try_acquire_lock`. process.rs(1,000줄 규율)에서 분리했다.

use std::sync::Mutex;

/// 실설치/업데이트(`perform_update`/`perform_install`)를 직렬화하는 전역 락. 반드시
/// `try_acquire_lock`으로만 잡는다 — 이 락은 `()`만 보호하므로 한 번의 패닉이 만든
/// poison이 이후 모든 실행을 영구 거부하면 안 된다.
pub(crate) static EXECUTION_LOCK: Mutex<()> = Mutex::new(());

/// 미리보기(`preview_dev_tool_update` — brew dry-run 등 최대 120초짜리 자식 프로세스)의
/// 동시 실행을 1건으로 제한하는 락. `EXECUTION_LOCK`과 일부러 별개다:
/// - 미리보기끼리는 대기 없이 거부한다(`PREVIEW_BUSY_MESSAGE`). 같은 패키지 매니저에
///   dry-run을 겹쳐 띄우면 서로의 인덱스 갱신·락과 부딪히고, 연타하면 자식이 무제한 쌓인다.
/// - 미리보기와 실설치는 서로를 막지 않는다. 미리보기는 참고용이고 실행 안전성은
///   `plan_id` 재계산(`perform_update`/`perform_install`이 락 안에서 대조)이 지킨다.
///   미리보기가 `EXECUTION_LOCK`을 쥐면 긴 설치 중 다른 도구의 미리보기가 막히고,
///   반대로 설치가 미리보기를 기다리면 최대 120초 지연된다.
pub(crate) static PREVIEW_LOCK: Mutex<()> = Mutex::new(());

/// `EXECUTION_LOCK`이 이미 잡혀 있을 때(= 실제로 다른 설치/업데이트가 실행 중) 사용자에게
/// 보이는 문구. poison(패닉 잔재)은 이 문구를 쓰지 않는다 — 회복하고 진행한다.
pub(crate) const EXECUTION_BUSY_MESSAGE: &str =
    "다른 업데이트가 이미 실행 중입니다. 완료 후 다시 시도해주세요.";

/// `PREVIEW_LOCK`이 이미 잡혀 있을 때 사용자에게 보이는 문구(프론트가 "미리보기를
/// 가져오지 못했습니다 — {문구}" 토스트로 그대로 보여준다).
pub(crate) const PREVIEW_BUSY_MESSAGE: &str =
    "다른 미리보기가 진행 중입니다. 완료 후 다시 시도해주세요.";

/// `lock`을 대기 없이 잡는다. 이미 누군가 쥐고 있으면(`WouldBlock`) `None` — 호출자가
/// 용도에 맞는 "실행 중" 문구로 거부한다. 쥔 쪽이 패닉해 poison된 경우(`Poisoned`)는
/// "실행 중"이 아니라 "죽은 쪽이 남긴 흔적"이므로 가드를 회복(`into_inner`)하고 poison
/// 표식을 지운 뒤 정상 진행한다(보호 대상이 `()`라 깨진 상태가 있을 수 없다). 이 둘을
/// 한 `Err`로 뭉개면 poison 뒤 해당 기능이 앱 재시작 전까지 영구히 "실행 중"으로 보인다.
pub(crate) fn try_acquire_lock(lock: &Mutex<()>) -> Option<std::sync::MutexGuard<'_, ()>> {
    match lock.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => {
            eprintln!("[locks] 락이 이전 패닉으로 poison되어 있어 회복하고 진행합니다");
            lock.clear_poison();
            Some(poisoned.into_inner())
        }
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 다른 스레드가 가드를 쥔 채 패닉해 poison된 락을 만든다.
    fn poisoned_lock() -> Arc<Mutex<()>> {
        let lock = Arc::new(Mutex::new(()));
        let cloned = Arc::clone(&lock);
        let joined = std::thread::spawn(move || {
            let _guard = cloned.lock().unwrap();
            panic!("락을 쥔 채 패닉(테스트용 poison 유발)");
        })
        .join();
        assert!(joined.is_err());
        assert!(lock.is_poisoned(), "전제: 락이 poison되어 있어야 한다");
        lock
    }

    #[test]
    fn free_lock_is_acquired_and_released_on_drop() {
        let lock = Mutex::new(());
        {
            let guard = try_acquire_lock(&lock);
            assert!(guard.is_some());
        }
        assert!(try_acquire_lock(&lock).is_some(), "가드를 놓으면 다시 잡혀야 한다");
    }

    #[test]
    fn held_lock_is_would_block_not_recovered() {
        let lock = Mutex::new(());
        let _held = try_acquire_lock(&lock).expect("첫 획득");
        assert!(
            try_acquire_lock(&lock).is_none(),
            "실제로 쥐고 있는 동안은 거부(None) — 호출자가 '실행 중' 문구를 낸다"
        );
    }

    #[test]
    fn poisoned_lock_is_recovered_instead_of_rejecting_forever() {
        let lock = poisoned_lock();
        // 예전 `try_lock().map_err(|_| ...)`는 여기서 영구히 "실행 중"으로 거부했다.
        let guard = try_acquire_lock(&lock);
        assert!(guard.is_some(), "poison은 회복하고 진행해야 한다");
        assert!(!lock.is_poisoned(), "회복 뒤 poison 표식은 지워져야 한다");
        // 회복한 가드를 쥔 동안에는 진짜 WouldBlock으로 구분되어 거부된다.
        assert!(try_acquire_lock(&lock).is_none());
        drop(guard);
        assert!(try_acquire_lock(&lock).is_some());
    }

    #[test]
    fn busy_messages_are_distinct_per_feature() {
        assert_ne!(EXECUTION_BUSY_MESSAGE, PREVIEW_BUSY_MESSAGE);
        assert!(EXECUTION_BUSY_MESSAGE.contains("업데이트"));
        assert!(PREVIEW_BUSY_MESSAGE.contains("미리보기"));
    }
}
