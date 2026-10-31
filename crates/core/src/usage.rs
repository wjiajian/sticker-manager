use rusqlite::params;

use crate::{Error, Library, Result, time::unix_millis};

impl Library {
    pub fn record_usage(&self, id: &str) -> Result<()> {
        let now = unix_millis()?;
        let changed = self.conn.execute(
            "INSERT INTO usages (sticker_id, device_id, count, last_used_at)
             SELECT id, ?2, 1, ?3 FROM stickers WHERE id = ?1 AND deleted_at IS NULL
             ON CONFLICT(sticker_id, device_id) DO UPDATE SET
                 count = usages.count + 1, last_used_at = excluded.last_used_at",
            params![id, self.device.id, now],
        )?;
        if changed == 0 {
            return Err(Error::StickerUnavailable);
        }
        Ok(())
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE stickers SET favorite = NOT favorite WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(Error::StickerUnavailable);
        }
        Ok(self.sticker(id)?.favorite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LibraryView;

    fn library(root: &std::path::Path) -> Library {
        let library = Library::open(root).unwrap();
        library.conn.execute_batch(
            "INSERT INTO stickers (id, hash, ext, name, width, height, byte_size, added_at) VALUES
                ('one', 'hash1', 'png', '收到', 1, 1, 1, 1),
                ('two', 'hash2', 'gif', '晚安', 1, 1, 1, 2);"
        ).unwrap();
        library
    }

    fn now() -> i64 {
        unix_millis().unwrap()
    }

    #[test]
    fn counts_usage_on_this_device_and_updates_last_used_and_recent_view() {
        let dir = tempfile::tempdir().unwrap();
        let library = library(dir.path());
        let unused = library.sticker("one").unwrap();
        assert_eq!((unused.usage_count, unused.last_used_at), (0, None));
        assert!(
            library
                .list_stickers(LibraryView::Recent)
                .unwrap()
                .is_empty()
        );
        let before = now();
        library.record_usage("one").unwrap();
        let once = library.sticker("one").unwrap();
        assert_eq!(once.usage_count, 1);
        assert!((before..=now()).contains(&once.last_used_at.unwrap()));
        library
            .conn
            .execute("UPDATE usages SET last_used_at = 1", [])
            .unwrap();
        library.record_usage("one").unwrap();
        let twice = library.sticker("one").unwrap();
        assert_eq!(twice.usage_count, 2);
        assert!((before..=now()).contains(&twice.last_used_at.unwrap()));
        let device_id: String = library
            .conn
            .query_row("SELECT device_id FROM usages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(device_id, library.device().id);
        assert_eq!(library.overview().unwrap().recent, 1);
        assert_eq!(
            library.list_stickers(LibraryView::Recent).unwrap()[0].id,
            "one"
        );
    }

    #[test]
    fn aggregates_all_devices_without_changing_other_device_counts() {
        let dir = tempfile::tempdir().unwrap();
        let library = library(dir.path());
        let future = now() + 60_000;
        library
            .conn
            .execute(
                "INSERT INTO usages VALUES ('one', 'other', 7, ?1)",
                [future],
            )
            .unwrap();
        library.record_usage("one").unwrap();
        library.record_usage("one").unwrap();
        library.record_usage("two").unwrap();
        let sticker = library.sticker("one").unwrap();
        assert_eq!(
            (sticker.usage_count, sticker.last_used_at),
            (9, Some(future))
        );
        let other_count: i64 = library
            .conn
            .query_row(
                "SELECT count FROM usages WHERE device_id = 'other'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(other_count, 7);
        assert_eq!(
            library.list_stickers(LibraryView::Recent).unwrap()[0].id,
            "one"
        );
        drop(library);
        assert_eq!(
            Library::open(dir.path())
                .unwrap()
                .sticker("one")
                .unwrap()
                .usage_count,
            9
        );
    }

    #[test]
    fn rejects_deleted_and_missing_stickers_without_changing_usage() {
        let dir = tempfile::tempdir().unwrap();
        let library = library(dir.path());
        library.record_usage("one").unwrap();
        library
            .conn
            .execute("UPDATE stickers SET deleted_at = 1 WHERE id = 'one'", [])
            .unwrap();
        for id in ["one", "missing"] {
            assert!(matches!(
                library.sticker_for_use(id),
                Err(Error::StickerUnavailable)
            ));
            assert!(matches!(
                library.record_usage(id),
                Err(Error::StickerUnavailable)
            ));
            assert!(matches!(
                library.toggle_favorite(id),
                Err(Error::StickerUnavailable)
            ));
        }
        assert_eq!(library.sticker("one").unwrap().usage_count, 1);
        assert_eq!(library.overview().unwrap().recent, 0);
    }

    #[test]
    fn toggles_favorite_and_updates_views_without_recording_usage() {
        let dir = tempfile::tempdir().unwrap();
        let library = library(dir.path());
        assert!(library.toggle_favorite("one").unwrap());
        assert!(library.sticker("one").unwrap().favorite);
        assert_eq!(library.overview().unwrap().favorites, 1);
        assert_eq!(
            library.list_stickers(LibraryView::Favorites).unwrap()[0].id,
            "one"
        );
        assert!(!library.toggle_favorite("one").unwrap());
        assert_eq!(library.overview().unwrap().favorites, 0);
        assert_eq!(library.sticker("one").unwrap().usage_count, 0);
    }
}
