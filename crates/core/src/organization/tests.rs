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

fn mark_deleted(library: &Library) {
    library
        .conn
        .execute(
            "UPDATE stickers SET deleted_at = 10 WHERE id = 'deleted'",
            [],
        )
        .unwrap();
}

#[test]
fn creates_renames_recolors_and_deletes_categories() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let category = library
        .create_category("  开心  ", Some(CATEGORY_COLORS[2]))
        .unwrap();
    assert_eq!(
        (category.name.as_str(), category.color.as_str()),
        ("开心", CATEGORY_COLORS[2])
    );
    library.rename_category(&category.id, "  Happy ").unwrap();
    library.set_category_color(&category.id, "#3e7bfa").unwrap();
    let listed = library.categories().unwrap();
    let renamed = &listed[0].category;
    assert_eq!(
        (renamed.name.as_str(), renamed.color.as_str(), renamed.position),
        ("Happy", CATEGORY_COLORS[3], category.position)
    );
    library.delete_category(&category.id).unwrap();
    assert!(library.categories().unwrap().is_empty());
}

#[test]
fn rejects_empty_duplicate_reserved_and_unsafe_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let category = library.create_category(" Hello ", None).unwrap();
    let other = library.create_category("World", None).unwrap();
    for name in ["", " ", "\t\n", "\u{3000}"] {
        assert!(matches!(
            library.create_category(name, None),
            Err(Error::EmptyName("分类"))
        ));
        assert!(matches!(
            library.rename_category(&category.id, name),
            Err(Error::EmptyName("分类"))
        ));
    }
    for name in ["Hello", "hello", "HELLO", " Hello ", "\u{3000}hElLo\t"] {
        assert!(matches!(
            library.create_category(name, None),
            Err(Error::DuplicateName("分类"))
        ));
        assert!(matches!(
            library.rename_category(&other.id, name),
            Err(Error::DuplicateName("分类"))
        ));
    }
    for name in [UNCATEGORIZED, " 未分类 "] {
        assert!(matches!(
            library.create_category(name, None),
            Err(Error::ReservedCategoryName)
        ));
        assert!(matches!(
            library.rename_category(&other.id, name),
            Err(Error::ReservedCategoryName)
        ));
    }
    for name in [".hidden", "a/b", "a\\b", "/"] {
        assert!(matches!(
            library.create_category(name, None),
            Err(Error::InvalidCategoryName)
        ));
        assert!(matches!(
            library.rename_category(&other.id, name),
            Err(Error::InvalidCategoryName)
        ));
    }
    // 自身改大小写合法。
    library.rename_category(&category.id, " HELLO ").unwrap();
    assert_eq!(library.categories().unwrap()[0].category.name, "HELLO");
    library.create_category("今天' OR 1=1 --", None).unwrap();
}

#[test]
fn assigns_only_palette_colors_in_order_and_wraps() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    for i in 0..10 {
        let category = library
            .create_category(&format!("分类{i}"), None)
            .unwrap();
        assert_eq!(category.color, CATEGORY_COLORS[i % CATEGORY_COLORS.len()]);
        assert_eq!(category.position, i as i64);
    }
    let explicit = library
        .create_category("指定颜色", Some(CATEGORY_COLORS[7]))
        .unwrap();
    assert_eq!(explicit.color, CATEGORY_COLORS[7]);
    for color in ["", "red", "#FFFFFF", "#F49AC2"] {
        assert!(matches!(
            library.create_category("无效颜色", Some(color)),
            Err(Error::InvalidCategoryColor)
        ));
        assert!(matches!(
            library.set_category_color(&explicit.id, color),
            Err(Error::InvalidCategoryColor)
        ));
    }
    assert_eq!(library.categories().unwrap().len(), 11);
}

