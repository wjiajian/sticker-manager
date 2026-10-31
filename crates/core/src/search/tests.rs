use std::time::Instant;

use rusqlite::params;

use super::*;
use crate::LibraryView;

fn insert(library: &Library, id: &str, name: &str, caption: Option<&str>, added_at: i64) {
    library.conn.execute(
        "INSERT INTO stickers (id, hash, ext, name, caption, width, height, byte_size, added_at)
         VALUES (?1, ?1, 'png', ?2, ?3, 1, 1, 1, ?4)",
        params![id, name, caption, added_at],
    ).unwrap();
}

fn ids(library: &Library, query: &str) -> Vec<String> {
    library
        .search_stickers(query)
        .unwrap()
        .into_iter()
        .map(|sticker| sticker.id)
        .collect()
}

#[test]
fn matches_english_case_in_names_captions_and_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    insert(&library, "name", "HeLLo", None, 1);
    insert(&library, "caption", "配文匹配", Some("HELLO"), 2);
    insert(&library, "tag", "标签匹配", None, 3);
    library.create_tag_and_add("tag", "hello").unwrap();
    for query in ["hello", "HELLO", "hElLo", "ell", "ELL"] {
        assert_eq!(ids(&library, query), ["tag", "caption", "name"]);
    }
}

#[test]
fn treats_wildcards_and_escape_characters_literally_in_every_field() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    insert(&library, "decoy", "普通名称", Some("普通配文"), 100);
    library.create_tag_and_add("decoy", "普通标签").unwrap();
    for (i, literal) in ["%", "_", "\\", "\\%_", "'"].into_iter().enumerate() {
        let name = format!("name{i}");
        let caption = format!("caption{i}");
        let tag = format!("tag{i}");
        insert(&library, &name, literal, None, 1);
        insert(&library, &caption, "配文匹配", Some(literal), 2);
        insert(&library, &tag, "标签匹配", None, 3);
        library.create_tag_and_add(&tag, literal).unwrap();
    }
    for (i, literal) in ["%", "_", "\\", "\\%_", "'"].into_iter().enumerate() {
        let matches = ids(&library, literal);
        assert_eq!(
            &matches[..3],
            &[format!("tag{i}"), format!("caption{i}"), format!("name{i}")]
        );
        assert_eq!(matches.len(), if i < 3 { 6 } else { 3 });
        assert!(!matches.iter().any(|id| id == "decoy"));
    }
    assert!(ids(&library, "' OR 1=1 --").is_empty());
}

#[test]
fn requires_every_word_across_fields_and_excludes_recently_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    let happy = library.create_tag("开心", None, None).unwrap();
    let cat = library.create_tag("猫猫", None, None).unwrap();
    for id in ["active", "deleted"] {
        insert(&library, id, "欢迎回家", Some("已经收到啦"), 1);
        library.add_sticker_tag(id, &happy.id).unwrap();
        library.add_sticker_tag(id, &cat.id).unwrap();
    }
    insert(&library, "partial", "开心", Some("没有配文关键词"), 2);
    library
        .conn
        .execute(
            "UPDATE stickers SET deleted_at = 2 WHERE id = 'deleted'",
            [],
        )
        .unwrap();

    for query in [
        "开心 收到",
        " 欢迎\t开心\n收到\u{3000}猫 ",
        "经收",
        "回家",
        "猫猫",
    ] {
        assert_eq!(ids(&library, query), ["active"]);
    }
    assert_eq!(ids(&library, "开心"), ["partial", "active"]);
    for query in ["开心 收到 不存在", "", " ", "\t\n\r\u{3000}"] {
        assert!(ids(&library, query).is_empty());
    }
}

#[test]
fn returns_all_tags_and_the_same_sticker_data_as_listing() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    insert(&library, "one", "表情名称", Some("收到"), 1);
    let group = library.create_tag_group("情绪").unwrap();
    let tag = library
        .create_tag("开心", Some("#F2B705"), Some(&group.id))
        .unwrap();
    library.add_sticker_tag("one", &tag.id).unwrap();
    library.create_tag_and_add("one", "猫猫").unwrap();
    library.record_usage("one").unwrap();

    let searched = library.search_stickers("开心 收到").unwrap();
    assert_eq!(searched[0].tags.len(), 2);
    assert_eq!(
        serde_json::to_value(searched).unwrap(),
        serde_json::to_value(library.list_stickers(LibraryView::All).unwrap()).unwrap()
    );
}

