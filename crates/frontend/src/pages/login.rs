//! 登录页与注册页。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

use crate::api;

/// 登录表单。
#[component]
pub fn LoginPage() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let stay = RwSignal::new(true);
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(String::new());
        let (u, p, s) = (username.get(), password.get(), stay.get());
        spawn_local(async move {
            match api::login(&u, &p, s).await {
                Ok(_) => {
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href("/");
                    }
                }
                Err(e) => {
                    error.set(e.message);
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="auth-card">
            <h1>"登录 homeitem"</h1>
            <form on:submit=on_submit>
                <label>
                    "用户名或邮箱"
                    <input type="text" bind:value=username placeholder="用户名或邮箱" required />
                </label>
                <label>
                    "密码"
                    <input type="password" bind:value=password required />
                </label>
                <label class="check">
                    <input type="checkbox" bind:checked=stay />
                    "保持登录 30 天"
                </label>
                <Show when={move || !error.get().is_empty()}>
                    <p class="error">{error}</p>
                </Show>
                <button type="submit" disabled={move || busy.get()}>
                    {move || if busy.get() { "登录中…" } else { "登录" }}
                </button>
            </form>
            <p class="muted">
                <a href="/register">"使用邀请码注册"</a>
                " · "
                <a href="/setup">"首次初始化"</a>
            </p>
        </div>
    }
}

/// 使用邀请令牌注册。
#[component]
pub fn RegisterPage() -> impl IntoView {
    let query = use_query_map();
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let invite_token = RwSignal::new(query.get().get("token").unwrap_or_default());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(String::new());
        let (n, e, p, t) = (
            name.get(),
            email.get(),
            password.get(),
            invite_token.get(),
        );
        spawn_local(async move {
            match api::register(&n, &e, &p, &t).await {
                Ok(_) => {
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href("/");
                    }
                }
                Err(err) => {
                    error.set(err.message);
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="auth-card">
            <h1>"注册 homeitem"</h1>
            <p class="muted">"需要组所有者提供的邀请令牌。"</p>
            <form on:submit=on_submit>
                <label>
                    "姓名"
                    <input type="text" bind:value=name required />
                </label>
                <label>
                    "邮箱"
                    <input type="email" bind:value=email required />
                </label>
                <label>
                    "密码"
                    <input type="password" bind:value=password required minlength="8" />
                </label>
                <label>
                    "邀请令牌"
                    <input type="text" bind:value=invite_token required />
                </label>
                <Show when={move || !error.get().is_empty()}>
                    <p class="error">{error}</p>
                </Show>
                <button type="submit" disabled={move || busy.get()}>
                    {move || if busy.get() { "注册中…" } else { "注册" }}
                </button>
            </form>
            <p class="muted">
                <a href="/">"返回登录"</a>
            </p>
        </div>
    }
}
