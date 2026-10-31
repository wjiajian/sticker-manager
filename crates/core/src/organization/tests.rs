use super::*;
use crate::LibraryView;

fn library(root: &std::path::Path) -> Library {
    let library = Library::open(root).unwrap();
    library
        .conn
        .execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at) VALUES
            ('one', 'hash1', 'png', '收到', 1, 1, 1, 1),
            ('two', 'hash2', 'png', '晚安', 1, 1, 1, 2),
            ('deleted', 'hash3', 'png', '无语', 1, 1, 1, 3);",
        )
        .unwrap();
    library
}

#[test]
fn creates_renames_recolors_moves_and_deletes_tags_and_groups() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let first = library.create_tag_group("  情绪  ").unwrap();
    let second = library.create_tag_group("场景").unwrap();
    assert_eq!(first.name, "情绪");
    assert_eq!((first.position, second.position), (0, 1));
    let tag = library
        .create_tag("  开心  ", Some(TAG_COLORS[2]), Some(&first.id))
        .unwrap();
    assert_eq!(
        (
            tag.name.as_str(),
            tag.color.as_str(),
            tag.group_id.as_deref()
        ),
        ("开心", TAG_COLORS[2], Some(first.id.as_str()))
    );
    library.rename_tag(&tag.id, "  Happy ").unwrap();
    library.set_tag_color(&tag.id, "#3e7bfa").unwrap();
    library.move_tag(&tag.id, Some(&second.id)).unwrap();
    library.rename_tag_group(&second.id, "  日常  ").unwrap();
    let sidebar = library.tag_sidebar().unwrap();
    assert!(sidebar.groups[0].tags.is_empty());
    assert_eq!(sidebar.groups[1].name, "日常");
    let moved = &sidebar.groups[1].tags[0].tag;
    assert_eq!(
        (moved.name.as_str(), moved.color.as_str(), moved.position),
        ("Happy", TAG_COLORS[3], tag.position)
    );
    library.move_tag(&tag.id, None).unwrap();
    assert_eq!(library.tag_sidebar().unwrap().ungrouped[0].tag.id, tag.id);
    library.delete_tag(&tag.id).unwrap();
    library.delete_tag_group(&first.id).unwrap();
    library.delete_tag_group(&second.id).unwrap();
    let empty = library.tag_sidebar().unwrap();
    assert!(empty.groups.is_empty() && empty.ungrouped.is_empty());
}

#[test]
fn rejects_empty_and_duplicate_names_for_creation_and_rename() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let tag = library.create_tag(" Hello ", None, None).unwrap();
    let group = library.create_tag_group(" Hello ").unwrap();
    let other_tag = library.create_tag("World", None, Some(&group.id)).unwrap();
    let other_group = library.create_tag_group("World").unwrap();
    for name in ["", " ", "\t\n", "\u{3000}"] {
        assert!(matches!(
            library.create_tag(name, None, None),
            Err(Error::EmptyName("标签"))
        ));
        assert!(matches!(
            library.rename_tag(&tag.id, name),
            Err(Error::EmptyName("标签"))
        ));
        assert!(matches!(
            library.create_tag_group(name),
            Err(Error::EmptyName("标签分组"))
        ));
        assert!(matches!(
            library.rename_tag_group(&group.id, name),
            Err(Error::EmptyName("标签分组"))
        ));
    }
    for name in ["Hello", "hello", "HELLO", " Hello ", "\u{3000}hElLo\t"] {
        assert!(matches!(
            library.create_tag(name, None, Some(&other_group.id)),
            Err(Error::DuplicateName("标签"))
        ));
        assert!(matches!(
            library.rename_tag(&other_tag.id, name),
            Err(Error::DuplicateName("标签"))
        ));
        assert!(matches!(
            library.create_tag_group(name),
            Err(Error::DuplicateName("标签分组"))
        ));
        assert!(matches!(
            library.rename_tag_group(&other_group.id, name),
            Err(Error::DuplicateName("标签分组"))
        ));
    }
    // 自身改大小写合法；标签和标签分组分别检查唯一性。
    library.rename_tag(&tag.id, " HELLO ").unwrap();
    library.rename_tag_group(&group.id, " hello ").unwrap();
    let sidebar = library.tag_sidebar().unwrap();
    assert_eq!(sidebar.ungrouped[0].tag.name, "HELLO");
    assert_eq!(sidebar.groups[0].name, "hello");
    assert_eq!(sidebar.groups[0].tags[0].tag.name, "World");
    assert_eq!(sidebar.groups[1].name, "World");
    library.create_tag("今天' OR 1=1 --", None, None).unwrap();
}

