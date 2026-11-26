//! 登录页。

use leptos::prelude::*;
use leptos::task::spawn_local;

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
                    "用户名"
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
        </div>
    }
}
