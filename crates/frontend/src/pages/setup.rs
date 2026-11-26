//! 首次运行初始化页。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;

/// 创建管理员账号 + 默认组。
#[component]
pub fn SetupPage() -> impl IntoView {
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        let (n, e, p) = (name.get(), email.get(), password.get());
        spawn_local(async move {
            match api::setup(&n, &e, &p).await {
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
            <h1>"初始化 homeitem"</h1>
            <p class="muted">"创建第一个管理员账号，开始管理你的物品。"</p>
            <form on:submit=on_submit>
                <label>
                    "昵称"
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
                <Show when={move || !error.get().is_empty()}>
                    <p class="error">{error}</p>
                </Show>
                <button type="submit" disabled={move || busy.get()}>
                    {move || if busy.get() { "创建中…" } else { "创建并进入" }}
                </button>
            </form>
        </div>
    }
}