#[test]
fn assigns_only_palette_colors_in_order_and_wraps() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    for i in 0..10 {
        let tag = library.create_tag(&format!("标签{i}"), None, None).unwrap();
        assert_eq!(tag.color, TAG_COLORS[i % TAG_COLORS.len()]);
        assert_eq!(tag.position, i as i64);
    }
    let explicit = library
        .create_tag("指定颜色", Some(TAG_COLORS[7]), None)
        .unwrap();
    assert_eq!(explicit.color, TAG_COLORS[7]);
    for color in ["", "red", "#FFFFFF", "#F49AC2"] {
        assert!(matches!(
            library.create_tag("无效颜色", Some(color), None),
            Err(Error::InvalidTagColor)
        ));
        assert!(matches!(
            library.set_tag_color(&explicit.id, color),
            Err(Error::InvalidTagColor)
        ));
    }
    assert_eq!(library.tag_sidebar().unwrap().ungrouped.len(), 11);
}

#[test]
fn adding_is_idempotent_and_removing_last_tag_updates_untagged() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let one = library.create_tag("开心", None, None).unwrap();
    let two = library.create_tag("回复", None, None).unwrap();
    library.add_sticker_tag("one", &one.id).unwrap();
    library.add_sticker_tag("one", &one.id).unwrap();
    assert_eq!(library.sticker("one").unwrap().tags.len(), 1);
    assert_eq!(library.tag_sidebar().unwrap().ungrouped[0].sticker_count, 1);
    library.add_sticker_tag("one", &two.id).unwrap();
    assert_eq!(
        library.sticker("one").unwrap().tags,
        [one.clone(), two.clone()]
    );
    library.remove_sticker_tag("one", &one.id).unwrap();
    library.remove_sticker_tag("one", &one.id).unwrap();
    assert_eq!(library.overview().unwrap().untagged, 2);
    library.remove_sticker_tag("one", &two.id).unwrap();
    assert_eq!(library.overview().unwrap().untagged, 3);
    assert_eq!(
        library.list_stickers(LibraryView::Untagged).unwrap().len(),
        3
    );
}

#[test]
fn deleting_tag_removes_all_associations_and_restores_untagged() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let tag = library.create_tag("开心", None, None).unwrap();
    for id in ["one", "two", "deleted"] {
        library.add_sticker_tag(id, &tag.id).unwrap();
    }
    library
        .conn
        .execute(
            "UPDATE stickers SET deleted_at = 10 WHERE id = 'deleted'",
            [],
        )
        .unwrap();
    assert_eq!(library.overview().unwrap().untagged, 0);
    library.delete_tag(&tag.id).unwrap();
    assert_eq!(library.overview().unwrap().untagged, 2);
    assert_eq!(
        library.list_stickers(LibraryView::Untagged).unwrap().len(),
        2
    );
    for id in ["one", "two", "deleted"] {
        assert!(library.sticker(id).unwrap().tags.is_empty());
    }
    assert!(
        library
            .list_stickers(LibraryView::Tag(tag.id))
            .unwrap()
            .is_empty()
    );
    assert!(library.tag_sidebar().unwrap().ungrouped.is_empty());
}

#[test]
fn deleting_group_preserves_tags_and_their_sticker_associations() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let group = library.create_tag_group("情绪").unwrap();
    let one = library.create_tag("开心", None, Some(&group.id)).unwrap();
    let two = library.create_tag("无语", None, Some(&group.id)).unwrap();
    library.add_sticker_tag("one", &one.id).unwrap();
    library.add_sticker_tag("one", &two.id).unwrap();
    library.delete_tag_group(&group.id).unwrap();
    let sidebar = library.tag_sidebar().unwrap();
    assert!(sidebar.groups.is_empty());
    assert_eq!(sidebar.ungrouped.len(), 2);
    assert!(
        sidebar
            .ungrouped
            .iter()
            .all(|tag| tag.tag.group_id.is_none() && tag.sticker_count == 1)
    );
    assert_eq!(library.sticker("one").unwrap().tags.len(), 2);
    assert_eq!(library.overview().unwrap().untagged, 2);
}