#[test]
fn ranks_the_whole_trimmed_query_then_latest_usage_and_added_time() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    for (id, name, caption, added) in [
        ("exact-name", "hello world", None, 10),
        ("exact-name-new", "HELLO WORLD", None, 30),
        ("exact-caption", "配文完全匹配", Some("Hello World"), 5),
        ("exact-caption-tie", "同次使用时间", Some("HELLO WORLD"), 20),
        ("exact-tag", "标签完全匹配", None, 1),
        ("prefix-name", "Hello World!", None, 40),
        ("prefix-caption", "配文前缀匹配", Some("HELLO WORLD!"), 50),
        ("prefix-tag", "标签前缀匹配", None, 1),
        ("prefix-old", "hello world old", None, 30),
        ("substring", "say hello world!", None, 100),
        ("substring-new", "包含匹配", Some("say hello world!"), 200),
        ("substring-old", "标签包含匹配", None, 100),
    ] {
        insert(&library, id, name, caption, added);
    }
    library
        .create_tag_and_add("exact-tag", "hELLo WoRLD")
        .unwrap();
    library
        .create_tag_and_add("prefix-tag", "hello world tag")
        .unwrap();
    library
        .create_tag_and_add("substring-old", "say hello world!")
        .unwrap();
    library
        .conn
        .execute_batch(
            "INSERT INTO usages VALUES
            ('exact-tag', 'mac', 1, 150), ('exact-tag', 'win', 1, 200),
            ('exact-caption', 'mac', 1, 100), ('exact-caption-tie', 'mac', 1, 100),
            ('prefix-name', 'mac', 1, 1000), ('prefix-tag', 'mac', 1, -5),
            ('substring', 'mac', 1, 2000);",
        )
        .unwrap();

    assert_eq!(
        ids(&library, " \tHeLLo wORLD  "),
        [
            "exact-tag",
            "exact-caption-tie",
            "exact-caption",
            "exact-name-new",
            "exact-name",
            "prefix-name",
            "prefix-tag",
            "prefix-caption",
            "prefix-old",
            "substring",
            "substring-new",
            "substring-old",
        ]
    );
    // 分词时允许多个空白，但档位比较保留搜索词内部的原始空白。
    assert_eq!(
        &ids(&library, "hello   world")[..6],
        [
            "substring",
            "prefix-name",
            "exact-tag",
            "exact-caption-tie",
            "exact-caption",
            "prefix-tag",
        ]
    );
}

#[test]
fn metadata_changes_affect_subsequent_searches() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    insert(&library, "one", "猫猫", Some("收到"), 1);
    let tag = library.create_tag_and_add("one", "开心").unwrap();
    assert_eq!(ids(&library, "开心 收到"), ["one"]);
    library.set_sticker_caption("one", "好的").unwrap();
    assert!(ids(&library, "开心 收到").is_empty());
    assert_eq!(ids(&library, "开心 好的"), ["one"]);
    library.rename_tag(&tag.id, "高兴").unwrap();
    assert!(ids(&library, "开心").is_empty());
    assert_eq!(ids(&library, "高兴"), ["one"]);
    library.remove_sticker_tag("one", &tag.id).unwrap();
    assert!(ids(&library, "高兴").is_empty());
    library.add_sticker_tag("one", &tag.id).unwrap();
    assert_eq!(ids(&library, "高兴"), ["one"]);
}

#[test]
#[ignore = "手动测量一万张表情的搜索耗时"]
fn search_ten_thousand_stickers() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    let transaction = library.conn.transaction().unwrap();
    for (i, name) in [
        "常用", "开心", "无语", "生气", "打工", "猫猫", "熊熊", "回复",
    ]
    .iter()
    .enumerate()
    {
        transaction
            .execute(
                "INSERT INTO tags (id, name, color, position) VALUES (?1, ?2, '#F2B705', ?3)",
                params![i.to_string(), name, i as i64],
            )
            .unwrap();
    }
    {
        let mut sticker = transaction.prepare(
            "INSERT INTO stickers (id, hash, ext, name, caption, width, height, byte_size, added_at)
             VALUES (?1, ?1, 'png', ?2, ?3, 128, 128, 4096, ?4)",
        ).unwrap();
        let mut tag = transaction
            .prepare("INSERT INTO sticker_tags VALUES (?1, ?2)")
            .unwrap();
        let mut usage = transaction
            .prepare("INSERT INTO usages VALUES (?1, ?2, 2, ?3)")
            .unwrap();
        for i in 0..10_000 {
            let id = format!("{i:05}");
            let name = format!("表情{i:05} {}", if i % 10 == 0 { "Happy" } else { "日常" });
            let caption = ["收到", "好的谢谢", "晚安", "100%_完成\\"][i % 4];
            sticker
                .execute(params![id, name, caption, i as i64])
                .unwrap();
            for tag_id in [0, 1 + i % 4]
                .into_iter()
                .chain((i % 2 == 0).then_some(5 + i % 3))
            {
                tag.execute(params![id, tag_id.to_string()]).unwrap();
            }
            if i % 3 == 0 {
                usage.execute(params![id, "mac", i as i64]).unwrap();
                usage.execute(params![id, "win", i as i64 + 1]).unwrap();
            }
        }
    }
    transaction.commit().unwrap();
    assert_eq!(library.overview().unwrap().all, 10_000);

    for query in [
        "表情09999",
        "HAPPY",
        "谢谢",
        "开心 收到",
        "%_",
        "不存在",
        "常用",
    ] {
        let mut elapsed = Vec::new();
        let mut count = 0;
        for _ in 0..10 {
            let start = Instant::now();
            let stickers = library.search_stickers(query).unwrap();
            elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
            count = stickers.len();
        }
        println!(
            "{query:?}: {count} 张；首次 {:.2} ms，平均 {:.2} ms，最大 {:.2} ms（10 次）",
            elapsed[0],
            elapsed.iter().sum::<f64>() / elapsed.len() as f64,
            elapsed.iter().copied().fold(0.0, f64::max),
        );
    }
}