#[test]
fn a_sticker_has_one_category_and_setting_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let one = library.create_category("开心", None).unwrap();
    let two = library.create_category("回复", None).unwrap();
    library.set_sticker_category("one", Some(&one.id)).unwrap();
    library.set_sticker_category("one", Some(&one.id)).unwrap();
    assert_eq!(library.sticker("one").unwrap().category, Some(one.clone()));
    assert_eq!(library.categories().unwrap()[0].sticker_count, 1);
    library.set_sticker_category("one", Some(&two.id)).unwrap();
    assert_eq!(library.sticker("one").unwrap().category, Some(two));
    assert_eq!(library.categories().unwrap()[0].sticker_count, 0);
    assert_eq!(library.overview().unwrap().uncategorized, 2);
    library.set_sticker_category("one", None).unwrap();
    assert_eq!(library.overview().unwrap().uncategorized, 3);
    assert_eq!(
        library
            .list_stickers(LibraryView::Uncategorized)
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn deleting_a_category_moves_all_its_stickers_to_uncategorized() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let category = library.create_category("开心", None).unwrap();
    let ids = ["one", "two", "deleted"].map(String::from);
    library
        .set_stickers_category(&ids, Some(&category.id))
        .unwrap();
    mark_deleted(&library);
    assert_eq!(library.overview().unwrap().uncategorized, 0);
    library.delete_category(&category.id).unwrap();
    assert_eq!(library.overview().unwrap().uncategorized, 2);
    for id in ids {
        assert!(library.sticker(&id).unwrap().category.is_none());
    }
    assert!(
        library
            .list_stickers(LibraryView::Category(category.id))
            .unwrap()
            .is_empty()
    );
    assert!(library.categories().unwrap().is_empty());
}

#[test]
fn categories_are_ordered_by_position_and_exclude_deleted_from_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let shared = library.create_category("共享", None).unwrap();
    let unused = library.create_category("未使用", None).unwrap();
    let earlier = library.create_category("较早", None).unwrap();
    for id in ["one", "two", "deleted"] {
        library.set_sticker_category(id, Some(&shared.id)).unwrap();
    }
    mark_deleted(&library);
    library
        .conn
        .execute(
            "UPDATE categories SET position = -1 WHERE id = ?1",
            [&earlier.id],
        )
        .unwrap();
    let listed = library.categories().unwrap();
    let order = listed
        .iter()
        .map(|c| c.category.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(order, [earlier.id, shared.id, unused.id]);
    assert_eq!((listed[1].sticker_count, listed[1].total_sticker_count), (2, 3));
    assert_eq!(listed[2].sticker_count, 0);
}

#[test]
fn category_view_lists_only_active_matches_with_category_and_caption() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let category = library.create_category("回复", None).unwrap();
    for id in ["one", "two", "deleted"] {
        library.set_sticker_category(id, Some(&category.id)).unwrap();
    }
    library.set_sticker_caption("two", "  收到  ").unwrap();
    mark_deleted(&library);
    let stickers = library
        .list_stickers(LibraryView::Category(category.id.clone()))
        .unwrap();
    assert_eq!(
        stickers
            .iter()
            .map(|sticker| sticker.id.as_str())
            .collect::<Vec<_>>(),
        ["two", "one"]
    );
    assert_eq!(stickers[0].category, Some(category));
    assert_eq!(stickers[0].caption.as_deref(), Some("收到"));
    assert!(
        library
            .list_stickers(LibraryView::Category("' OR 1=1 --".into()))
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
fn deleted_and_missing_stickers_reject_edits_without_partial_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let category = library.create_category("开心", None).unwrap();
    library
        .set_sticker_category("deleted", Some(&category.id))
        .unwrap();
    library.set_sticker_caption("deleted", "原配文").unwrap();
    mark_deleted(&library);
    for id in ["deleted", "missing"] {
        assert!(matches!(
            library.set_sticker_category(id, None),
            Err(Error::StickerUnavailable)
        ));
        assert!(matches!(
            library.set_sticker_caption(id, "修改"),
            Err(Error::StickerUnavailable)
        ));
    }
    assert_eq!(
        library.sticker("deleted").unwrap().caption.as_deref(),
        Some("原配文")
    );
    assert_eq!(library.sticker("deleted").unwrap().category, Some(category));
}

