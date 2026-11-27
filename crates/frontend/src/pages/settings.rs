//! 设置页：个人信息、组成员与邀请、组设置。

use common::GroupRole;
use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

type Ts = chrono::DateTime<chrono::Utc>;

fn role_label(role: GroupRole) -> &'static str {
    match role {
        GroupRole::Owner => "所有者",
        GroupRole::User => "普通成员",
    }
}

fn fmt_ts(ts: Option<Ts>) -> String {
    ts.map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".to_string())
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    // ---- 区块 1：个人信息 ----
    let user = LocalResource::new(move || api::me());
    let edit_name = RwSignal::new(String::new());
    let edit_email = RwSignal::new(String::new());
    let current_password = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let profile_error = RwSignal::new(String::new());
    let profile_notice = RwSignal::new(String::new());
    let profile_ready = RwSignal::new(false);
    Effect::new(move |_| {
        if !profile_ready.get() {
            if let Some(Ok(u)) = user.get() {
                edit_name.set(u.name.clone());
                edit_email.set(u.email.clone());
                profile_ready.set(true);
            }
        }
    });
    // ---- 区块 2：组成员与邀请 ----
    let members = LocalResource::new(move || api::members());
    let invitations = LocalResource::new(move || api::invitations());
    let member_error = RwSignal::new(String::new());
    let invite_error = RwSignal::new(String::new());
    let invite_notice = RwSignal::new(String::new());
    let invite_role = RwSignal::new("user".to_string());
    let invite_email = RwSignal::new(String::new());
    let invite_link = RwSignal::new(String::new());

    // ---- 区块 3：组设置 ----
    let group = LocalResource::new(move || api::group());
    let group_name = RwSignal::new(String::new());
    let group_currency = RwSignal::new(String::new());
    let group_error = RwSignal::new(String::new());
    let group_notice = RwSignal::new(String::new());
    let group_ready = RwSignal::new(false);
    Effect::new(move |_| {
        if !group_ready.get() {
            if let Some(Ok(g)) = group.get() {
                group_name.set(g.name.clone());
                group_currency.set(g.currency.clone());
                group_ready.set(true);
            }
        }
    });

    view! {
        <div class="page">
            <h1>"设置"</h1>

            <h2>"个人信息"</h2>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match user.get() {
                        Some(Ok(u)) => view! {
                            <div class="stat-card">
                                <p><strong>{u.name.clone()}</strong>" · "{u.email.clone()}</p>
                                <p class="muted">
                                    {format!("角色：{} · 组：{}", role_label(u.role), u.group_name)}
                                </p>
                            </div>
                        }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error">{format!("加载失败：{}", e.message)}</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>
            <form on:submit=move |ev: ev::SubmitEvent| {
                ev.prevent_default();
                let n = edit_name.get();
                let e = edit_email.get();
                spawn_local(async move {
                    match api::update_me(Some(&n), Some(&e)).await {
                        Ok(_) => {
                            profile_error.set(String::new());
                            profile_notice.set("个人信息已保存".to_string());
                            user.refetch();
                        }
                        Err(err) => {
                            profile_error.set(err.message);
                            profile_notice.set(String::new());
                        }
                    }
                });
            }>
                <label class="field">
                    <span>"姓名"</span>
                    <input type="text" bind:value=edit_name required />
                </label>
                <label class="field">
                    <span>"邮箱"</span>
                    <input type="email" bind:value=edit_email required />
                </label>
                <div class="form-actions">
                    <button type="submit">"保存个人信息"</button>
                </div>
            </form>
            <h3>"修改密码"</h3>
            <form on:submit=move |ev: ev::SubmitEvent| {
                ev.prevent_default();
                let cur = current_password.get();
                let new = new_password.get();
                spawn_local(async move {
                    match api::change_password(&cur, &new).await {
                        Ok(()) => {
                            profile_error.set(String::new());
                            profile_notice.set("密码已修改，请重新登录".to_string());
                            current_password.set(String::new());
                            new_password.set(String::new());
                            api::logout();
                            if let Some(w) = web_sys::window() {
                                let _ = w.location().set_href("/login");
                            }
                        }
                        Err(err) => {
                            profile_error.set(err.message);
                            profile_notice.set(String::new());
                        }
                    }
                });
            }>
                <label class="field">
                    <span>"当前密码"</span>
                    <input type="password" bind:value=current_password required />
                </label>
                <label class="field">
                    <span>"新密码"</span>
                    <input type="password" bind:value=new_password required minlength="8" />
                </label>
                <div class="form-actions">
                    <button type="submit">"修改密码"</button>
                </div>
            </form>
            <Show when={move || !profile_error.get().is_empty()}>
                <p class="error">{profile_error}</p>
            </Show>
            <Show when={move || !profile_notice.get().is_empty()}>
                <p class="muted">{profile_notice}</p>
            </Show>

            <h2>"组成员与邀请"</h2>
            <h3>"组成员"</h3>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match members.get() {
                        Some(Ok(items)) if !items.is_empty() => {
                            let owner = matches!(user.get(), Some(Ok(u)) if u.role == GroupRole::Owner);
                            view! {
                            <table class="items-table">
                                <thead>
                                    <tr>
                                        <th>"姓名"</th>
                                        <th>"邮箱"</th>
                                        <th>"角色"</th>
                                        <th>"激活时间"</th>
                                        <th>"操作"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|m| {
                                        let role_id = m.user_id.clone();
                                        let remove_id = m.user_id.clone();
                                        let remove_name = m.name.clone();
                                        let role = m.role;
                                        view! {
                                            <tr>
                                                <td>{m.name.clone()}</td>
                                                <td>{m.email.clone()}</td>
                                                <td>{role_label(m.role)}</td>
                                                <td>{fmt_ts(m.activated_on)}</td>
                                                <td class="actions">
                                                    {owner.then(|| view! {
                                                        <select on:change=move |ev| {
                                                            let v = event_target_value(&ev);
                                                            let new_role = if v == "owner" {
                                                                GroupRole::Owner
                                                            } else {
                                                                GroupRole::User
                                                            };
                                                            let id = role_id.clone();
                                                            spawn_local(async move {
                                                                match api::set_member_role(&id, new_role).await {
                                                                    Ok(()) => {
                                                                        member_error.set(String::new());
                                                                        members.refetch();
                                                                        user.refetch();
                                                                    }
                                                                    Err(e) => member_error.set(e.message),
                                                                }
                                                            });
                                                        }>
                                                            <option value="user" prop:selected=move || role == GroupRole::User>"普通成员"</option>
                                                            <option value="owner" prop:selected=move || role == GroupRole::Owner>"所有者"</option>
                                                        </select>
                                                        <button type="button" class="btn-danger" on:click=move |_| {
                                                            let msg = format!("确定移除成员「{remove_name}」？");
                                                            if web_sys::window()
                                                                .and_then(|w| w.confirm_with_message(&msg).ok())
                                                                .unwrap_or(false)
                                                            {
                                                                let id = remove_id.clone();
                                                                spawn_local(async move {
                                                                    match api::remove_member(&id).await {
                                                                        Ok(()) => {
                                                                            member_error.set(String::new());
                                                                            members.refetch();
                                                                        }
                                                                        Err(e) => member_error.set(e.message),
                                                                    }
                                                                });
                                                            }
                                                        }>"移除"</button>
                                                    })}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            }.into_any()
                        }
                        Some(Ok(_)) => view! { <p class="muted">"还没有组成员。"</p> }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error">{format!("加载失败：{}", e.message)}</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>
            <Show when={move || !member_error.get().is_empty()}>
                <p class="error">{member_error}</p>
            </Show>

            <h3>"邀请列表"</h3>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match invitations.get() {
                        Some(Ok(items)) if !items.is_empty() => {
                            let owner = matches!(user.get(), Some(Ok(u)) if u.role == GroupRole::Owner);
                            view! {
                            <table class="items-table">
                                <thead>
                                    <tr>
                                        <th>"邀请令牌"</th>
                                        <th>"角色"</th>
                                        <th>"邮箱"</th>
                                        <th>"过期时间"</th>
                                        <th>"操作"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {items.into_iter().map(|inv| {
                                        let delete_id = inv.id.clone();
                                        let token = inv.token.clone();
                                        view! {
                                            <tr>
                                                <td><input type="text" readonly value=token /></td>
                                                <td>{role_label(inv.role)}</td>
                                                <td>{inv.email.clone().unwrap_or_default()}</td>
                                                <td>{fmt_ts(inv.expires_at)}</td>
                                                <td class="actions">
                                                    {owner.then(|| view! {
                                                        <button type="button" class="btn-danger" on:click=move |_| {
                                                            let id = delete_id.clone();
                                                            spawn_local(async move {
                                                                match api::delete_invitation(&id).await {
                                                                    Ok(()) => {
                                                                        invite_error.set(String::new());
                                                                        invitations.refetch();
                                                                    }
                                                                    Err(e) => invite_error.set(e.message),
                                                                }
                                                            });
                                                        }>"删除"</button>
                                                    })}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            }.into_any()
                        }
                        Some(Ok(_)) => view! { <p class="muted">"还没有邀请。"</p> }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error">{format!("加载失败：{}", e.message)}</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>

            <h3>"新建邀请"</h3>
            <form on:submit=move |ev: ev::SubmitEvent| {
                ev.prevent_default();
                let role = match invite_role.get().as_str() {
                    "owner" => GroupRole::Owner,
                    _ => GroupRole::User,
                };
                let raw_email = invite_email.get();
                let email = if raw_email.trim().is_empty() {
                    None
                } else {
                    Some(raw_email.trim().to_string())
                };
                let origin = web_sys::window()
                    .and_then(|w| w.location().origin().ok())
                    .unwrap_or_default();
                spawn_local(async move {
                    match api::create_invitation(role, email.as_deref()).await {
                        Ok(inv) => {
                            invite_error.set(String::new());
                            invite_notice.set("邀请已创建".to_string());
                            invite_link.set(format!(
                                "{}/register?token={}",
                                origin.trim_end_matches('/'),
                                inv.token
                            ));
                            invitations.refetch();
                        }
                        Err(e) => {
                            invite_error.set(e.message);
                            invite_link.set(String::new());
                        }
                    }
                });
            }>
                <label class="field">
                    <span>"角色"</span>
                    <select bind:value=invite_role>
                        <option value="user">"普通成员"</option>
                        <option value="owner">"所有者"</option>
                    </select>
                </label>
                <label class="field">
                    <span>"邮箱（可选）"</span>
                    <input type="email" bind:value=invite_email />
                </label>
                <div class="form-actions">
                    <button type="submit">"创建邀请"</button>
                </div>
            </form>
            <Show when={move || !invite_error.get().is_empty()}>
                <p class="error">{invite_error}</p>
            </Show>
            <Show when={move || !invite_notice.get().is_empty()}>
                <p class="muted">{invite_notice}</p>
            </Show>
            <Show when={move || !invite_link.get().is_empty()}>
                <label class="field">
                    <span>"完整邀请链接（可复制）"</span>
                    <input type="text" readonly value=move || invite_link.get() />
                </label>
            </Show>

            <h2>"组设置"</h2>
            <Suspense fallback={|| view! { <p class="muted">"加载中…"</p> }}>
                {move || Suspend::new(async move {
                    match group.get() {
                        Some(Ok(g)) => view! {
                            <div class="stat-card">
                                <p><strong>{g.name.clone()}</strong></p>
                                <p class="muted">{format!("货币：{}", g.currency)}</p>
                            </div>
                        }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error">{format!("加载失败：{}", e.message)}</p>
                        }.into_any(),
                        None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                    }
                })}
            </Suspense>
            <form on:submit=move |ev: ev::SubmitEvent| {
                ev.prevent_default();
                let n = group_name.get();
                let c = group_currency.get();
                spawn_local(async move {
                    match api::update_group(&n, &c).await {
                        Ok(_) => {
                            group_error.set(String::new());
                            group_notice.set("组设置已保存".to_string());
                            group.refetch();
                        }
                        Err(e) => {
                            group_error.set(e.message);
                            group_notice.set(String::new());
                        }
                    }
                });
            }>
                <label class="field">
                    <span>"组名称"</span>
                    <input type="text" bind:value=group_name required />
                </label>
                <label class="field">
                    <span>"货币代码"</span>
                    <input type="text" bind:value=group_currency required />
                </label>
                <div class="form-actions">
                    <button type="submit">"保存组设置"</button>
                </div>
            </form>
            <Show when={move || !group_error.get().is_empty()}>
                <p class="error">{group_error}</p>
            </Show>
            <Show when={move || !group_notice.get().is_empty()}>
                <p class="muted">{group_notice}</p>
            </Show>
            <p><a href="/notifiers">"管理通知器"</a></p>
        </div>
    }
}
