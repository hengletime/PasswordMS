//! D-61 自动锁定档位持久化回归（A 案：闲置达档位即锁，档位存 `Vault` 明文头部）。
//!
//! 覆盖：①合法档位（1/5/15）写入后**重开库读回一致**（真实持久化，非仅内存）；
//! ②非法档位（0/2/999）拒绝 `ERR_VALIDATION`；③幂等（同值重复 set 不报错、不炸）。
//! 执行层（前端闲置计时 / Android 回前台补判）属前端纯逻辑层 `src/lib/auto-lock.ts`，
//! 由 vitest 假时钟覆盖，不在本套件范围。

use std::path::PathBuf;

use passwordms_core::error::CoreError;
use passwordms_core::session::Session;

const PWD: &str = "AutoLock#Master1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-autolock-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn set_auto_lock_persists_across_reopen() {
    for minutes in [1u16, 5u16, 15u16] {
        let dir = temp_dir("persist");
        let vault_path = dir.join("vault");

        // 建库：默认档位 5（constants::AUTO_LOCK_MINUTES）。
        {
            let mut session = Session::create(&vault_path, PWD, true).unwrap();
            assert_eq!(
                session.repo().unwrap().auto_lock_minutes(),
                5,
                "新建库默认档位应为 5"
            );
            session.repo_mut().unwrap().set_auto_lock_minutes(minutes).unwrap();
        }

        // drop 后重开：读回一致（持久化验证——只验内存的话 update_header 白写）。
        {
            let session = Session::unlock(&vault_path, PWD).unwrap();
            assert_eq!(
                session.repo().unwrap().auto_lock_minutes(),
                minutes,
                "档位 {minutes} 应在重开库后保持"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn set_auto_lock_rejects_values_off_the_ladder() {
    let dir = temp_dir("invalid");
    let vault_path = dir.join("vault");
    let mut session = Session::create(&vault_path, PWD, true).unwrap();

    for minutes in [0u16, 2u16, 999u16] {
        let err = session
            .repo_mut()
            .unwrap()
            .set_auto_lock_minutes(minutes)
            .unwrap_err();
        assert!(
            matches!(err, CoreError::Validation(_)),
            "档位 {minutes} 应报 Validation，实际 {err:?}"
        );
        assert_eq!(err.code(), "ERR_VALIDATION");
    }

    // 非法值不得污染头部：读回仍是默认 5。
    assert_eq!(session.repo().unwrap().auto_lock_minutes(), 5);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_auto_lock_is_idempotent_for_same_value() {
    let dir = temp_dir("idempotent");
    let vault_path = dir.join("vault");
    let mut session = Session::create(&vault_path, PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();

    repo.set_auto_lock_minutes(15).unwrap();
    // 同值重复 set：幂等 Ok（内部不重写头部）。
    repo.set_auto_lock_minutes(15).unwrap();
    repo.set_auto_lock_minutes(15).unwrap();
    assert_eq!(repo.auto_lock_minutes(), 15);

    // 幂等后仍可换档（1 → 15 已验证；此处验证 15 → 1 往返）。
    repo.set_auto_lock_minutes(1).unwrap();
    assert_eq!(repo.auto_lock_minutes(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
