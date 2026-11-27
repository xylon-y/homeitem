//! 标签与类型管理页面。

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use common::dto::{
    EntityTypeCreate, EntityTypeOut, EntityTypeUpdate, TagCreate, TagSummary, TagUpdate,
};

use crate::api;

/// 标签行的编辑 / 删除回调，经 context 传给递归渲染函数。
#[derive(Clone, Copy)]
struct TagCallbacks {
    edit: Callback<TagSummary, ()>,
    delete: Callback<String, ()>,
}

/// 递归渲染标签树的一行：缩进 + 标签信息 + 编辑/删除按钮。
fn render_tag(tag: &common::dto::TagOut, level: usize) -> AnyView {
    let summary = tag.summary.clone();
    let name = summary.name.clone();
    let name_for_confirm = summary.name.clone();
    let description = summary.description.clone();
    let icon = summary.icon.clone();
    let color = format!("background-color: {}", summary.color);
    let count = summary.item_count.unwrap_or(0);
    let indent = format!("margin-left: {}px", level * 20);
    let id = summary.id.clone();
    let children = tag.children.clone();
    let cb = expect_context::<TagCallbacks>();

    view! {
        <div class="tag-row" style=indent>
            <span class="tag-dot" style=color></span>
            <strong>{name}</strong>
            <span class="muted">{format!(" — {}", description)}</span>
            <span class="muted">{format!(" [图标: {}]", icon)}</span>
            <span class="muted">{format!("（{count} 件物品）")}</span>
            <button type="button" on:click=move |_| cb.edit.run(summary.clone())>"编辑"</button>
            <button type="button" on:click=move |_| {
                let msg = format!("确定删除标签「{name_for_confirm}」？");
                if web_sys::window().unwrap().confirm_with_message(&msg).unwrap_or(false) {
                    cb.delete.run(id.clone());
                }
            }>"删除"</button>
        </div>
        {children.iter().map(|child| render_tag(child, level + 1)).collect_view()}
    }
    .into_any()
}

/// 标签管理页：树形展示 + 新建/编辑表单。
#[component]
pub fn TagsPage() -> impl IntoView {
    let tree_resource = LocalResource::new(move || api::tags_tree());
    let flat_resource = LocalResource::new(move || api::tags());

    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let color = RwSignal::new("#3b82f6".to_string());
    let icon = RwSignal::new(String::new());
    let parent_id = RwSignal::new(String::new());
    let editing_id = RwSignal::new(Option::<String>::None);
    let error = RwSignal::new(String::new());

    let reset: Callback<(), ()> = Callback::new(move |_| {
        name.set(String::new());
        description.set(String::new());
        color.set("#3b82f6".to_string());
        icon.set(String::new());
        parent_id.set(String::new());
        editing_id.set(None);
        error.set(String::new());
    });

    let edit_tag: Callback<TagSummary, ()> = Callback::new(move |tag: TagSummary| {
        name.set(tag.name);
        description.set(tag.description);
        color.set(if tag.color.is_empty() {
            "#3b82f6".to_string()
        } else {
            tag.color
        });
        icon.set(tag.icon);
        parent_id.set(tag.parent_id.unwrap_or_default());
        editing_id.set(Some(tag.id));
        error.set(String::new());
    });

    let delete_tag = Callback::new(move |id: String| {
        spawn_local(async move {
            match api::delete_tag(&id).await {
                Ok(()) => {
                    tree_resource.refetch();
                    flat_resource.refetch();
                }
                Err(e) => error.set(e.message),
            }
        });
    });
    provide_context(TagCallbacks {
        edit: edit_tag,
        delete: delete_tag,
    });

    let submit = move |ev: ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get().trim().to_string();
        if n.is_empty() {
            error.set("名称不能为空".to_string());
            return;
        }
        let d = description.get();
        let c = color.get();
        let i = icon.get();
        let p = parent_id.get();
        let parent = if p.is_empty() { None } else { Some(p) };
        let id = editing_id.get();
        error.set(String::new());

        spawn_local(async move {
            let result = match id {
                Some(id) => api::update_tag(
                    &id,
                    &TagUpdate {
                        name: n,
                        description: d,
                        color: c,
                        icon: i,
                        parent_id: parent,
                    },
                )
                .await
                .map(|_| ()),
                None => api::create_tag(&TagCreate {
                    name: n,
                    description: d,
                    color: c,
                    icon: i,
                    parent_id: parent,
                })
                .await
                .map(|_| ()),
            };
            match result {
                Ok(()) => {
                    reset.run(());
                    tree_resource.refetch();
                    flat_resource.refetch();
                }
                Err(e) => error.set(e.message),
            }
        });
    };

    view! {
        <div class="page">
            <h1>"标签管理"</h1>
            <section class="card">
                <h2>{move || if editing_id.get().is_some() { "编辑标签" } else { "新建标签" }}</h2>
                <form class="form" on:submit=submit>
                    <label>
                        "名称"
                        <input type="text" required bind:value=name />
                    </label>
                    <label>
                        "描述"
                        <input type="text" bind:value=description />
                    </label>
                    <label>
                        "颜色"
                        <input type="color" bind:value=color />
                    </label>
                    <label>
                        "图标"
                        <input type="text" bind:value=icon />
                    </label>
                    <label>
                        "上级标签"
                        <Suspense fallback=|| view! { <select disabled><option>"加载中…"</option></select> }>
                            {move || Suspend::new(async move {
                                let options = match flat_resource.get() {
                                    Some(Ok(list)) => list,
                                    _ => Vec::new(),
                                };
                                view! {
                                    <select bind:value=parent_id>
                                        <option value="">"无上级"</option>
                                        {options.into_iter().map(|t| view! {
                                            <option value=t.id.clone()>{t.name}</option>
                                        }).collect_view()}
                                    </select>
                                }
                            })}
                        </Suspense>
                    </label>
                    <div class="form-actions">
                        <button type="submit">"保存"</button>
                        <button type="button" on:click=move |_| reset.run(())>"取消"</button>
                    </div>
                </form>
                {move || {
                    let msg = error.get();
                    (!msg.is_empty()).then(|| view! { <p class="error">{msg}</p> })
                }}
            </section>

            <section class="card">
                <h2>"标签列表"</h2>
                <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
                    {move || Suspend::new(async move {
                        match tree_resource.get() {
                            Some(Ok(tags)) if !tags.is_empty() => view! {
                                <div class="tag-tree">
                                    {tags.iter().map(|t| render_tag(t, 0)).collect_view()}
                                </div>
                            }.into_any(),
                            Some(Ok(_)) => view! { <p class="muted">"还没有标签。"</p> }.into_any(),
                            Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
                            None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                        }
                    })}
                </Suspense>
            </section>
        </div>
    }
}