#[test]
fn rejects_missing_categories_and_preserves_existing_data() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let category = library.create_category("开心", None).unwrap();
    assert!(matches!(
        library.rename_category("missing", "新名称"),
        Err(Error::CategoryUnavailable)
    ));
    assert!(matches!(
        library.set_category_color("missing", CATEGORY_COLORS[0]),
        Err(Error::CategoryUnavailable)
    ));
    assert!(matches!(
        library.delete_category("missing"),
        Err(Error::CategoryUnavailable)
    ));
    assert!(matches!(
        library.set_sticker_category("one", Some("missing")),
        Err(Error::CategoryUnavailable)
    ));
    assert!(matches!(
        library.set_stickers_category(&["one".into()], Some("missing")),
        Err(Error::CategoryUnavailable)
    ));
    assert_eq!(library.categories().unwrap()[0].category, category);
    assert!(library.sticker("one").unwrap().category.is_none());
}

#[test]
fn creating_and_setting_is_atomic_and_survives_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    let (category, count) = library
        .create_category_and_set_stickers(&["one".into()], "  回复 ")
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(library.sticker("one").unwrap().category, Some(category.clone()));
    assert_eq!(library.overview().unwrap().uncategorized, 2);
    // 同名分类复用，不报重名。
    let (same, count) = library
        .create_category_and_set_stickers(&["two".into()], "回复")
        .unwrap();
    assert_eq!((same.id, count), (category.id.clone(), 1));
    // 模拟写入失败，确认新建分类也会撤销。
    library.conn.execute_batch(
        "CREATE TRIGGER fail_set BEFORE UPDATE OF category_id ON stickers WHEN NEW.id='deleted' BEGIN SELECT RAISE(ABORT, 'test'); END;",
    ).unwrap();
    assert!(
        library
            .create_category_and_set_stickers(&["deleted".into()], "应撤销")
            .is_err()
    );
    assert_eq!(library.categories().unwrap().len(), 1);
    drop(library);
    let library = Library::open(dir.path()).unwrap();
    assert_eq!(library.sticker("one").unwrap().category, Some(category));
}

#[test]
fn library_view_wire_format_preserves_fixed_views_and_accepts_category_ids() {
    for value in ["all", "recent", "favorites", "uncategorized", "recentlyDeleted"] {
        let view: LibraryView = serde_json::from_value(serde_json::json!(value)).unwrap();
        assert!(!matches!(view, LibraryView::Category(_)));
    }
    let view: LibraryView =
        serde_json::from_value(serde_json::json!({ "category": "chosen-id" })).unwrap();
    assert!(matches!(view, LibraryView::Category(id) if id == "chosen-id"));
    assert!(serde_json::from_value::<LibraryView>(serde_json::json!({ "category": 42 })).is_err());
}

#[test]
fn batch_set_is_atomic_idempotent_skips_deleted_and_records_operations() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = library(dir.path());
    library.delete_sticker("deleted").unwrap();
    let ids = ["one", "two", "one", "deleted", "missing"].map(String::from);
    let before = library.local_revision().unwrap();
    let (category, count) = library
        .create_category_and_set_stickers(&ids, " 猫猫 ")
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(library.local_revision().unwrap(), before + 3); // 分类 create + 两条表情设置
    let revision = library.local_revision().unwrap();
    assert_eq!(
        library.set_stickers_category(&ids, Some(&category.id)).unwrap(),
        0
    );
    let (same, count) = library
        .create_category_and_set_stickers(&ids, "猫猫")
        .unwrap();
    assert_eq!((same.id, count), (category.id.clone(), 0));
    assert_eq!(library.local_revision().unwrap(), revision);
    assert!(library.sticker("deleted").unwrap().category.is_none());
    assert!(library.set_stickers_category(&ids, Some("missing")).is_err());
    assert!(library.create_category_and_set_stickers(&ids, " ").is_err());
    assert_eq!(library.local_revision().unwrap(), revision);
    library.conn.execute_batch("CREATE TRIGGER reject_second BEFORE UPDATE OF category_id ON stickers WHEN NEW.id='two' BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
    assert!(
        library
            .create_category_and_set_stickers(&ids, "回滚")
            .is_err()
    );
    assert_eq!(library.categories().unwrap().len(), 1);
    assert_eq!(library.local_revision().unwrap(), revision);
}