#[test]
fn sidebar_orders_by_position_includes_empty_groups_and_excludes_deleted_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let first = library.create_tag_group("后显示").unwrap();
    let second = library.create_tag_group("先显示").unwrap();
    let tag = library.create_tag("共享", None, Some(&first.id)).unwrap();
    let ungrouped = library.create_tag("未使用", None, None).unwrap();
    let another = library.create_tag("较早", None, Some(&first.id)).unwrap();
    for id in ["one", "two", "deleted"] {
        library.add_sticker_tag(id, &tag.id).unwrap();
    }
    library
        .conn
        .execute(
            "UPDATE stickers SET deleted_at = 10 WHERE id = 'deleted'",
            [],
        )
        .unwrap();
    library
        .conn
        .execute(
            "UPDATE tag_groups SET position = 3 WHERE id = ?1",
            [&first.id],
        )
        .unwrap();
    library
        .conn
        .execute("UPDATE tags SET position = -1 WHERE id = ?1", [&another.id])
        .unwrap();
    let sidebar = library.tag_sidebar().unwrap();
    assert_eq!(sidebar.groups[0].id, second.id);
    assert!(sidebar.groups[0].tags.is_empty());
    assert_eq!(sidebar.groups[1].tags[0].tag.id, another.id);
    assert_eq!(sidebar.groups[1].tags[1].sticker_count, 2);
    assert_eq!(sidebar.groups[1].tags[1].total_sticker_count, 3);
    assert_eq!(
        (
            sidebar.ungrouped[0].tag.id.as_str(),
            sidebar.ungrouped[0].sticker_count
        ),
        (ungrouped.id.as_str(), 0)
    );
}

#[test]
fn tag_view_lists_only_active_matches_with_all_tags_and_caption() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let tag = library.create_tag("回复", None, None).unwrap();
    let other = library.create_tag("开心", None, None).unwrap();
    for id in ["one", "two", "deleted"] {
        library.add_sticker_tag(id, &tag.id).unwrap();
    }
    library.add_sticker_tag("two", &other.id).unwrap();
    library.set_sticker_caption("two", "  收到  ").unwrap();
    library
        .conn
        .execute(
            "UPDATE stickers SET deleted_at = 10 WHERE id = 'deleted'",
            [],
        )
        .unwrap();
    let stickers = library
        .list_stickers(LibraryView::Tag(tag.id.clone()))
        .unwrap();
    assert_eq!(
        stickers
            .iter()
            .map(|sticker| sticker.id.as_str())
            .collect::<Vec<_>>(),
        ["two", "one"]
    );
    assert_eq!(stickers[0].tags, [tag, other]);
    assert_eq!(stickers[0].caption.as_deref(), Some("收到"));
    assert_eq!(stickers[0].tags, library.sticker("two").unwrap().tags);
    assert!(
        library
            .list_stickers(LibraryView::Tag("' OR 1=1 --".into()))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn caption_can_be_changed_cleared_and_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let library = library(dir.path());
    assert_eq!(library.sticker("one").unwrap().caption, None);
    library
        .set_sticker_caption("one", "\u{3000} 收到 \n")
        .unwrap();
    assert_eq!(
        library.sticker("one").unwrap().caption.as_deref(),
        Some("收到")
    );
    library.set_sticker_caption("one", " OK ").unwrap();
    drop(library);
    let library = Library::open(dir.path()).unwrap();
    assert_eq!(
        library.sticker("one").unwrap().caption.as_deref(),
        Some("OK")
    );
    for empty in ["", " \t\n\u{3000}"] {
        library.set_sticker_caption("one", empty).unwrap();
        assert_eq!(library.sticker("one").unwrap().caption, None);
        let is_null: bool = library
            .conn
            .query_row(
                "SELECT caption IS NULL FROM stickers WHERE id = 'one'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(is_null);
    }
    assert_eq!(library.sticker("one").unwrap().usage_count, 0);
}

#[test]
fn deleted_and_missing_stickers_reject_metadata_edits_without_partial_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let tag = library.create_tag("开心", None, None).unwrap();
    library.add_sticker_tag("deleted", &tag.id).unwrap();
    library.set_sticker_caption("deleted", "原配文").unwrap();
    library
        .conn
        .execute(
            "UPDATE stickers SET deleted_at = 10 WHERE id = 'deleted'",
            [],
        )
        .unwrap();
    for id in ["deleted", "missing"] {
        assert!(matches!(
            library.add_sticker_tag(id, &tag.id),
            Err(Error::StickerUnavailable)
        ));
        assert!(matches!(
            library.remove_sticker_tag(id, &tag.id),
            Err(Error::StickerUnavailable)
        ));
        assert!(matches!(
            library.set_sticker_caption(id, "修改"),
            Err(Error::StickerUnavailable)
        ));
        assert!(matches!(
            library.create_tag_and_add(id, "不应新建"),
            Err(Error::StickerUnavailable)
        ));
    }
    assert_eq!(
        library.sticker("deleted").unwrap().caption.as_deref(),
        Some("原配文")
    );
    assert_eq!(library.sticker("deleted").unwrap().tags, [tag]);
    assert_eq!(library.tag_sidebar().unwrap().ungrouped.len(), 1);
}