/// 物品类型管理页：表格展示 + 新建/编辑表单。
#[component]
pub fn TypesPage() -> impl IntoView {
    let types_resource = LocalResource::new(move || api::entity_types());

    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let icon = RwSignal::new(String::new());
    let is_location = RwSignal::new(false);
    let editing_id = RwSignal::new(Option::<String>::None);
    let error = RwSignal::new(String::new());

    let reset: Callback<(), ()> = Callback::new(move |_| {
        name.set(String::new());
        description.set(String::new());
        icon.set(String::new());
        is_location.set(false);
        editing_id.set(None);
        error.set(String::new());
    });

    let edit_type: Callback<EntityTypeOut, ()> = Callback::new(move |t: EntityTypeOut| {
        name.set(t.name);
        description.set(t.description);
        icon.set(t.icon);
        is_location.set(t.is_location);
        editing_id.set(Some(t.id));
        error.set(String::new());
    });

    let delete_type = Callback::new(move |id: String| {
        spawn_local(async move {
            match api::delete_type(&id).await {
                Ok(()) => types_resource.refetch(),
                Err(e) => error.set(e.message),
            }
        });
    });

    let submit = move |ev: ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get().trim().to_string();
        if n.is_empty() {
            error.set("名称不能为空".to_string());
            return;
        }
        let d = description.get();
        let i = icon.get();
        let loc = is_location.get();
        let id = editing_id.get();
        error.set(String::new());

        spawn_local(async move {
            let result = match id {
                Some(id) => api::update_type(
                    &id,
                    &EntityTypeUpdate {
                        name: n,
                        description: d,
                        icon: i,
                        is_location: loc,
                    },
                )
                .await
                .map(|_| ()),
                None => api::create_type(&EntityTypeCreate {
                    name: n,
                    description: d,
                    icon: i,
                    is_location: loc,
                })
                .await
                .map(|_| ()),
            };
            match result {
                Ok(()) => {
                    reset.run(());
                    types_resource.refetch();
                }
                Err(e) => error.set(e.message),
            }
        });
    };

    view! {
        <div class="page">
            <h1>"物品类型管理"</h1>
            <section class="card">
                <h2>{move || if editing_id.get().is_some() { "编辑类型" } else { "新建类型" }}</h2>
                <form class="form" on:submit=submit>
                    <label>
                        "名称"
                        <input type="text" required bind:value=name />
                    </label>
                    <label>
                        "描述"
                        <input type="text" bind:value=description />
                    </label>
                    <label>
                        "图标"
                        <input type="text" bind:value=icon />
                    </label>
                    <label>
                        <input type="checkbox" bind:checked=is_location />
                        "位置类型（取消勾选则为物品类型）"
                    </label>
                    <div class="form-actions">
                        <button type="submit">"保存"</button>
                        <button type="button" on:click=move |_| reset.run(())>"取消"</button>
                    </div>
                </form>
                {move || {
                    let msg = error.get();
                    (!msg.is_empty()).then(|| view! { <p class="error">{msg}</p> })
                }}
            </section>

            <section class="card">
                <h2>"类型列表"</h2>
                <Suspense fallback=|| view! { <p class="muted">"加载中…"</p> }>
                    {move || Suspend::new(async move {
                        match types_resource.get() {
                            Some(Ok(types)) if !types.is_empty() => view! {
                                <table class="types-table">
                                    <thead>
                                        <tr>
                                            <th>"名称"</th>
                                            <th>"描述"</th>
                                            <th>"图标"</th>
                                            <th>"类别"</th>
                                            <th>"操作"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {types.into_iter().map(|t| {
                                            let edit = t.clone();
                                            let kind = if t.is_location { "位置类型" } else { "物品类型" };
                                            let delete_id = t.id.clone();
                                            view! {
                                                <tr>
                                                    <td>{t.name}</td>
                                                    <td>{t.description}</td>
                                                    <td>{t.icon}</td>
                                                    <td>{kind}</td>
                                                    <td>
                                                        <button type="button" on:click=move |_| edit_type.run(edit.clone())>"编辑"</button>
                                                        <button type="button" on:click=move |_| {
                                                            let msg = "确定删除该类型？仍有物品使用的类型无法删除。";
                                                            if web_sys::window().unwrap().confirm_with_message(msg).unwrap_or(false) {
                                                                delete_type.run(delete_id.clone());
                                                            }
                                                        }>"删除"</button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any(),
                            Some(Ok(_)) => view! { <p class="muted">"还没有类型。"</p> }.into_any(),
                            Some(Err(e)) => view! { <p class="error">{e.message}</p> }.into_any(),
                            None => view! { <p class="muted">"加载中…"</p> }.into_any(),
                        }
                    })}
                </Suspense>
            </section>
        </div>
    }
}
