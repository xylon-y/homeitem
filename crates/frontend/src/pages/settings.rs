//! 设置页：组信息、成员、邀请。

use leptos::prelude::*;

/// 设置占位页（组管理接口已就绪，界面后续迭代）。
#[component]
pub fn SettingsPage() -> impl IntoView {
    view! {
        <div class="page">
            <h1>"设置"</h1>
            <p class="muted">"组信息、成员管理、邀请令牌与通知器配置。"</p>
            <ul class="muted">
                <li>"组/成员管理: GET|PUT /api/v1/group, /group/members"</li>
                <li>"邀请: POST /api/v1/group/invitations"</li>
                <li>"修改密码: PUT /api/v1/user/password"</li>
            </ul>
        </div>
    }
}