#[test]
fn rejects_missing_tags_and_groups_and_preserves_existing_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let tag = library.create_tag("开心", None, None).unwrap();
    assert!(matches!(
        library.rename_tag("missing", "新名称"),
        Err(Error::TagUnavailable)
    ));
    assert!(matches!(
        library.set_tag_color("missing", TAG_COLORS[0]),
        Err(Error::TagUnavailable)
    ));
    assert!(matches!(
        library.move_tag("missing", None),
        Err(Error::TagUnavailable)
    ));
    assert!(matches!(
        library.delete_tag("missing"),
        Err(Error::TagUnavailable)
    ));
    assert!(matches!(
        library.add_sticker_tag("one", "missing"),
        Err(Error::TagUnavailable)
    ));
    assert!(matches!(
        library.remove_sticker_tag("one", "missing"),
        Err(Error::TagUnavailable)
    ));
    assert!(matches!(
        library.create_tag("无效分组", None, Some("missing")),
        Err(Error::TagGroupUnavailable)
    ));
    assert!(matches!(
        library.move_tag(&tag.id, Some("missing")),
        Err(Error::TagGroupUnavailable)
    ));
    assert!(matches!(
        library.rename_tag_group("missing", "新名称"),
        Err(Error::TagGroupUnavailable)
    ));
    assert!(matches!(
        library.delete_tag_group("missing"),
        Err(Error::TagGroupUnavailable)
    ));
    assert_eq!(library.tag_sidebar().unwrap().ungrouped[0].tag, tag);
    assert!(library.sticker("one").unwrap().tags.is_empty());
}

#[test]
fn creating_and_adding_is_atomic_and_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let tag = library.create_tag_and_add("one", "  回复 ").unwrap();
    assert_eq!(
        library.sticker("one").unwrap().tags,
        std::slice::from_ref(&tag)
    );
    assert_eq!(library.overview().unwrap().untagged, 2);
    assert!(matches!(
        library.create_tag_and_add("two", "回复"),
        Err(Error::DuplicateName("标签"))
    ));
    // 模拟第二次写入失败，确认新建标签也会撤销。
    library.conn.execute_batch(
        "CREATE TRIGGER fail_add BEFORE INSERT ON sticker_tags BEGIN SELECT RAISE(ABORT, 'test'); END;",
    ).unwrap();
    assert!(library.create_tag_and_add("two", "应撤销").is_err());
    assert_eq!(library.tag_sidebar().unwrap().ungrouped.len(), 1);
    drop(library);
    let library = Library::open(dir.path()).unwrap();
    assert_eq!(library.sticker("one").unwrap().tags, [tag]);
    assert!(library.sticker("two").unwrap().tags.is_empty());
}

#[test]
fn library_view_wire_format_preserves_fixed_views_and_accepts_tag_ids() {
    for value in ["all", "recent", "favorites", "untagged", "recentlyDeleted"] {
        let view: LibraryView = serde_json::from_value(serde_json::json!(value)).unwrap();
        assert!(!matches!(view, LibraryView::Tag(_)));
    }
    let view: LibraryView =
        serde_json::from_value(serde_json::json!({ "tag": "chosen-id" })).unwrap();
    assert!(matches!(view, LibraryView::Tag(id) if id == "chosen-id"));
    assert!(serde_json::from_value::<LibraryView>(serde_json::json!({ "tag": 42 })).is_err());
}
