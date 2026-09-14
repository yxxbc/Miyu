//! 任务接口的账号维度收口(docs/plan/2026-09-14/job-control.md §A)。
//!
//! 后台任务注册表是进程级一份,`jobs::job_visible` 只按「调用方所在会话」判——HTTP
//! 请求不在任何会话里,落进它的「全局可见」分支,于是任何登录账号(包括成员)都能
//! 列出、读日志、停掉所有人的任务。这里补上账号这一维,口径与事件流的
//! `EventOwnerFilter` 相同:会话在谁的库里就是谁的。
//!
//! 看不到的任务一律回 404 而不是 403:403 等于告诉成员「这个 id 存在」。

use crate::web::*;

/// 纯判定。`session_owner` 是任务所属会话的归属键(管理员 = 空串,成员 = 账号 id);
/// 任务没有会话、或会话查不到归属时为 None。
pub(in crate::web) fn job_allowed(admin: bool, owner_key: &str, session_owner: Option<&str>) -> bool {
    if admin {
        return true;
    }
    // 成员只认自己名下的会话。拿不准(无会话 / 查不到归属)一律不可见——
    // 这是权限判定,不是显示偏好。
    session_owner.is_some_and(|owner| !owner_key.is_empty() && owner == owner_key)
}

pub(in crate::web) fn job_visible_to(
    state: &DaemonState,
    identity: &WebIdentity,
    session_id: Option<&str>,
) -> bool {
    if identity.admin {
        return true;
    }
    let owner = session_id.and_then(|session_id| state.stores.owner_of_session(session_id));
    job_allowed(false, identity.owner_key(), owner.as_deref())
}

/// 单个任务的访问闸:日志、子过程、停止共用。
pub(in crate::web) fn require_job_access(
    state: &DaemonState,
    identity: &WebIdentity,
    job_id: &str,
) -> std::result::Result<(), ApiError> {
    let session_id = tools::jobs::job_session_id(job_id);
    if job_visible_to(state, identity, session_id.as_deref()) {
        Ok(())
    } else {
        Err(ApiError::new(StatusCode::NOT_FOUND, "job not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::job_allowed;

    #[test]
    fn members_only_reach_jobs_in_their_own_sessions() {
        // 管理员:谁的都行,包括没有会话的旧任务。
        assert!(job_allowed(true, "", Some("")));
        assert!(job_allowed(true, "", Some("member-a")));
        assert!(job_allowed(true, "", None));
        // 成员:自己名下的会话。
        assert!(job_allowed(false, "member-a", Some("member-a")));
        // 管理员的会话、别的成员的会话:修前 HTTP 一侧全部可见。
        assert!(!job_allowed(false, "member-a", Some("")));
        assert!(!job_allowed(false, "member-a", Some("member-b")));
        // 没有会话 / 查不到归属:不可见。
        assert!(!job_allowed(false, "member-a", None));
        // 成员归属键理论上不为空;万一为空,也不能借此匹配上管理员的会话。
        assert!(!job_allowed(false, "", Some("")));
    }
}
